"""Mine regtest blocks independently of wallet funding and RPC failures."""

import json
import os
import time

from rpc import RpcError
from runtime import exclusive_process, write_json

CONFIRMATIONS = 3


def regtest(node):
    info = node.call("getblockchaininfo", timeout=5)
    upgrade = info.get("upgrades", {}).get("37a5165b", {})
    if info.get("chain") != "test" or upgrade.get("activationheight") != 2:
        raise RuntimeError("Mining requires the configured Ironwood regtest")
    return info


class Miner:
    def __init__(self, node, state):
        self.node = node
        self.state = state
        self.last_block = None
        self.last_error = None
        self.confirmation_blocks = 0

    def tick(self, now):
        height = self.node.call("getblockcount", timeout=5)
        pending = self.node.call("getrawmempool", timeout=5)
        if pending:
            self.confirmation_blocks = CONFIRMATIONS

        address_file = self.state / "mining-address.json"
        configuration = (
            json.loads(address_file.read_text()) if address_file.exists() else {}
        )
        confirming = self.confirmation_blocks > 0
        bootstrap = height < configuration.get("bootstrap_until", 0) and not confirming
        if confirming:
            interval = 1
        elif bootstrap:
            interval = 0.1
        else:
            interval = 10

        if self.last_block is not None and now - self.last_block < interval:
            return

        address = configuration.get("address")
        if address:
            self.node.call("generatetoaddress", [1, address], timeout=10)
        else:
            self.node.call("generate", [1], timeout=10)
        self.last_block = now
        self.last_error = None
        self.confirmation_blocks = max(0, self.confirmation_blocks - 1)
        write_json(
            self.state / "mining-status.json",
            {
                "updated_at": time.time(),
                "height": height + 1,
                "pid": os.getpid(),
            },
        )


def run(node, state):
    with exclusive_process(state / "mining.lock"):
        miner = Miner(node, state)
        verified = False
        while True:
            try:
                if not verified:
                    regtest(node)
                    verified = True
                miner.tick(time.monotonic())
            except (OSError, RpcError, ValueError) as error:
                signature = str(error)
                if signature != miner.last_error:
                    print(f"Mining RPC unavailable; retrying: {error}", flush=True)
                miner.last_error = signature
            time.sleep(0.25)


def healthy(state):
    try:
        status = json.loads((state / "mining-status.json").read_text())
        return 0 <= time.time() - status["updated_at"] < 30
    except (OSError, ValueError, KeyError, TypeError):
        return False
