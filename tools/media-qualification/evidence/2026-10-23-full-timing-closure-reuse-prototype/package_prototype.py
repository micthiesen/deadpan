#!/usr/bin/env python3
"""Gated packager for the private full-timing-closure reuse prototype.

Root runs this after all build and measurement producers are terminal. This
records a five-pair diagnostic only; it does not claim production adoption.
"""
from __future__ import annotations
import difflib
import gzip
import hashlib
import importlib.util
import json
import re
import shutil
from pathlib import Path

ROOT = Path("/tmp/deadpan-full-timing-closure-reuse-2026-10-23")
DEST = Path("/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-full-timing-closure-reuse-prototype")
STAGE = DEST.with_name(DEST.name + ".staging")
SOURCE = ROOT / "source"
CONTROL_MANIFEST = Path("/tmp/deadpan-compact-json-size-qualification-2026-10-23/release-native-binaries.json")
CHANGED = ["crates/deadpan-core/src/audio_reference.rs"]
BUILD_STEPS = [
    "clone-release-cache", "format", "focused-reuse-tests", "scoped-capture-tests",
    "command-work-tests", "insert-time-tests", "release-build",
]
TEST_STEPS = BUILD_STEPS[2:6]
PAIR_ORDER = [
    (1, "control"), (1, "prototype"),
    (2, "prototype"), (2, "control"),
    (3, "control"), (3, "prototype"),
    (4, "prototype"), (4, "control"),
    (5, "control"), (5, "prototype"),
]
SIDECARS = {"project.sqlite-shm", "project.sqlite-wal"}

def read_json(path):
    return json.loads(path.read_text())

def sha(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

def require(ok, why):
    if not ok:
        raise RuntimeError(why)

def copy_payload(src, rel, compress_large=True):
    require(src.is_file(), f"missing required payload: {src}")
    dst = STAGE / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    raw = src.read_bytes()
    if compress_large and len(raw) > 100_000 and src.suffix in {".json", ".txt", ".log", ".html"}:
        with dst.with_suffix(dst.suffix + ".gz").open("wb") as stream:
            with gzip.GzipFile(filename="", mode="wb", compresslevel=9, fileobj=stream, mtime=0) as gz:
                gz.write(raw)
    else:
        shutil.copyfile(src, dst)

def copy_tree(src, rel):
    require(src.is_dir(), f"missing report directory: {src}")
    paths = [p for p in src.rglob("*") if p.is_file()]
    require(paths, f"empty report directory: {src}")
    for p in paths:
        copy_payload(p, str(Path(rel) / p.relative_to(src)))

def write_json(value, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")

def main():
    require(not DEST.exists(), f"destination already exists: {DEST}")
    require(not STAGE.exists(), f"staging destination already exists: {STAGE}")

    build = read_json(ROOT / "build-receipt.json")
    require([row["name"] for row in build] == BUILD_STEPS, "build receipt has missing, extra, or reordered steps")
    require(all(row.get("exit") == 0 for row in build), "a build or focused test step failed")

    bins = read_json(ROOT / "binaries.json")
    require(bins.get("deadpan-app", {}).get("sha256"), "missing prototype app binary identity")
    for name, row in bins.items():
        path = Path(row["executed"])
        require(path.is_file() and sha(path) == row["sha256"], f"built binary hash mismatch: {name}")
    control_bins = read_json(CONTROL_MANIFEST)
    control_app = control_bins["deadpan-app"]
    require(control_app["sha256"] == "c36b894466eb4f16fe976924b05c02ada87b0e8872f7f9a7fc6028a60a342037",
            "qualified control binary identity changed")
    for name, row in control_bins.items():
        path = Path(row["executed"])
        require(path.is_file() and sha(path) == row["sha256"], f"qualified control binary hash mismatch: {name}")

    baseline = read_json(ROOT / "original-source-identities.json")
    proposed_map = read_json(ROOT / "build-source-identities.json")
    overlay = read_json(ROOT / "formatted-overlay-audit.json")
    require(overlay.get("changed") == CHANGED and overlay.get("nonowned_rust_unchanged") is True
            and overlay.get("inputs_unchanged") is True,
            "source overlay audit differs from one-file scope")
    require(set(proposed_map) == set(baseline), "Rust source set changed")
    require({p for p in proposed_map if proposed_map[p] != baseline[p]} == set(CHANGED),
            "formatted source changed outside audio_reference.rs")
    live_map = {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob("*.rs")}
    require(live_map == proposed_map, "live formatted Rust source differs from frozen identity map")
    inputs = read_json(ROOT / "input-identities.json")
    require(len(inputs) == 6 and all(sha(SOURCE / p) == h for p, h in inputs.items()),
            "one or more of six qualified inputs changed")
    worker_hashes = read_json(ROOT / "worker-notes" / "source-hashes.json")
    before_rel = CHANGED[0]
    before_file = ROOT / "worker-notes" / "before" / (before_rel + ".txt")
    proposed_file = ROOT / "worker-notes" / "after" / (before_rel + ".txt")
    require(sha(before_file) == worker_hashes["before"][before_rel] == baseline[before_rel],
            "baseline source snapshot identity mismatch")
    require(sha(proposed_file) == worker_hashes["proposed"][before_rel],
            "proposed pre-format snapshot identity mismatch")
    require(sha(SOURCE / before_rel) == proposed_map[before_rel],
            "formatted final source identity mismatch")

    focused = read_json(ROOT / "focused-binaries.json")
    require(focused and all(row.get("step") in TEST_STEPS for row in focused),
            "focused test binary manifest missing or has unexpected step")
    for row in focused:
        path = Path(row["retained_executed_copy"])
        require(path.is_file() and sha(path) == row["sha256"],
                f"retained focused test binary hash mismatch: {row['step']}")
    require(set(row["step"] for row in focused) == set(TEST_STEPS),
            "focused binary manifest does not cover every focused test command")
    for step in TEST_STEPS:
        step_rows = [row for row in focused if row["step"] == step]
        require(all(row.get("test_results") for row in step_rows), f"{step}: missing test result counts")
        for row in step_rows:
            for passed, failed, ignored in row["test_results"]:
                require(int(passed) > 0 and failed == "0" and ignored == "0",
                        f"{step}: a focused test target failed or ignored tests")
    focused_output = (ROOT / "focused-reuse-tests.stdout").read_text()
    reuse_tests = [line for line in focused_output.splitlines()
                   if "test " in line and "full_layout_reuse_" in line and line.rstrip().endswith("... ok")]
    require(len(reuse_tests) == 5, "expected five passing full_layout_reuse_ unit tests")

    require((ROOT / "summary.json").is_file(), "measurement incomplete: summary.json absent")
    summary = read_json(ROOT / "summary.json")
    trials = read_json(ROOT / "trials.json")
    commands = read_json(ROOT / "commands.json")
    require(len(trials) == 10 and [(row["pair"], row["kind"]) for row in trials] == PAIR_ORDER,
            "expected five alternating control/prototype pairs")
    require(len(commands) == 20 and all(row.get("exit") == 0 for row in commands),
            "expected 20 successful fixture/run commands")
    require(summary.get("n_per_kind") == 5 and summary.get("prototype_only") is True
            and summary.get("no_percentile_or_target_pass") is True,
            "summary does not preserve diagnostic-only scope")
    require(all(sum(row.get("kind") == kind for row in trials) == 5
                for kind in ("control", "prototype")),
            "expected five trials for each kind")
    require(summary["kinds"].keys() >= {"control", "prototype"}, "summary kind rows missing")

    for trial in trials:
        key = "{}-{}".format(trial["pair"], trial["kind"])
        require(trial.get("checks") == 7 and trial.get("kestrel_checks") == 1,
                f"{key}: check counts mismatch")
        require(all(trial.get(flag) is True for flag in (
            "source_and_binary_guards", "authored_inputs_unchanged", "empty_wal"
        )), f"{key}: source, binary or authored-input guard failed")
        require(trial.get("load", [999])[0] <= 3, f"{key}: quiet-load gate exceeded")
        binary = control_app if trial["kind"] == "control" else bins["deadpan-app"]
        require(trial.get("binary_sha256") == binary["sha256"], f"{key}: binary identity mismatch")
        before = read_json(ROOT / (key + "-input-before.json"))
        after = read_json(ROOT / (key + "-input-after.json"))
        require(before.get("project.sqlite", {}).get("sha256") == after.get("project.sqlite", {}).get("sha256"),
                f"{key}: main SQLite database changed")
        require({k:v for k,v in before.items() if k not in SIDECARS}
                == {k:v for k,v in after.items() if k not in SIDECARS},
                f"{key}: authored package contents changed")
        require(after.get("project.sqlite-wal", {}).get("bytes", 0) == 0, f"{key}: WAL nonempty")
        report = read_json(ROOT / (key + "-report") / "report.json")
        require("failed" in report and not report["failed"], f"{key}: failed report or missing failure field")
        require(report.get("metadata", {}).get("binary_sha256") == binary["sha256"],
                f"{key}: report binary identity mismatch")
        require(report.get("metadata", {}).get("build_profile") == "release",
                f"{key}: report is not release profile")
        require(report.get("metadata", {}).get("fixture_sha256") == before["project.sqlite"]["sha256"],
                f"{key}: fixture identity mismatch")
        pause = next(s for s in report["scenarios"] if s["name"] == "large-first-pause")
        kestrel = next(s for s in report["scenarios"] if s["name"] == "kestrel-shortcuts")
        require(len(pause.get("checks", [])) == 7 and all(c["passed"] for c in pause["checks"]),
                f"{key}: first-pause checks failed")
        require(len(kestrel.get("checks", [])) == 1 and all(c["passed"] for c in kestrel["checks"]),
                f"{key}: Kestrel check failed")

    for kind in ("control", "prototype"):
        rows = [row for row in trials if row["kind"] == kind]
        metrics = set(rows[0]["metrics"])
        require(all(set(row["metrics"]) == metrics for row in rows), f"{kind}: timing metric set changed")
        require(set(summary["kinds"][kind]) == metrics, f"{kind}: summary metric set differs")
        for metric in metrics:
            values = sorted(float(row["metrics"][metric]) for row in rows)
            recorded = summary["kinds"][kind][metric]
            expected = {"min": values[0], "median": values[2], "max": values[-1]}
            require(all(abs(float(recorded[key]) - expected[key]) < 1e-6 for key in expected),
                    f"{kind}/{metric}: summary differs from the five raw trials")

    STAGE.mkdir(parents=True)
    mandatory = [
        "build_prototype.py", "measure.py", "build-receipt.json", "build-source-identities.json",
        "original-source-identities.json", "input-identities.json", "formatted-overlay-audit.json",
        "source-basis.json", "independent-review.json", "binaries.json", "focused-binaries.json",
        "build-driver.stdout", "build-driver.stderr",
        "build-owner.json", "measure-owner.json", "focused-binaries.json", "commands.json",
        "trials.json", "summary.json", "focused-reuse-tests.stdout", "focused-reuse-tests.stderr",
        "worker-notes/HANDOFF.md", "worker-notes/instrumentation.patch", "worker-notes/source-hashes.json",
        "worker-notes/before-source-hashes.json", "worker-notes/source-review.json",
        "worker-notes/worker-artifact-hashes.json",
    ]
    for name in BUILD_STEPS:
        mandatory.extend([name + ".stdout", name + ".stderr"])
    for name in ("clone-release-cache", "format"):
        pass
    for name in ("focused-reuse-tests", "scoped-capture-tests", "command-work-tests", "insert-time-tests"):
        mandatory.extend([name + ".stdout", name + ".stderr"])
    mandatory.extend(["measurement-driver.stdout", "measurement-driver.stderr"])
    for rel in mandatory:
        copy_payload(ROOT / rel, rel)
    copy_payload(Path(__file__), "package_prototype.py")
    copy_payload(CONTROL_MANIFEST, "qualified-control-binaries.json")

    # Keep any named failure/correction/retry records that the root retained.
    optional_pattern = re.compile(r"(fail|correct|retry|replay|review)", re.IGNORECASE)
    for path in sorted(ROOT.iterdir()):
        if path.is_file() and optional_pattern.search(path.name) and path.suffix in {
            ".json", ".txt", ".md", ".stdout", ".stderr", ".log", ".patch"
        }:
            rel = "optional-history/" + path.name
            copy_payload(path, rel)

    for version in ("before",):
        src = ROOT / "worker-notes" / version / (CHANGED[0] + ".txt")
        dst = STAGE / "sources" / version / (CHANGED[0] + ".txt")
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dst)
    proposed_dst = STAGE / "sources" / "proposed" / (CHANGED[0] + ".txt")
    proposed_dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(proposed_file, proposed_dst)
    final_dst = STAGE / "sources" / "formatted" / (CHANGED[0] + ".txt")
    final_dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(SOURCE / CHANGED[0], final_dst)
    diff = difflib.unified_diff(
        before_file.read_text().splitlines(keepends=True),
        (SOURCE / CHANGED[0]).read_text().splitlines(keepends=True),
        fromfile="a/" + CHANGED[0], tofile="b/" + CHANGED[0],
    )
    (STAGE / "formatted-overlay.patch").write_text("".join(diff))
    (STAGE / "source-snapshot-notes.md").write_text(
        "before/ is the original baseline. proposed/ is the worker's pre-format overlay. "
        "formatted/ is the release-build source and matches build-source-identities.json.\n")

    for trial in trials:
        key = "{}-{}".format(trial["pair"], trial["kind"])
        for suffix in ("make.stdout", "make.stderr", "input-before.json", "input-after.json", "pre-run.json"):
            copy_payload(ROOT / (key + "-" + suffix), "trials/" + key + "/" + suffix)
        for suffix in ("stdout", "stderr"):
            copy_payload(ROOT / (key + "." + suffix), "trials/" + key + "/" + suffix)
        copy_tree(ROOT / (key + "-report"), "trials/" + key + "/report")

    inventory = []
    for path in sorted(STAGE.rglob("*"), key=lambda p: p.as_posix()):
        if path.is_file() and path != STAGE / "inventory.json":
            inventory.append({"path": path.relative_to(STAGE).as_posix(),
                              "bytes": path.stat().st_size, "sha256": sha(path)})
    manifest_out = {"total_files": len(inventory), "total_bytes": sum(row["bytes"] for row in inventory),
                    "files": inventory}
    write_json(manifest_out, STAGE / "inventory.json")
    for row in inventory:
        path = STAGE / row["path"]
        require(path.stat().st_size == row["bytes"] and sha(path) == row["sha256"],
                f"staged payload verification failed: {row['path']}")
    inventory_hash = sha(STAGE / "inventory.json")
    require(not DEST.exists(), f"destination appeared during packaging: {DEST}")
    STAGE.rename(DEST)
    print(json.dumps({"destination": str(DEST), "total_files": manifest_out["total_files"],
                      "total_bytes": manifest_out["total_bytes"], "inventory_sha256": inventory_hash,
                      "trial_count": len(trials), "diagnostic_only": True}, sort_keys=True))

if __name__ == "__main__":
    main()
