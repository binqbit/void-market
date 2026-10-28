#!/usr/bin/env python3
"""Run pinned local chains or maintain a private, shielded development faucet."""

import argparse
import json
import os
import sqlite3
import subprocess
import time
from contextlib import closing
from pathlib import Path

import faucet
import mining
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        choices=[
            "anvil",
            "zebra",
            "wallet",
            "miner",
            "faucet",
            "health",
            "mining-health",
        ],
    )
    parser.add_argument(
        "--state",
        type=Path,
        default=Path(os.environ.get("LOCALNET_STATE_DIR", "/var/lib/localnet")),
    )
    parser.add_argument("--bin-dir", type=Path, default=Path("/usr/local/bin"))
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument(
        "--allow-insecure-rpc",
        action="store_true",
        help="Allow wallet HTTP RPC on a trusted private network; never publish its port",
    )
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
        mining.run(Rpc(args.node), args.state)
        return
    if args.command == "faucet":
        faucet.run(
            Rpc(args.node),
            Rpc(args.wallet, args.state / "wallet" / ".cookie"),
            args.state,
        )
        return
    if args.command in ("health", "mining-health"):
        healthy = (
            mining.healthy(args.state)
            if args.command == "mining-health"
            else faucet.healthy(args.state)
        )
        raise SystemExit(0 if healthy else 1)
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
        deadline = time.monotonic() + 300
        while True:
            try:
                height = node.call("getblockchaininfo", timeout=5)["blocks"]
                break
            except (OSError, RpcError, ValueError):
                if time.monotonic() >= deadline:
                    raise RuntimeError("Zebra did not become ready")
                time.sleep(1)
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
allow_insecure_remote_bind = {str(args.allow_insecure_rpc).lower()}
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
        with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as connection:
            seeds = connection.execute(
                "SELECT count(*) FROM ext_zallet_keystore_mnemonics"
            ).fetchone()[0]
        if seeds == 0:
            subprocess.run(base + ["generate-mnemonic"], check=True)
        command = base + ["start"]
    os.execv(command[0], command)


if __name__ == "__main__":
    main()
