import hashlib, json, os, subprocess, time
from pathlib import Path
repo = Path("/Users/michael/Code/deadpan")
out = Path("/tmp/deadpan-ui-wake-20260926/gate-1")
out.mkdir(exist_ok=False)
base = json.loads(Path("/tmp/deadpan-ui-wake-20260926/baseline.json").read_text())["source_hashes"]
paths = sorted(set(base) | {"crates/deadpan-app/src/preview/harness/wake.rs"})
def seal():
    return {p: hashlib.sha256((repo / p).read_bytes()).hexdigest() for p in paths}
before = seal()
(out / "source-before.json").write_text(json.dumps(before, indent=2) + "\n")
commands = [
    ["cargo", "fmt", "--all", "--", "--check"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
    ["cargo", "test", "--workspace", "--locked"],
    ["cargo", "build", "--workspace", "--locked"],
    ["cargo", "run", "-p", "deadpan-cli", "--", "doctor"],
    ["cargo", "clippy", "-p", "deadpan-app", "--features", "ui-harness", "--all-targets", "--locked", "--", "-D", "warnings"],
    ["cargo", "test", "-p", "deadpan-app", "--features", "ui-harness", "--all-targets", "--locked"],
    ["cargo", "run", "-p", "deadpan-app", "--features", "ui-harness", "--locked", "--", "--ui-check", "--scenario", "editing", "--output", str(out / "ui-editing")],
    ["cargo", "run", "-p", "deadpan-app", "--features", "ui-harness", "--release", "--locked", "--", "--ui-check", "--mode", "performance", "--scenario", "edit-latency", "--output", str(out / "ui-performance")],
]
records = []
for i, command in enumerate(commands):
    start = time.monotonic()
    with (out / f"{i}.log").open("w") as log:
        result = subprocess.run(command, cwd=repo, env=dict(os.environ, DEADPAN_FFMPEG_PREFIX="/tmp/deadpan-ui-ffmpeg/prefix"), stdout=log, stderr=subprocess.STDOUT)
    record = dict(command=command, exit_code=result.returncode, seconds=time.monotonic()-start, log=f"{i}.log")
    records.append(record)
    (out / "progress.json").write_text(json.dumps(records, indent=2) + "\n")
    print(json.dumps(record), flush=True)
after = seal()
(out / "source-after.json").write_text(json.dumps(after, indent=2) + "\n")
report = dict(commands=records, source_count=len(after), source_unchanged=before == after, changed_since_baseline=[p for p in paths if base.get(p) != before[p]])
(out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k:v for k,v in report.items() if k != "commands"}), flush=True)
