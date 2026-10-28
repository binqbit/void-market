"""Replenish the local faucet without controlling or interrupting block production."""

import json
import time
from dataclasses import dataclass

from mining import regtest
from rpc import RpcError
from runtime import exclusive_process, write_json

NAME = "Void Market local faucet"
TARGET = 3_000_000_000


@dataclass(frozen=True)
class Balance:
    spendable: int
    pending: int
    coinbase: int


def synchronized(node, wallet):
    status = wallet.call("getwalletstatus", timeout=10)
    tip = status.get("wallet_tip")
    return (
        tip is not None
        and tip == status.get("node_tip")
        and not status.get("locked", True)
        and tip.get("height") == node.call("getblockcount", timeout=5)
    )


def balance(wallet, account):
    accounts = wallet.call("z_getbalances", [1], timeout=10)["accounts"]
    row = next((row for row in accounts if row["account_uuid"] == account), None)
    if row is None:
        raise RuntimeError("Faucet balance is unavailable")
    shielded = row.get("ironwood", {})
    coinbase = row.get("transparent", {}).get("coinbase", {})
    values = [
        shielded.get("spendable", {}).get("valueZat", 0),
        shielded.get("pending", {}).get("valueZat", 0),
        coinbase.get("spendable", {}).get("valueZat", 0),
    ]
    if any(type(value) is not int or value < 0 for value in values):
        raise RuntimeError("Faucet balance contains invalid amounts")
    return Balance(*values)


class Faucet:
    def __init__(self, node, wallet, state):
        self.node = node
        self.wallet = wallet
        self.state = state
        self.initialized = False
        self.account = None
        self.address = None
        self.transparent = None
        self.bootstrap_until = 0
        self.retry_at = 0
        self.last_status = None

    @property
    def operation_file(self):
        return self.state / "shielding.json"

    def initialize(self):
        regtest(self.node)
        accounts = self.wallet.call("z_listaccounts", timeout=10)
        matches = [row for row in accounts if row["name"] == NAME]
        if len(matches) > 1:
            raise RuntimeError("Duplicate local faucet accounts require reconciliation")
        account = (
            matches[0]
            if matches
            else self.wallet.call("z_getnewaccount", [NAME], timeout=10)
        )
        self.account = account["account_uuid"]
        self.address = self.wallet.call(
            "z_getaddressforaccount", [self.account, ["orchard"], 1000000], timeout=10
        )["address"]
        if not self.address.startswith("uregtest1"):
            raise RuntimeError("Faucet receiver is not on regtest")
        account = next(
            row
            for row in self.wallet.call("z_listaccounts", timeout=10)
            if row["account_uuid"] == self.account
        )
        self.transparent = self.wallet.call(
            "z_listunifiedreceivers", [account["addresses"][0]["ua"]], timeout=10
        )["p2pkh"]

        path = self.state / "mining-address.json"
        if path.exists():
            configuration = json.loads(path.read_text())
            if configuration["address"] != self.transparent:
                raise RuntimeError("The retained faucet mining address changed")
            self.bootstrap_until = configuration.get("bootstrap_until", 0)
        else:
            height = self.node.call("getblockcount", timeout=5)
            self.bootstrap_until = height + 160 if height < 200 else 0
            write_json(
                path,
                {"address": self.transparent, "bootstrap_until": self.bootstrap_until},
            )
        self.initialized = True

    def report(self, status, ready=False, error=None):
        record = {"status": status, "ready": ready, "updated_at": time.time()}
        if error is not None:
            record["error"] = error
        write_json(self.state / "faucet-status.json", record)
        if status != self.last_status:
            print(f"Local faucet: {status}", flush=True)
            self.last_status = status
        return status

    def reconcile(self, pending, funds):
        ready = funds.spendable + funds.pending >= TARGET
        if "txid" in pending:
            return self.confirm(pending["txid"], ready)

        if "opid" not in pending:
            before = set(pending.get("known_operations", []))
            candidates = (
                set(self.wallet.call("z_listoperationids", timeout=10)) - before
            )
            if len(candidates) != 1:
                return self.report("reconciling submission", ready)
            pending["opid"] = candidates.pop()
            write_json(self.operation_file, pending)
        operations = self.wallet.call(
            "z_getoperationstatus", [[pending["opid"]]], timeout=10
        )
        if len(operations) != 1:
            return self.report("reconciling operation", ready)
        operation = operations[0]
        status = operation["status"]
        if status == "failed":
            # A terminal failure is retained for diagnosis, then a later attempt may use new inputs.
            write_json(self.state / "last-funding-failure.json", operation)
            self.operation_file.unlink()
            self.retry_at = time.monotonic() + 10
            return self.report("funding failed; will retry", ready)
        if status == "success":
            txid = operation.get("result", {}).get("txid")
            if txid:
                pending["txid"] = txid
                write_json(self.operation_file, pending)
                return self.confirm(txid, ready)
            return self.report("waiting for funding transaction", ready)
        return self.report("funding in progress", ready)

    def confirm(self, txid, ready):
        try:
            receipt = self.node.call("getrawtransaction", [txid, 1], timeout=5)
        except RpcError as error:
            if error.code != -5:
                raise
            return self.report("waiting for funding transaction", ready)

        if receipt.get("confirmations", 0) < 3:
            return self.report("waiting for funding confirmations", ready)
        self.operation_file.unlink()
        return self.report("ready", ready)

    def tick(self):
        if not self.initialized:
            self.initialize()
        if not synchronized(self.node, self.wallet):
            return self.report("synchronizing")
        funds = balance(self.wallet, self.account)
        if self.operation_file.exists():
            pending = json.loads(self.operation_file.read_text())
            return self.reconcile(pending, funds)
        if funds.spendable >= TARGET:
            return self.report("ready", True)
        if funds.spendable + funds.pending >= TARGET:
            return self.report("waiting for pending funds", True)
        if self.node.call("getrawmempool", timeout=5):
            return self.report("waiting for pending transactions")
        if time.monotonic() < self.retry_at:
            return self.report("waiting before funding retry")
        if self.node.call("getblockcount", timeout=5) < self.bootstrap_until:
            return self.report("mining initial funds")
        if funds.coinbase < TARGET:
            return self.report("waiting for mature coinbase funds")

        # Persist the boundary before a mutating RPC. A lost response is reconciled by operation ID.
        known = self.wallet.call("z_listoperationids", timeout=10)
        pending = {"known_operations": known, "created_at": time.time()}
        write_json(self.operation_file, pending)
        try:
            result = self.wallet.call(
                "z_shieldcoinbase",
                [self.transparent, self.address, None, 50],
                timeout=30,
            )
        except RpcError:
            # A rejected RPC created no operation; transport failures retain the ambiguous boundary.
            self.operation_file.unlink()
            self.retry_at = time.monotonic() + 10
            raise
        pending["opid"] = result["opid"]
        write_json(self.operation_file, pending)
        return self.report("funding in progress")


def run(node, wallet, state):
    with exclusive_process(state / "faucet.lock"):
        funding = Faucet(node, wallet, state)
        while True:
            try:
                funding.tick()
            except (OSError, RpcError, ValueError, KeyError, RuntimeError) as error:
                funding.report("funding unavailable; will retry", error=str(error))
            time.sleep(1)


def healthy(state):
    try:
        status = json.loads((state / "faucet-status.json").read_text())
        return (
            status.get("ready") is True and 0 <= time.time() - status["updated_at"] < 30
        )
    except (OSError, ValueError, KeyError, TypeError):
        return False
