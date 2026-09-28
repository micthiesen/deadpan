"""Failure-path checks without compilers, subprocesses, or wall-clock waits."""
import ctypes
import errno
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("pipe_probe", Path(__file__).with_name("check_pipe_inheritance.py"))
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class CleanupTests(unittest.TestCase):
    def membership(self, count, members, error=0):
        groups = object.__new__(driver.DarwinGroups)

        def query(_kind, _pid, buffer, _length):
            ctypes.set_errno(error)
            for index, member in enumerate(members):
                buffer[index] = member
            return count

        groups.list_pids = query
        with patch.object(driver, "observe_owned_exit", return_value=object()):
            return groups.exited_leader_is_alone(42)

    def test_membership_requires_exited_leader_and_valid_complete_list(self):
        size = ctypes.sizeof(ctypes.c_int)
        self.assertTrue(self.membership(0, []))
        self.assertTrue(self.membership(size, [42]))
        self.assertFalse(self.membership(size, [43]))
        self.assertFalse(self.membership(size * 2, [42, 43]))
        for count, error in [(-1, 0), (0, errno.EPERM), (1, 0), (size * 3, 0)]:
            with self.assertRaises(OSError):
                self.membership(count, [], error)
        groups = object.__new__(driver.DarwinGroups)
        with patch.object(driver, "observe_owned_exit", return_value=None):
            self.assertFalse(groups.exited_leader_is_alone(42))

    def test_zombie_only_group_needs_no_signal(self):
        groups = object.__new__(driver.DarwinGroups)
        with patch.object(groups, "exited_leader_is_alone", return_value=True), \
                patch.object(driver.os, "killpg") as signal:
            groups.stop_owned_group(42)
            signal.assert_not_called()

    def test_permission_error_requires_later_membership_confirmation(self):
        groups = object.__new__(driver.DarwinGroups)
        denied = PermissionError(errno.EPERM, "denied")
        with patch.object(groups, "exited_leader_is_alone", side_effect=[False, True]), \
                patch.object(driver.os, "killpg", side_effect=denied) as signal, \
                patch.object(driver.time, "sleep"):
            groups.stop_owned_group(42)
            signal.assert_called_once()
        with patch.object(groups, "exited_leader_is_alone", return_value=False), \
                patch.object(driver.os, "killpg", side_effect=denied), \
                patch.object(driver.time, "monotonic", side_effect=[0, 0, 4]), \
                patch.object(driver.time, "sleep"):
            with self.assertRaises(TimeoutError) as failure:
                groups.stop_owned_group(42)
            self.assertIs(failure.exception.__cause__, denied)

    def test_cleanup_failure_retains_output_exit_and_error_before_raising(self):
        class Process:
            pid = 42
            returncode = None

            def __init__(self, _command, **options):
                options["stdout"].write(b"command output\n")
                options["stderr"].write(b"command diagnostic\n")

            def wait(self, timeout):
                self.returncode = 0

        class FailedCleanup:
            def stop_owned_group(self, _pid):
                raise PermissionError(errno.EPERM, "group denied")

        with tempfile.TemporaryDirectory(prefix="deadpan-pipe-driver-test-") as scratch:
            output = Path(scratch) / "evidence"
            with patch.object(sys, "argv",
                    ["probe", "--output", str(output)]), \
                    patch.object(driver.platform, "system", return_value="Darwin"), \
                    patch.object(driver.platform, "platform", return_value="Darwin fixture"), \
                    patch.object(driver, "DarwinGroups", return_value=FailedCleanup()), \
                    patch.object(driver.subprocess, "Popen", Process), \
                    patch.object(driver, "observe_owned_exit", return_value=object()), \
                    patch.object(driver, "stop_owned_leader") as fallback:
                with self.assertRaisesRegex(RuntimeError, "command cleanup failed"):
                    driver.main()
                fallback.assert_called_once_with(42)
            row = json.loads((output / "result.json").read_text())["commands"][0]
            self.assertEqual(row["returncode"], 0)
            self.assertEqual(row["stdout"], "command output\n")
            self.assertEqual(row["stderr"], "command diagnostic\n")
            self.assertFalse(row["group_cleanup_confirmed"])
            self.assertEqual(row["cleanup_errors"][0]["errno"], errno.EPERM)


if __name__ == "__main__":
    unittest.main()
