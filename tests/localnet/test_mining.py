import json
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

from support import ChainFixture

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/localnet"))

from mining import Miner, healthy


class MiningTests(unittest.TestCase):
    def test_bootstrap_never_accelerates_pending_transactions(self):
        fixture = ChainFixture()
        try:
            with tempfile.TemporaryDirectory() as directory:
                state = Path(directory)
                (state / "mining-address.json").write_text(
                    '{"address":"tmFaucet","bootstrap_until":2000}'
                )
                miner = Miner(fixture, state)
                miner.tick(0)
                miner.tick(0.25)
                miner.tick(0.5)
                miner.tick(1)
                self.assertEqual(fixture.generated, [1, 1])
        finally:
            fixture.close()

    def test_dead_miner_is_not_reported_healthy(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            (state / "mining-status.json").write_text('{"updated_at":0,"height":1000}')
            self.assertFalse(healthy(state))

    def test_confirmations_continue_after_transaction_leaves_mempool(self):
        fixture = ChainFixture()
        try:
            with tempfile.TemporaryDirectory() as directory:
                miner = Miner(fixture, Path(directory))
                miner.tick(0)
                fixture.pending = False

                miner.tick(1)
                miner.tick(2)
                miner.tick(3)
                self.assertEqual(fixture.generated, [1, 1, 1])

                miner.tick(12)
                self.assertEqual(fixture.generated, [1, 1, 1, 1])
        finally:
            fixture.close()

    def test_failed_faucet_cannot_stop_pending_payment_blocks(self):
        fixture = ChainFixture()
        try:
            with tempfile.TemporaryDirectory() as directory:
                state = Path(directory)
                (state / "wallet").mkdir()
                (state / "wallet/.cookie").write_text("test:test")
                (state / "shielding.json").write_text('{"opid":"refill"}')
                with (state / "miner.log").open("w+") as log:
                    arguments = [
                        "--state",
                        str(state),
                        "--node",
                        fixture.url,
                        "--wallet",
                        fixture.url,
                    ]
                    process = subprocess.Popen(
                        [
                            sys.executable,
                            str(ROOT / "scripts/localnet/node.py"),
                            "miner",
                            *arguments,
                        ],
                        stdout=log,
                        stderr=log,
                    )
                    faucet_process = subprocess.Popen(
                        [
                            sys.executable,
                            str(ROOT / "scripts/localnet/node.py"),
                            "faucet",
                            *arguments,
                        ],
                        stdout=log,
                        stderr=log,
                    )
                    try:
                        deadline = time.monotonic() + 5
                        while (
                            time.monotonic() < deadline
                            and process.poll() is None
                            and len(fixture.generated) < 3
                        ):
                            time.sleep(0.05)
                        log.seek(0)
                        self.assertIsNone(process.poll(), log.read())
                        self.assertIsNone(faucet_process.poll())
                        failure = json.loads(
                            (state / "last-funding-failure.json").read_text()
                        )
                        self.assertEqual(failure["status"], "failed")
                        self.assertGreaterEqual(len(fixture.generated), 3)
                        self.assertEqual(
                            max(fixture.generated),
                            1,
                            "Funding must not fast-forward pending transactions",
                        )
                        self.assertEqual(fixture.shield_requests, 0)
                    finally:
                        for operator in (process, faucet_process):
                            if operator.poll() is None:
                                operator.terminate()
                                operator.wait(timeout=5)
        finally:
            fixture.close()


if __name__ == "__main__":
    unittest.main()
