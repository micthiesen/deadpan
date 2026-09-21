import os
import signal
import sys
import tempfile
import time
import unittest

from build_sanitized import run_test_command


class HarnessProcessTests(unittest.TestCase):
    def run_command(self, command, timeout=5):
        with tempfile.TemporaryFile(mode="w+") as log:
            result = run_test_command(command, cwd=None, env=os.environ, log=log, timeout=timeout)
            log.seek(0)
            return result, log.read()

    def test_success_keeps_output(self):
        result, output = self.run_command([sys.executable, "-c", "print('checked')"])
        self.assertEqual(result, (0, None))
        self.assertEqual(output.strip(), "checked")

    def test_failed_check_keeps_exit_status(self):
        result, _ = self.run_command([sys.executable, "-c", "raise SystemExit(7)"])
        self.assertEqual(result, (7, None))

    def test_missing_executable_is_recorded(self):
        result, _ = self.run_command(["/missing/deadpan-harness-command"])
        self.assertEqual(result[0], 1)
        self.assertIn("failed to start", result[1])

    def test_timeout_stops_and_reaps_the_process(self):
        start = time.monotonic()
        result, _ = self.run_command([sys.executable, "-c", "import time; time.sleep(30)"], timeout=0.05)
        self.assertEqual(result[0], -signal.SIGKILL)
        self.assertIn("exceeded 0.05 seconds", result[1])
        self.assertLess(time.monotonic() - start, 5)


if __name__ == "__main__":
    unittest.main()
