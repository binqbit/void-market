import json
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/localnet"))

import faucet
import mining


class HealthTests(unittest.TestCase):
    def test_funded_wallet_cannot_mask_a_stopped_miner(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            (state / "faucet-status.json").write_text(
                json.dumps({"ready": True, "updated_at": time.time()})
            )
            (state / "mining-status.json").write_text(
                json.dumps({"height": 1000, "updated_at": time.time() - 60})
            )
            self.assertTrue(faucet.healthy(state))
            self.assertFalse(mining.healthy(state))


if __name__ == "__main__":
    unittest.main()
