import gzip, hashlib, json, shutil
from pathlib import Path
scratch = Path("/tmp/deadpan-ui-wake-20260926")
repo = Path("/Users/michael/Code/deadpan")
out = repo / "tools/ui-feedback/evidence/2026-09-26-repaint-wake"
out.mkdir(exist_ok=False)
files = ["baseline.json", "context.json", "review.json", "incremental-source.patch", "validate.py", "post-validation.py", "retain.py", "app-harness-tests.log", "app-harness-tests-02.log"]
for folder in ("gate-1", "post-validation"):
    root = scratch / folder
    for source in sorted(root.rglob("*")):
        if source.is_file() and source.suffix in (".log", ".json"):
            files.append(str(source.relative_to(scratch)))
for name in files:
    source = scratch / name
    relative = Path(name)
    compress = source.suffix == ".log" or (source.name == "report.json" and source.parent.name.startswith("ui-"))
    target = out / (str(relative) + ".gz" if compress else relative)
    target.parent.mkdir(parents=True, exist_ok=True)
    data = source.read_bytes()
    target.write_bytes(gzip.compress(data, mtime=0) if compress else data)
summary = dict(source_count=json.loads((scratch / "post-validation/report.json").read_text())["source_count"], original_gate=json.loads((scratch / "gate-1/report.json").read_text()), final_feature_checks=json.loads((scratch / "post-validation/report.json").read_text()))
(out / "verification.json").write_text(json.dumps(summary, indent=2) + "\n")
(out / "README.md").write_text("# Repaint waits and worker timing evidence\n\nSee [the qualification record](../../../../docs/qualification/repaint-wake-2026-09-26.md) for the exact scope, review fix, failed attempts and remaining host checks.\n\nThe original gate and final feature checks retain separate source manifests. Logs and blocked replay reports are gzip-compressed. `incremental-source.patch` identifies this increment against the prior source checkpoint; it is not a patch against Git HEAD. The first app test log retains the corrected pinned-egui API compilation errors. No file here establishes a new GPU latency or aesthetic result.\n")
hashes = {str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.rglob("*")) if p.is_file()}
(out / "sha256.json").write_text(json.dumps(hashes, indent=2) + "\n")
print(json.dumps({"retained_files":len(hashes),"evidence":str(out)}))
