import pathlib
import subprocess
import sys

scratch = pathlib.Path(__file__).resolve().parent
checks = [
    ("format-final-source", ["cargo", "fmt", "--all"]),
    ("clippy-final-core-audio", ["cargo", "clippy", "--locked", "-p", "deadpan-core", "-p", "deadpan-audio", "--all-targets", "--", "-D", "warnings"]),
    ("core-plan-audio-playback-store-final", ["cargo", "test", "--locked", "-p", "deadpan-core", "-p", "deadpan-plan", "-p", "deadpan-audio", "-p", "deadpan-playback", "-p", "deadpan-store"]),
    ("native-pauses", ["cargo", "test", "--locked", "-p", "deadpan-app", "--bin", "deadpan-app", "project::tests::pause::", "--", "--nocapture"]),
    ("debug-export", ["env", "DEADPAN_PREVIEW_EXPORT_KEEP=1", "DEADPAN_OCCURRENCE_PAUSE_RESULTS=" + str(scratch / "debug-export-result.json"), "cargo", "test", "--locked", "-p", "deadpan-cli", "--test", "preview_export", "occurrence_pause", "--", "--nocapture"]),
]
for name, command in checks:
    result = subprocess.run([sys.executable, str(scratch / "run.py"), name, *command])
    if result.returncode:
        sys.exit(result.returncode)
