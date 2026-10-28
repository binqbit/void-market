import json
import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/localnet"))

import compose


class ComposeIdentityTests(unittest.TestCase):
    def test_failed_manifest_save_preserves_the_previous_configuration(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            path = state / "compose.env"
            values = {"LOCAL_UID": str(os.getuid()), "LOCAL_GID": str(os.getgid())}
            compose.save(path, values)
            original = path.read_bytes()

            with patch.object(compose.os, "replace", side_effect=OSError("Disk error")):
                with self.assertRaises(OSError):
                    compose.save(path, values | {"NODE_ID": "new-node"})

            self.assertEqual(path.read_bytes(), original)
            self.assertEqual(list(state.iterdir()), [path])

    def test_start_recovers_an_identity_created_before_its_export_was_saved(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            secrets = state / "secrets"
            secrets.mkdir()
            identity = state / "identity-parent" / "node"
            identity.mkdir(parents=True)
            (identity / "node.json").write_text(json.dumps({"node_id": "retained-node"}))
            (identity / "signing.key").write_bytes(b"retained signing key")
            (identity / "service.token").write_text("retained service token")
            values = {
                "COMPOSE_PROJECT_NAME": "recovery-test",
                "LOCAL_UID": str(os.getuid()),
                "LOCAL_GID": str(os.getgid()),
                "DATABASE_PASSWORD": "test database password",
                "JWT_SECRET": "test JWT secret",
                "ARBITER_DATABASE_PASSWORD": "test arbiter password",
                "DEPLOYMENT_ID": "test-deployment",
                "NODE_ID": "not-yet-provisioned",
                "ETHEREUM_RPC_PORT": "18545",
                "ORIGIN": "http://localhost:3100",
            }
            for name in (
                "CUSTODY_MASTER_KEY_FILE",
                "ARBITER_SIGNING_KEY_FILE",
                "ARBITER_SERVICE_TOKEN_FILE",
            ):
                path = secrets / name.lower()
                path.touch()
                values[name] = str(path)
            compose.save(state / "compose.env", values)

            with (
                patch.object(sys, "argv", ["compose.py", "up", "--state", str(state)]),
                patch.object(compose.subprocess, "run") as run,
            ):
                compose.main()

            retained = dict(
                line.split("=", 1)
                for line in (state / "compose.env").read_text().splitlines()
            )
            self.assertEqual(retained["NODE_ID"], "retained-node")
            self.assertEqual(
                Path(values["ARBITER_SIGNING_KEY_FILE"]).read_bytes(), b"retained signing key"
            )
            self.assertEqual(
                Path(values["ARBITER_SERVICE_TOKEN_FILE"]).read_text(), "retained service token"
            )
            for call in run.call_args_list:
                self.assertEqual(call.args[0][:2], ["docker", "compose"])

    def test_sudo_start_uses_retained_identity_for_containers_and_generated_files(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            uid, gid = os.getuid(), os.getgid()
            values = {
                "COMPOSE_PROJECT_NAME": "identity-test",
                "LOCAL_UID": str(uid),
                "LOCAL_GID": str(gid),
                "DATABASE_PASSWORD": "test database password",
                "JWT_SECRET": "test JWT secret",
                "ARBITER_DATABASE_PASSWORD": "test arbiter password",
                "DEPLOYMENT_ID": "test-deployment",
                "ETHEREUM_RPC_PORT": "18545",
                "ORIGIN": "http://localhost:3100",
            }
            (state / "secrets").mkdir()
            (state / "identity-parent").mkdir()
            for name in (
                "CUSTODY_MASTER_KEY_FILE",
                "ARBITER_SIGNING_KEY_FILE",
                "ARBITER_SERVICE_TOKEN_FILE",
            ):
                path = state / "secrets" / name.lower()
                path.write_bytes(b"retained test secret")
                values[name] = str(path)
            compose.save(state / "compose.env", values)

            def run(command, **kwargs):
                if command[:2] == ["docker", "compose"]:
                    self.assertEqual(kwargs["env"]["LOCAL_UID"], str(uid))
                    self.assertEqual(kwargs["env"]["LOCAL_GID"], str(gid))
                    self.assertEqual(kwargs["env"]["COMPOSE_PROJECT_NAME"], "identity-test")
                    return

                self.assertEqual(command[:2], ["docker", "run"])
                self.assertEqual(command[command.index("--user") + 1], f"{uid}:{gid}")
                identity = state / "identity-parent" / "node"
                identity.mkdir()
                (identity / "node.json").write_text(json.dumps({"node_id": "test-node"}))
                (identity / "signing.key").write_bytes(b"test signing key")
                (identity / "service.token").write_text("test service token")

            with (
                patch.object(sys, "argv", ["compose.py", "up", "--state", str(state)]),
                patch.dict(
                    os.environ,
                    {
                        "LOCAL_UID": "12345",
                        "LOCAL_GID": "12345",
                        "COMPOSE_PROJECT_NAME": "another-stack",
                        "ETHEREUM_RPC_PORT": "19545",
                    },
                ),
                patch.object(compose.os, "getuid", return_value=0),
                patch.object(compose.os, "getgid", return_value=0),
                patch.object(compose.os, "umask"),
                patch.object(compose, "Rpc") as rpc,
                patch.object(compose.subprocess, "run", side_effect=run),
            ):
                rpc.return_value.call.return_value = ["0x0"] * 5
                compose.main()
                rpc.assert_called_once_with("http://127.0.0.1:19545")

            retained = dict(
                line.split("=", 1)
                for line in (state / "compose.env").read_text().splitlines()
            )
            self.assertEqual(retained["LOCAL_UID"], str(uid))
            self.assertEqual(retained["LOCAL_GID"], str(gid))
            self.assertEqual(retained["NODE_ID"], "test-node")
            for path in (state / "compose.env", state / "secrets" / "node.json"):
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
                self.assertEqual((path.stat().st_uid, path.stat().st_gid), (uid, gid))


if __name__ == "__main__":
    unittest.main()
