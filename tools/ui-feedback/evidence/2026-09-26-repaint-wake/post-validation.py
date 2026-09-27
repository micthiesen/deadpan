import hashlib, json, os, subprocess, time
from pathlib import Path
repo = Path("/Users/michael/Code/deadpan")
scratch = Path("/tmp/deadpan-ui-wake-20260926")
out = scratch / "post-validation"
out.mkdir(exist_ok=False)
old = json.loads((scratch / "gate-1/source-after.json").read_text())
def seal():
    return {p: hashlib.sha256((repo / p).read_bytes()).hexdigest() for p in old}
before = seal()
changed = [p for p in old if before[p] != old[p]]
assert set(changed) == {"crates/deadpan-app/src/preview/harness.rs", "crates/deadpan-app/src/preview/harness/wake.rs"}, changed
(out / "source-before.json").write_text(json.dumps(before, indent=2) + "\n")
commands = [
    ["cargo", "fmt", "--all", "--", "--check"],
    ["cargo", "clippy", "-p", "deadpan-app", "--features", "ui-harness", "--all-targets", "--locked", "--", "-D", "warnings"],
    ["cargo", "test", "-p", "deadpan-app", "--features", "ui-harness", "--all-targets", "--locked"],
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
report = dict(commands=records, source_count=len(after), source_unchanged=before == after, changed_since_full_gate=changed, default_feature_sources_unchanged=True)
(out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k:v for k,v in report.items() if k != "commands"}), flush=True)
