#!/usr/bin/env python3
"""Run pinned local chains or maintain a private, shielded development faucet."""

import argparse
import json
import os
import sqlite3
import subprocess
import time
from pathlib import Path

from rpc import Rpc, RpcError

UPGRADES = [
    "5ba81b19:1",
    "76b809bb:1",
    "2bb40e60:1",
    "f5b9230b:1",
    "e9ff75a6:1",
    "c2d6d0b4:2",
    "c8e71055:2",
    "4dec4df0:2",
    "5437f330:2",
    "37a5165b:2",
]
FAUCET = "Void Market local faucet"


def wait_for(description, check, timeout=300):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            result = check()
            if result:
                return result
        except (OSError, RpcError, ValueError):
            pass
        time.sleep(1)
    raise RuntimeError(f"Timed out: {description}. Retained state was not reset.")


def synchronized(wallet):
    status = wallet.call("getwalletstatus")
    return (
        status.get("wallet_tip") is not None
        and status.get("wallet_tip") == status.get("node_tip")
        and not status.get("locked", True)
    )


def faucet(wallet):
    matches = [item for item in wallet.call("z_listaccounts") if item["name"] == FAUCET]
    if len(matches) > 1:
        raise RuntimeError("Duplicate faucet accounts require reconciliation")
    account = matches[0] if matches else wallet.call("z_getnewaccount", [FAUCET])
    account_id = account["account_uuid"]
    receiver = wallet.call(
        "z_getaddressforaccount", [account_id, ["orchard"], 1000000]
    )["address"]
    return account_id, receiver


def shielded_balance(wallet, account):
    if not synchronized(wallet):
        return None
    return (
        wallet.call("z_getbalanceforaccount", [account, 1])
        .get("pools", {})
        .get("ironwood", {})
        .get("valueZat", 0)
    )


def fund(node, wallet, state):
    account, address = faucet(wallet)
    item = next(
        item
        for item in wallet.call("z_listaccounts")
        if item["account_uuid"] == account
    )
    transparent = wallet.call("z_listunifiedreceivers", [item["addresses"][0]["ua"]])[
        "p2pkh"
    ]
    # A brand-new wallet needs blocks after its account birthday before reporting a scan tip.
    bootstrapped = node.call("getblockcount") < 102
    if bootstrapped:
        node.call("generatetoaddress", [260, transparent])
    wait_for("wallet synchronization", lambda: synchronized(wallet))
    pending_file = state / "shielding.json"
    if (shielded_balance(wallet, account) or 0) >= 3_000_000_000:
        pending_file.unlink(missing_ok=True)
        return account
    if pending_file.exists():
        pending = json.loads(pending_file.read_text())
    else:
        if not bootstrapped:
            node.call("generatetoaddress", [260, transparent])
        wait_for("funding block scan", lambda: synchronized(wallet))
        # Transparent UTXO discovery can finish after the shielded scan cursor catches up.
        pending = wait_for(
            "spendable coinbase discovery",
            lambda: wallet.call("z_shieldcoinbase", [transparent, address, None, 50]),
        )
        pending_file.write_text(json.dumps(pending))
        pending_file.chmod(0o600)
    operation = pending["opid"]

    def shielded():
        status = wallet.call("z_getoperationstatus", [[operation]])
        if not status:
            return (shielded_balance(wallet, account) or 0) >= 3_000_000_000
        if status[0]["status"] == "failed":
            raise RuntimeError(
                "Retained shielding operation failed; inspect the wallet before retrying"
            )
        return status[0]["status"] == "success"

    wait_for("shielding transaction", shielded)
    node.call("generate", [10])
    wait_for(
        "shielded funds confirmed",
        lambda: (shielded_balance(wallet, account) or 0) >= 3_000_000_000,
    )
    pending_file.unlink(missing_ok=True)
    print("Local faucet funded with confirmed Ironwood notes", flush=True)
    return account


def miner(args):
    node = Rpc(args.node)
    wallet = Rpc(args.wallet, args.state / "wallet" / ".cookie")
    wait_for("Zebra regtest", lambda: node.call("getblockchaininfo")["chain"] == "test")
    wait_for("Zallet RPC", lambda: wallet.call("getwalletstatus"))
    account = fund(node, wallet, args.state)
    tail = 0
    last_block = time.monotonic()
    while True:
        if node.call("getrawmempool"):
            tail = 10
        if tail or time.monotonic() - last_block >= 10:
            node.call("generate", [1])
            tail = max(0, tail - 1)
            last_block = time.monotonic()
        if (
            not tail
            and synchronized(wallet)
            and (shielded_balance(wallet, account) or 0) < 3_000_000_000
        ):
            fund(node, wallet, args.state)
        time.sleep(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command", choices=["anvil", "zebra", "wallet", "miner", "health"]
    )
    parser.add_argument(
        "--state",
        type=Path,
        default=Path(os.environ.get("LOCALNET_STATE_DIR", "/var/lib/localnet")),
    )
    parser.add_argument("--bin-dir", type=Path, default=Path("/usr/local/bin"))
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument(
        "--port", type=int, help="Override the selected node listening port"
    )
    parser.add_argument(
        "--node", default=os.environ.get("ZCASH_NODE_RPC_URL", "http://127.0.0.1:18232")
    )
    parser.add_argument(
        "--wallet",
        default=os.environ.get("ZCASH_WALLET_RPC_URL", "http://127.0.0.1:28232"),
    )
    args = parser.parse_args()
    args.state.mkdir(parents=True, exist_ok=True, mode=0o700)
    args.state = args.state.resolve()
    os.umask(0o077)
    if args.command == "miner":
        miner(args)
        return
    if args.command == "health":
        node = Rpc(args.node)
        wallet = Rpc(args.wallet, args.state / "wallet" / ".cookie")
        info = node.call("getblockchaininfo")
        if info["upgrades"]["37a5165b"]["status"] != "active":
            raise SystemExit(1)
        accounts = [a for a in wallet.call("z_listaccounts") if a["name"] == FAUCET]
        if (
            len(accounts) != 1
            or (shielded_balance(wallet, accounts[0]["account_uuid"]) or 0)
            < 2_000_000_000
        ):
            raise SystemExit(1)
        return
    if args.command == "anvil":
        command = [
            str(args.bin_dir / "anvil"),
            "--host",
            args.host,
            "--port",
            str(args.port or 8545),
            "--chain-id",
            "31337",
            "--mnemonic-seed-unsafe",
            "43137",
            "--state",
            str(args.state / "anvil.json"),
            "--state-interval",
            "2",
            "--silent",
        ]
    elif args.command == "zebra":
        config = args.state / "zebra.toml"
        config.write_text(f'''[network]
network = "Regtest"
listen_addr = "127.0.0.1:0"
[network.testnet_parameters.activation_heights]
BeforeOverwinter = 1
Overwinter = 1
Sapling = 1
Blossom = 1
Heartwood = 1
Canopy = 1
NU5 = 2
NU6 = 2
"NU6.1" = 2
"NU6.2" = 2
"NU6.3" = 2
[state]
cache_dir = {json.dumps(str(args.state / "zebra"))}
[rpc]
listen_addr = "{args.host}:{args.port or 18232}"
enable_cookie_auth = false
[mining]
miner_address = "tmSRd1r8gs77Ja67Fw1JcdoXytxsyrLTPJm"
[tracing]
filter = "warn"
''')
        command = [str(args.bin_dir / "zebrad"), "-c", str(config), "start"]
    else:
        node = Rpc(args.node)
        height = wait_for("Zebra", lambda: node.call("getblockchaininfo"))["blocks"]
        if height < 2:
            node.call("generate", [2 - height])
        wallet = args.state / "wallet"
        wallet.mkdir(mode=0o700, exist_ok=True)
        config = args.state / "zallet.toml"
        validator = args.node.removeprefix("http://").rstrip("/")
        config.write_text(f'''backend = "zaino"
[builder.limits]
orchard_actions = 250
[consensus]
network = "regtest"
regtest_nuparams = {json.dumps(UPGRADES)}
[database]
[external]
[features]
as_of_version = "0.1.0-beta.3"
[features.deprecated]
[features.experimental]
[note_management]
[indexer]
validator_address = {json.dumps(validator)}
[keystore]
encryption_identity = {json.dumps(str(wallet / "identity.txt"))}
[rpc]
bind = ["{args.host}:{args.port or 28232}"]
timeout = 180
''')
        base = [
            str(args.bin_dir / "zallet-zaino"),
            "-d",
            str(wallet),
            "-c",
            str(config),
        ]
        database = wallet / "wallet.db"
        identity = wallet / "identity.txt"
        if database.exists() and not identity.exists():
            raise RuntimeError("Restore the original wallet identity before restarting")
        if not identity.exists():
            subprocess.run(base + ["generate-encryption-identity"], check=True)
        if not database.exists():
            subprocess.run(base + ["init-wallet-encryption"], check=True)
        # Pinned Zallet beta.3 keeps encrypted mnemonics in SQLite, not separate files.
        # Inspect only the count so a restart cannot generate another wallet seed.
        with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as connection:
            seeds = connection.execute(
                "SELECT count(*) FROM ext_zallet_keystore_mnemonics"
            ).fetchone()[0]
        if seeds == 0:
            subprocess.run(base + ["generate-mnemonic"], check=True)
        command = base + ["start"]
    os.execv(command[0], command)


if __name__ == "__main__":
    main()
