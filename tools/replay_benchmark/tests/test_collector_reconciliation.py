from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from test_powershell_contract import powershell

ROOT = Path(__file__).resolve().parent
RUNNER = ROOT.parent / "run.ps1"


class CollectorReconciliationTests(unittest.TestCase):
    def run_harness(self, name: str, timeout: int) -> dict:
        # Keep native/failure logs even if the subprocess fails or is interrupted.
        output = Path(tempfile.mkdtemp(prefix="queueback-collector-"))
        process = subprocess.run(
            [powershell(), "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
             str(ROOT / name), "-RunnerPath", str(RUNNER), "-OutputRoot", str(output)],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=timeout, check=False,
        )
        (output / "stdout.log").write_text(process.stdout, encoding="utf-8")
        (output / "stderr.log").write_text(process.stderr, encoding="utf-8")
        self.assertEqual(process.returncode, 0, f"{output}: {process.stdout}\n{process.stderr}")
        return json.loads(process.stdout)

    def test_actual_collector_deterministic(self):
        result = self.run_harness("collector_deterministic.ps1", 30)
        self.assertGreaterEqual(result["passed"], 35)
        self.assertEqual(result["app_launches"], 0)

    @unittest.skipUnless(os.name == "nt", "Windows Job Objects required")
    def test_native_disposable_job(self):
        result = self.run_harness("collector_native.ps1", 30)
        expected = result["expected_processes"]
        self.assertIn(expected, (17, 18))
        self.assertEqual(result["new_processes"], expected)
        self.assertEqual(result["exit_processes"], expected)
        self.assertEqual(result["final"]["total_processes"], expected)
        self.assertEqual(result["final"]["active_processes"], 0)
        self.assertEqual(result["final"]["unobserved_process_count"], 0)
        self.assertEqual(result["final"]["terminated_processes"], 0)
        self.assertGreater(result["batches"], 1)
        self.assertTrue(result["closed"])


if __name__ == "__main__":
    unittest.main()
