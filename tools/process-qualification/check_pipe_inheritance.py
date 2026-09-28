#!/usr/bin/env python3
"""Exercise the actual Darwin pipe race and Deadpan's production launch boundary."""
import argparse
import ctypes
import errno
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import tempfile
import time


def observe_owned_exit(pid):
    """Never reap: the driver's sole-owned leader pins every later signal."""
    return os.waitid(os.P_PID, pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)


class DarwinGroups:
    """The native adapter's bounded libproc membership check, for this probe."""

    def __init__(self):
        self.library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self.list_pids = self.library.proc_listpids
        self.list_pids.argtypes = [ctypes.c_uint, ctypes.c_uint, ctypes.c_void_p, ctypes.c_int]
        self.list_pids.restype = ctypes.c_int

    def exited_leader_is_alone(self, pid):
        if observe_owned_exit(pid) is None:
            return False
        members = (ctypes.c_int * 2)()
        ctypes.set_errno(0)
        count = self.list_pids(2, pid, members, ctypes.sizeof(members))  # PROC_PGRP_ONLY
        error = ctypes.get_errno()
        if count < 0 or (count == 0 and error != 0):
            raise OSError(error or errno.EIO, "process-group membership query failed")
        if count > ctypes.sizeof(members) or count % ctypes.sizeof(ctypes.c_int):
            raise OSError(errno.EIO, "invalid process-group membership length")
        return count == 0 or (count == ctypes.sizeof(ctypes.c_int) and members[0] == pid)

    def stop_owned_group(self, pid):
        deadline = time.monotonic() + 3
        last_signal_error = None
        while not self.exited_leader_is_alone(pid):
            # The preceding waitid establishes ownership even for a live leader.
            # EPERM can describe an exited member, but never proves cleanup.
            if time.monotonic() >= deadline:
                raise TimeoutError("process-group cleanup was not confirmed") from last_signal_error
            try:
                os.killpg(pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError) as error:
                last_signal_error = error
            time.sleep(0.002)


def stop_owned_leader(pid):
    """Checked fallback only; it cannot turn failed group cleanup into success."""
    deadline = time.monotonic() + 3
    while observe_owned_exit(pid) is None:
        if time.monotonic() >= deadline:
            raise TimeoutError("leader fallback cleanup was not confirmed")
        try:
            os.kill(pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass  # Only the next ownership/exit observation can establish success.
        time.sleep(0.002)


def error_record(error):
    return {"type": type(error).__name__, "message": str(error),
        "errno": getattr(error, "errno", None),
        "cause": None if error.__cause__ is None else str(error.__cause__)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New evidence directory")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("This qualification requires macOS and its dyld interposition support")
    repo = Path(__file__).resolve().parents[2]
    source = repo / "native/deadpan-process/src/spawn.rs"
    fixtures = Path(__file__).resolve().parent / "fixtures"
    inputs = [source, fixtures / "delayed_pipe.c", fixtures / "pipe_inheritance.rs", Path(__file__).resolve()]
    args.output.mkdir(parents=True, exist_ok=False)
    evidence = {"platform": platform.platform(), "sources": {
        str(path.relative_to(repo)): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs
    }, "commands": []}
    (args.output / "result.json").write_text(json.dumps(evidence, indent=2) + "\n")
    groups = DarwinGroups()

    def run(command, env=None):
        # Files keep an inherited output pipe from delaying timeout handling.
        # Keep the group leader unreaped until cleanup, so killpg cannot target
        # a reused PID. Every fixture child stays in this isolated group.
        with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            process = None
            timed_out = False
            command_error = None
            cleanup_errors = []
            group_clean = False
            try:
                process = subprocess.Popen(command, cwd=repo, env=env, stdout=stdout,
                    stderr=stderr, start_new_session=True)
                deadline = time.monotonic() + 30
                while observe_owned_exit(process.pid) is None:
                    if time.monotonic() >= deadline:
                        timed_out = True
                        break
                    time.sleep(0.01)
            except BaseException as error:
                command_error = error
            if process is not None:
                can_reap = False
                try:
                    groups.stop_owned_group(process.pid)
                    group_clean = True
                    can_reap = True
                except BaseException as error:
                    cleanup_errors.append(error_record(error))
                    try:
                        stop_owned_leader(process.pid)
                        can_reap = True
                    except BaseException as fallback_error:
                        cleanup_errors.append(error_record(fallback_error))
                if can_reap:
                    try:
                        process.wait(timeout=3)
                    except BaseException as error:
                        cleanup_errors.append(error_record(error))
            stdout.seek(0)
            stderr.seek(0)
            out = stdout.read().decode("utf-8", errors="replace")
            err = stderr.read().decode("utf-8", errors="replace")
        returncode = None if process is None else process.returncode
        evidence["commands"].append({"argv": command, "returncode": returncode,
            "pid": None if process is None else process.pid,
            "timed_out": timed_out, "stdout": out, "stderr": err,
            "command_error": None if command_error is None else error_record(command_error),
            "group_cleanup_confirmed": group_clean, "cleanup_errors": cleanup_errors})
        print(out, end="")
        if err:
            print(err, end="")
        (args.output / "result.json").write_text(json.dumps(evidence, indent=2) + "\n")
        if command_error is not None:
            raise command_error
        if cleanup_errors:
            raise RuntimeError(f"command cleanup failed: {cleanup_errors}")
        if timed_out:
            raise subprocess.TimeoutExpired(command, 30)
        if returncode:
            raise subprocess.CalledProcessError(returncode, command, out, err)

    with tempfile.TemporaryDirectory(prefix="deadpan-pipe-inheritance-") as scratch:
        scratch = Path(scratch)
        library = scratch / "delayed_pipe.dylib"
        probe = scratch / "pipe_inheritance"
        run(["rustc", "--version", "--verbose"])
        run(["clang", "--version"])
        run(["clang", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
            str(fixtures / "delayed_pipe.c"), "-o", str(library)])
        env = dict(os.environ, DEADPAN_SPAWN_SOURCE=str(source))
        run(["rustc", "--edition=2024", str(fixtures / "pipe_inheritance.rs"), "-o", str(probe)], env)
        for mode in ["raw", "serialized"]:
            root = scratch / mode
            root.mkdir()
            env = dict(os.environ, DYLD_INSERT_LIBRARIES=str(library), DEADPAN_PIPE_PROBE_ROOT=str(root))
            run([str(probe), mode], env)
    evidence["passed"] = True
    (args.output / "result.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    main()
