from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[3]
COLLECTOR = REPOSITORY / "tools" / "capture_benchmark" / "collect.ps1"


@unittest.skipUnless(os.name == "nt", "collector preflight is Windows-only")
class CollectorPreflightTests(unittest.TestCase):
    def test_git_revision_capture_preserves_native_exit_code_before_pipeline(self) -> None:
        source = COLLECTOR.read_text(encoding="utf-8")
        self.assertIn("$revisionOutput = @(& git", source)
        self.assertIn("$revisionExitCode = $LASTEXITCODE", source)
        self.assertIn("if ($revisionExitCode -ne 0", source)

    def test_missing_presentmon_error_is_actionable_and_stable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            missing = Path(temporary) / "PresentMon-2.x.exe"
            result = subprocess.run(
                [
                    "powershell",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(COLLECTOR),
                    "-PresentMonPath",
                    str(missing),
                    "-Condition",
                    "baseline",
                    "-FrameMode",
                    "capped",
                    "-RunNumber",
                    "1",
                    "-ResultRoot",
                    str(Path(temporary) / "results"),
                    "-PreflightOnly",
                ],
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            combined = result.stdout + result.stderr
            self.assertEqual(result.returncode, 2)
            self.assertIn("QB-PERF-PRESENTMON_MISSING", combined)
            self.assertIn("executable was not found", combined)
            self.assertFalse((Path(temporary) / "results").exists())


if __name__ == "__main__":
    unittest.main()
