"""Codec-independent command logs, artifacts, and final file admission."""

from datetime import datetime, timezone
import hashlib
import os
from pathlib import Path
import subprocess
import time


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


class RecordedHarness:
    def __init__(self, work, sanitizers):
        self.work = Path(work)
        self.sanitizers = sanitizers
        self.environment = dict(os.environ)
        self.admitted_files = {}
        self.loaded_paths = {}
        self.process_faults = []
        self.report = {
            "schema_version": 1, "started_utc": datetime.now(timezone.utc).isoformat(),
            "sanitizers": sanitizers, "work_directory": str(work), "result": "failed",
            "commands": [], "cases": [], "assertions": [], "process_faults": self.process_faults,
        }

    def assert_that(self, label, condition, details=None):
        self.report["assertions"].append({"label": label, "passed": bool(condition), "details": details})
        if not condition:
            raise AssertionError(f"{label}: {details}")

    def run(self, argv, *, required=True, timeout=120):
        argv = [str(value) for value in argv]
        number = len(self.report["commands"])
        paths = {name: self.work / f"command-{number:03d}.{name}" for name in ("stdout", "stderr")}
        record = {"argv": argv, "cwd": str(self.work), "timeout_seconds": timeout}
        self.report["commands"].append(record)
        start = time.monotonic()
        timed_out = False
        try:
            with paths["stdout"].open("wb") as stdout, paths["stderr"].open("wb") as stderr:
                try:
                    result = subprocess.run(argv, cwd=self.work, env=self.environment, stdout=stdout,
                                            stderr=stderr, check=False, timeout=timeout)
                    record["exit_code"] = result.returncode
                except subprocess.TimeoutExpired:
                    timed_out = True
                    record["timed_out"] = True
                    record["exit_code"] = None
                    self.process_faults.append({"command": number, "reason": "timeout"})
                except OSError as error:
                    record["exit_code"] = None
                    record["launch_error"] = str(error)
                    self.process_faults.append({"command": number, "reason": "launch error"})
                    raise RuntimeError(f"command {number} could not launch: {error}") from error
        finally:
            record["elapsed_seconds"] = time.monotonic() - start
            record["logs"] = {name: self.artifact(path) for name, path in paths.items() if path.exists()}
        if timed_out:
            raise RuntimeError(f"command {number} timed out; retained logs: {paths['stderr']}")
        if any(path.stat().st_size > 16 * 1024 * 1024 for path in paths.values()):
            self.process_faults.append({"command": number, "reason": "output limit"})
            raise RuntimeError(f"command {number} exceeded the 16 MiB report-read limit")
        try:
            result = subprocess.CompletedProcess(argv, record["exit_code"],
                                                 paths["stdout"].read_text(encoding="utf-8"),
                                                 paths["stderr"].read_text(encoding="utf-8"))
        except UnicodeError as error:
            self.process_faults.append({"command": number, "reason": "invalid UTF-8 output"})
            raise RuntimeError(f"command {number} emitted invalid UTF-8; retained original logs") from error
        if result.returncode < 0 or result.returncode in (86, 87) or any(
            marker in result.stderr for marker in ("ERROR: AddressSanitizer", "UndefinedBehaviorSanitizer",
                                                  "runtime error:", "ERROR: LeakSanitizer")
        ):
            self.process_faults.append({"command": number, "reason": "signal or sanitizer failure"})
        if required and result.returncode != 0:
            raise RuntimeError(f"command {number} failed ({result.returncode}); retained logs: {paths['stderr']}")
        return result

    def artifact(self, path):
        return {"path": str(path), "sha256": digest(path), "bytes": Path(path).stat().st_size}

    def finish_admission(self):
        observations = []
        for name, expected in self.admitted_files.items():
            try:
                actual = digest(Path(name))
                observations.append({"path": name, "expected": expected, "actual": actual, "passed": actual == expected})
            except OSError as error:
                observations.append({"path": name, "passed": False, "error": str(error)})
        self.report["final_file_admission"] = observations
        for name, expected in self.loaded_paths.items():
            try:
                actual = {"resolved": str(Path(name).resolve()), "sha256": digest(Path(name))}
                observations.append({"load_path": name, "expected": expected, "actual": actual, "passed": actual == expected})
            except OSError as error:
                observations.append({"load_path": name, "passed": False, "error": str(error)})
        if any(not row["passed"] for row in observations):
            self.report["result"] = "failed: admitted executable or library changed"
