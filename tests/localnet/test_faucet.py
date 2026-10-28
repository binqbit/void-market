import json
import sys
import tempfile
import unittest
from pathlib import Path

from support import ChainFixture

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/localnet"))

from faucet import Faucet


class FaucetTests(unittest.TestCase):
    def setUp(self):
        self.fixture = ChainFixture()
        self.fixture.pending = False
        self.directory = tempfile.TemporaryDirectory()
        self.state = Path(self.directory.name)
        self.faucet = Faucet(self.fixture, self.fixture, self.state)

    def tearDown(self):
        self.fixture.close()
        self.directory.cleanup()

    def test_pending_change_does_not_trigger_refill_or_generate_blocks(self):
        self.fixture.incoming = 30_000_000_000
        self.assertEqual(self.faucet.tick(), "waiting for pending funds")
        self.assertEqual(self.fixture.shield_requests, 0)
        self.assertEqual(self.fixture.generated, [])
        self.assertTrue(
            json.loads((self.state / "faucet-status.json").read_text())["ready"]
        )

    def test_failed_operation_is_retained_and_next_attempt_is_separate(self):
        (self.state / "shielding.json").write_text('{"opid":"refill"}')
        self.assertEqual(self.faucet.tick(), "funding failed; will retry")
        self.assertFalse((self.state / "shielding.json").exists())
        self.assertEqual(
            json.loads((self.state / "last-funding-failure.json").read_text())[
                "status"
            ],
            "failed",
        )
        self.faucet.retry_at = 0
        self.fixture.operation_status = "executing"
        self.assertEqual(self.faucet.tick(), "funding in progress")
        self.assertEqual(self.faucet.tick(), "funding in progress")
        self.assertEqual(self.fixture.shield_requests, 1)
        self.assertEqual(self.fixture.generated, [])

    def test_unknown_submit_is_reconciled_without_repeating_spend(self):
        (self.state / "shielding.json").write_text('{"known_operations":[]}')
        self.fixture.operation_status = "executing"
        self.assertEqual(self.faucet.tick(), "funding in progress")
        self.assertEqual(self.fixture.shield_requests, 0)
        self.assertEqual(
            json.loads((self.state / "shielding.json").read_text())["opid"], "refill"
        )

    def test_unknown_submit_without_matching_operation_stays_pending(self):
        (self.state / "shielding.json").write_text('{"known_operations":["refill"]}')
        self.assertEqual(self.faucet.tick(), "reconciling submission")
        self.assertEqual(self.fixture.shield_requests, 0)
        self.assertTrue((self.state / "shielding.json").exists())

    def test_confirmations_survive_loss_of_wallet_operation_history(self):
        (self.state / "shielding.json").write_text('{"opid":"refill"}')
        self.fixture.operation_status = "success"
        self.fixture.confirmations = 1
        self.assertEqual(self.faucet.tick(), "waiting for funding confirmations")
        self.assertEqual(
            json.loads((self.state / "shielding.json").read_text())["txid"],
            "funding-transaction",
        )

        self.fixture.operation_known = False
        self.fixture.confirmations = 3
        self.fixture.spendable = 30_000_000_000
        restarted = Faucet(self.fixture, self.fixture, self.state)
        self.assertEqual(restarted.tick(), "ready")
        self.assertFalse((self.state / "shielding.json").exists())
        self.assertEqual(self.fixture.shield_requests, 0)


if __name__ == "__main__":
    unittest.main()
