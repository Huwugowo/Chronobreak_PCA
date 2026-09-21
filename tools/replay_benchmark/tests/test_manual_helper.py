"""Manual watcher rejection evidence; never touches a real app or process."""
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, patch
from contextlib import ExitStack, redirect_stdout

spec = importlib.util.spec_from_file_location(
    'manual_helper', Path(__file__).resolve().parents[1] / 'qb010_manual.py')
manual = importlib.util.module_from_spec(spec)
spec.loader.exec_module(manual)


class ManualWatcherTests(unittest.TestCase):
    def test_hold_window_allows_a_deliberate_manual_overlap(self):
        self.assertEqual(manual.MAX_MANUAL_HOLD_SECONDS, 30)
        with self.assertRaisesRegex(RuntimeError, 'between one and 30 seconds'):
            manual.hold('too-long', 31)

    def rejected_child(self, *, denied=False, identity_error=False):
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
            work = Path(directory)
            app = work / 'app.exe'
            probe = work / 'ffprobe.exe'
            (work / 'launch.json').write_text(json.dumps(dict(pid=1, creation_filetime=10)))
            kernel = MagicMock()
            kernel.OpenProcess.side_effect = [11, 0 if denied else 22]
            kernel.WaitForSingleObject.return_value = 258
            nt = MagicMock()
            stack.enter_context(patch.object(manual, 'WORK', work))
            stack.enter_context(patch.object(manual, 'APP', app))
            stack.enter_context(patch.object(manual, 'PROBE', probe))
            stack.enter_context(patch.object(manual, 'windows', return_value=kernel))
            stack.enter_context(patch.object(manual.ctypes, 'WinDLL', return_value=nt, create=True))
            stack.enter_context(patch.object(manual.ctypes, 'get_last_error', return_value=5, create=True))
            identities = [(app.resolve(), 10), OSError(5, 'identity failed') if identity_error
                          else ((work / 'foreign.exe').resolve(), 20)]
            stack.enter_context(patch.object(manual, 'identity', side_effect=identities))
            stack.enter_context(patch.object(manual, 'children', return_value=[2]))
            stack.enter_context(patch.object(manual.time, 'monotonic', side_effect=[0, 0, 61]))
            stack.enter_context(patch.object(manual.time, 'sleep'))
            with redirect_stdout(io.StringIO()) as output:
                self.assertFalse(manual.hold('rejected', 8))
            receipt = json.loads((work / 'hold-rejected.json').read_text())
            self.assertEqual(receipt['status'], 'not-observed')
            self.assertFalse(receipt['ui_action_observed'])
            self.assertIn('NO HOLD', output.getvalue())
            nt.NtSuspendProcess.assert_not_called()
            kernel.CloseHandle.assert_any_call(11)
            if not denied:
                kernel.CloseHandle.assert_any_call(22)
            return receipt

    def test_wrong_image_is_never_suspended_and_rejection_is_retained(self):
        receipt = self.rejected_child()
        self.assertEqual(receipt['observations']['path_mismatches'], 1)

    def test_open_denial_is_retained_instead_of_silently_skipped(self):
        receipt = self.rejected_child(denied=True)
        self.assertEqual(receipt['observations']['open_errors'], 1)
        self.assertEqual(receipt['failures'], [dict(stage='OpenProcess', pid=2, code=5)])

    def test_identity_failure_closes_handle_and_retains_reason(self):
        receipt = self.rejected_child(identity_error=True)
        self.assertEqual(receipt['observations']['identity_errors'], 1)
        self.assertEqual(receipt['failures'], [dict(stage='process_identity', pid=2, code=5)])


if __name__ == '__main__':
    unittest.main()
