import pathlib
import subprocess
import sys

scratch = pathlib.Path(__file__).resolve().parent
checks = [
    ("release-build", ["cargo", "build", "--locked", "--release", "-p", "deadpan-app", "-p", "deadpan-cli"]),
    ("release-export", ["env", "DEADPAN_PREVIEW_EXPORT_KEEP=1", "DEADPAN_OCCURRENCE_PAUSE_RESULTS=" + str(scratch / "release-export-result.json"), "cargo", "test", "--locked", "--release", "-p", "deadpan-cli", "--test", "preview_export", "occurrence_pause", "--", "--nocapture"]),
    ("fmt-final", ["cargo", "fmt", "--all", "--", "--check"]),
]
for name, command in checks:
    result = subprocess.run([sys.executable, str(scratch / "run.py"), name, *command])
    if result.returncode:
        sys.exit(result.returncode)
