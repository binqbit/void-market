import sqlite3
import sys
import tempfile
import tomllib
import unittest
from contextlib import closing
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/localnet"))

import node


class WalletRpcTests(unittest.TestCase):
    def configuration(self, *arguments):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            wallet = state / "wallet"
            wallet.mkdir()
            (wallet / "identity.txt").write_text("test identity")
            with closing(sqlite3.connect(wallet / "wallet.db")) as database:
                database.execute("CREATE TABLE ext_zallet_keystore_mnemonics (id INTEGER)")
                database.execute("INSERT INTO ext_zallet_keystore_mnemonics VALUES (1)")
                database.commit()

            argv = ["node.py", "wallet", "--state", str(state), *arguments]
            with (
                patch.object(sys, "argv", argv),
                patch.object(node, "Rpc") as rpc,
                patch.object(node.os, "umask"),
                patch.object(node.os, "execv"),
            ):
                rpc.return_value.call.return_value = {"blocks": 2}
                node.main()

            return tomllib.loads((state / "zallet.toml").read_text())["rpc"]

    def test_default_wallet_keeps_loopback_rpc(self):
        rpc = self.configuration()
        self.assertEqual(rpc["bind"], ["127.0.0.1:28232"])
        self.assertFalse(rpc["allow_insecure_remote_bind"])

    def test_non_loopback_host_does_not_implicitly_relax_rpc_policy(self):
        rpc = self.configuration("--host", "0.0.0.0")
        self.assertFalse(rpc["allow_insecure_remote_bind"])

    def test_private_docker_network_requires_explicit_opt_in(self):
        rpc = self.configuration("--host", "0.0.0.0", "--allow-insecure-rpc")
        self.assertEqual(rpc["bind"], ["0.0.0.0:28232"])
        self.assertTrue(rpc["allow_insecure_remote_bind"])


if __name__ == "__main__":
    unittest.main()
