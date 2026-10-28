import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/localnet"))

from compose import prepare_secret_files, set_private_permissions


class ComposeSecretTests(unittest.TestCase):
    def test_migration_preserves_values_and_restart_reuses_files(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            original = {
                "DATABASE_PASSWORD": "retained database password",
                "JWT_SECRET": "retained JWT secret",
                "ARBITER_DATABASE_PASSWORD": "retained arbiter password",
            }
            values = original.copy()
            values.update(LOCAL_UID=str(os.getuid()), LOCAL_GID=str(os.getgid()))
            prepare_secret_files(state, values)

            for name, secret in original.items():
                path = Path(values[name + "_FILE"])
                self.assertEqual(path.read_text(), secret)
                self.assertNotIn(name, values)
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            self.assertEqual(stat.S_IMODE((state / "secrets").stat().st_mode), 0o700)

            retained = values.copy()
            prepare_secret_files(state, values)
            self.assertEqual(values, retained)
            for name, secret in original.items():
                self.assertEqual(Path(values[name + "_FILE"]).read_text(), secret)

    def test_conflicting_retained_password_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            path = state / "database_password"
            path.write_text("original")
            values = {
                "DATABASE_PASSWORD_FILE": str(path),
                "DATABASE_PASSWORD": "changed",
                "LOCAL_UID": str(os.getuid()),
                "LOCAL_GID": str(os.getgid()),
            }

            with self.assertRaisesRegex(RuntimeError, "does not match"):
                prepare_secret_files(state, values)
            self.assertEqual(path.read_text(), "original")

    def test_missing_retained_secret_is_not_regenerated(self):
        with tempfile.TemporaryDirectory() as directory:
            values = {"LOCAL_UID": str(os.getuid()), "LOCAL_GID": str(os.getgid())}
            with self.assertRaisesRegex(RuntimeError, "Restore the retained"):
                prepare_secret_files(Path(directory), values)

    def test_secret_ownership_matches_retained_container_user(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            uid, gid = os.getuid() + 1, os.getgid() + 1
            values = {
                "LOCAL_UID": str(uid),
                "LOCAL_GID": str(gid),
                "DATABASE_PASSWORD": "retained database password",
                "JWT_SECRET": "retained JWT secret",
                "ARBITER_DATABASE_PASSWORD": "retained arbiter password",
            }

            # The caller creates files under its own UID, as sudo does on migration.
            # Verify ownership is assigned to the retained runtime identity.
            with patch("compose.os.chown") as chown:
                prepare_secret_files(state, values)

            for name in ("database_password", "jwt_secret", "arbiter_database_password"):
                chown.assert_any_call(state / "secrets" / name, uid, gid)
            chown.assert_any_call(state / "secrets", uid, gid)

            # Repair a retained file with the same mismatch without rotating it.
            with patch("compose.os.chown") as chown:
                prepare_secret_files(state, values)
            chown.assert_any_call(state / "secrets" / "database_password", uid, gid)
            self.assertEqual(
                (state / "secrets" / "database_password").read_text(),
                "retained database password",
            )

    def test_private_file_owner_is_repaired_without_changing_contents(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "node.json"
            path.write_text("retained provisioning record")
            uid, gid = os.getuid() + 1, os.getgid() + 1

            with patch("compose.os.chown") as chown:
                set_private_permissions(
                    path, {"LOCAL_UID": str(uid), "LOCAL_GID": str(gid)}
                )

            chown.assert_called_once_with(path, uid, gid)
            self.assertEqual(path.read_text(), "retained provisioning record")
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)


if __name__ == "__main__":
    unittest.main()
