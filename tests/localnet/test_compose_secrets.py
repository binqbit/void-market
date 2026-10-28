import stat
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/localnet"))

from compose import prepare_secret_files


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
            values = {"DATABASE_PASSWORD_FILE": str(path), "DATABASE_PASSWORD": "changed"}

            with self.assertRaisesRegex(RuntimeError, "does not match"):
                prepare_secret_files(state, values)
            self.assertEqual(path.read_text(), "original")

    def test_missing_retained_secret_is_not_regenerated(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "Restore the retained"):
                prepare_secret_files(Path(directory), {})


if __name__ == "__main__":
    unittest.main()
