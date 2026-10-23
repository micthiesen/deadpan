#!/usr/bin/env python3
"""Root-run gated packager for the isolated Core planning diagnostic.

Requires terminal successful build, parser, measurement and fixture guards.
Retains source and reports, never binaries, archives, source/target trees or DBs.
"""
from __future__ import annotations
import gzip
import hashlib
import importlib.util
import json
import shutil
import difflib
from pathlib import Path

ROOT = Path("/tmp/deadpan-core-planning-detail-profile-2026-10-23")
DEST = Path("/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-core-planning-detail-profile")
STAGE = DEST.with_name(DEST.name + ".staging")
SOURCE = ROOT / "source"
KINDS = ("root-control", "off", "on")
TRIAL_ORDER = [
    (1, "root-control"), (1, "off"), (1, "on"),
    (2, "off"), (2, "on"), (2, "root-control"),
    (3, "on"), (3, "root-control"), (3, "off"),
]
CHANGED = [
    "crates/deadpan-app/src/project/service.rs",
    "crates/deadpan-app/src/project/service/macros.rs",
    "crates/deadpan-core/src/lib.rs",
    "crates/deadpan-core/src/semantic/planner.rs",
    "crates/deadpan-core/src/semantic/planner/pause.rs",
    "crates/deadpan-core/src/compound.rs",
    "crates/deadpan-core/src/command.rs",
    "crates/deadpan-core/src/work_profile.rs",
    "crates/deadpan-store/src/compound.rs",
]
SIDECARS = {"project.sqlite-shm", "project.sqlite-wal"}
CONTROL_MANIFEST = Path("/tmp/deadpan-compact-json-size-qualification-2026-10-23/release-native-binaries.json")

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
    data = src.read_bytes()
    if compress_large and len(data) > 100_000 and src.suffix in {".json", ".txt", ".log", ".html"}:
        with dst.with_suffix(dst.suffix + ".gz").open("wb") as raw:
            with gzip.GzipFile(filename="", mode="wb", compresslevel=9, fileobj=raw, mtime=0) as gz:
                gz.write(data)
    else:
        shutil.copyfile(src, dst)

def copy_tree_files(src, rel):
    require(src.is_dir(), f"missing report directory: {src}")
    files = [p for p in src.rglob("*") if p.is_file()]
    require(files, f"empty report directory: {src}")
    for p in files:
        copy_payload(p, str(Path(rel) / p.relative_to(src)))

def write_json(value, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")

def main():
    require(not DEST.exists(), f"destination already exists: {DEST}")
    require(not STAGE.exists(), f"staging destination already exists: {STAGE}")

    build = read_json(ROOT / "build-receipt.json")
    names = [row["name"] for row in build]
    require(names == ["clone-release-cache", "format", "build"], f"incomplete build receipt: {names}")
    require(all(row.get("exit") == 0 for row in build), "one or more build steps failed")
    parser_record = read_json(ROOT / "parser-tests.json")
    require(parser_record.get("synthetic_only") is True and parser_record.get("valid_shape_passed") is True,
            "parser synthetic checks did not pass")
    require(parser_record.get("invalid_case_count") == 17
            and len(parser_record.get("invalid_shapes_rejected", [])) == 17,
            "parser must reject exactly 17 invalid synthetic cases")

    bins = read_json(ROOT / "binaries.json")
    require(bins.get("deadpan-app", {}).get("sha256"), "missing built app identity")
    for name, record in bins.items():
        executable = Path(record["executed"])
        require(executable.is_file() and sha(executable) == record["sha256"],
                f"executed binary identity mismatch: {name}")
    control_bins = read_json(CONTROL_MANIFEST)
    for name, record in control_bins.items():
        executable = Path(record["executed"])
        require(executable.is_file() and sha(executable) == record["sha256"],
                f"qualified control binary identity mismatch: {name}")

    original = read_json(ROOT / "original-source-identities.json")
    final = read_json(ROOT / "instrumented-source-identities.json")
    overlay = read_json(ROOT / "formatted-overlay-audit.json")
    require(sorted(overlay.get("changed", [])) == sorted(CHANGED),
            "formatted source overlay differs from exact nine-file assignment")
    require(overlay.get("unchanged_cli_and_nonowned_sources") is True
            and overlay.get("inputs_unchanged") is True,
            "overlay non-owned-source or input guard failed")
    require({path for path in final if final[path] != original.get(path)} == set(CHANGED),
            "full source map differs outside exact overlay")
    live_rust = {str(path.relative_to(SOURCE)): sha(path) for path in SOURCE.rglob("*.rs")}
    require(live_rust == final, "live Rust source set differs from frozen formatted source map")
    inputs = read_json(ROOT / "input-identities.json")
    require(inputs and all(sha(SOURCE / path) == digest for path, digest in inputs.items()),
            "qualified input identity guard failed")
    require(len(inputs) == 6, "expected exactly six qualified inputs")

    require((ROOT / "service-summary.json").is_file(), "measurement incomplete: service-summary.json absent")
    summary = read_json(ROOT / "service-summary.json")
    trials = read_json(ROOT / "service-trials.json")
    require(len(trials) == 9, f"expected nine complete measurement trials, got {len(trials)}")
    require([(row["round"], row["kind"]) for row in trials] == TRIAL_ORDER,
            "rotated trial order differs from measurement driver")
    require(summary.get("diagnostic_only") is True
            and summary.get("no_percentile_or_target_pass") is True
            and summary.get("instrumentation_overhead_unqualified") is True
            and summary.get("no_production_optimization_in_this_profile") is True,
            "summary diagnostic scope flags missing or incorrect")
    require(summary.get("n_per_kind") == 3, "summary sample count mismatch")
    require(all(sum(row.get("kind") == kind for row in trials) == 3 for kind in KINDS),
            "expected three trials per control/off/on kind")
    commands = read_json(ROOT / "measurement-commands.json")
    require(len(commands) == 21 and all(row.get("exit") == 0 for row in commands),
            "measurement command receipt must contain 21 successful commands")

    profile_spec = importlib.util.spec_from_file_location("core_profile_check", ROOT / "check_profile.py")
    profile_check = importlib.util.module_from_spec(profile_spec)
    profile_spec.loader.exec_module(profile_check)
    schema = read_json(ROOT / "profile_schema.json")
    require(len(schema.get("phases", [])) == 24 and schema.get("expected_profile_lines") == 26,
            "unexpected Core profile schema shape")
    expected_counts = {phase["name"]: phase["expected_count"] for phase in schema["phases"]}
    expected_count_values = sorted(expected_counts.values())
    require(expected_count_values.count(1) == 14 and expected_count_values.count(2) == 5
            and expected_count_values.count(3) == 4 and expected_count_values.count(4) == 1,
            "schema phase multiplicities changed")
    profile_rows = []
    for trial in trials:
        key = "{}-{}".format(trial["round"], trial["kind"])
        require(trial.get("checks") == 7 and trial.get("kestrel_checks") == 1,
                f"{key}: report check counts mismatch")
        require(all(trial.get(flag) is True for flag in (
            "source_and_binary_guards", "input_guards", "authored_input_unchanged", "wal_empty"
        )), f"{key}: recorded guard failed")
        require(trial.get("load", [999])[0] <= 3, f"{key}: load gate exceeded")
        before = read_json(ROOT / (key + "-input-before.json"))
        after = read_json(ROOT / (key + "-input-after.json"))
        require(before.get("project.sqlite", {}).get("sha256") == after.get("project.sqlite", {}).get("sha256"),
                f"{key}: main DB changed")
        require({k:v for k,v in before.items() if k not in SIDECARS}
                == {k:v for k,v in after.items() if k not in SIDECARS},
                f"{key}: authored files changed")
        require(after.get("project.sqlite-wal", {}).get("bytes", 0) == 0, f"{key}: WAL is nonempty")
        report = read_json(ROOT / (key + "-report") / "report.json")
        require("failed" in report and not report["failed"], f"{key}: report missing or has failed checks")
        expected_binary = control_bins["deadpan-app"] if trial["kind"] == "root-control" else bins["deadpan-app"]
        require(trial.get("binary_sha256") == expected_binary["sha256"],
                f"{key}: app hash differs from retained binary manifest")
        require(report.get("metadata", {}).get("binary_sha256") == trial.get("binary_sha256"),
                f"{key}: report app identity mismatch")
        require(report.get("metadata", {}).get("fixture_sha256") == before["project.sqlite"]["sha256"],
                f"{key}: report fixture identity mismatch")
        workload = next(s for s in report["scenarios"] if s["name"] == "large-first-pause")
        kestrel = next(s for s in report["scenarios"] if s["name"] == "kestrel-shortcuts")
        require(len(workload.get("checks", [])) == 7 and all(c["passed"] for c in workload["checks"]),
                f"{key}: workload checks failed")
        require(len(kestrel.get("checks", [])) == 1 and all(c["passed"] for c in kestrel["checks"]),
                f"{key}: Kestrel check failed")
        stderr = (ROOT / (key + ".stderr")).read_text()
        if trial["kind"] == "on":
            parsed = read_json(ROOT / (key + "-parse.stdout"))
            raw = profile_check.parse_profile(stderr)
            require(parsed == raw, f"{key}: saved parser result differs from captured stderr")
            require(parsed == trial.get("profile"), f"{key}: trial profile differs from captured stderr")
            require(parsed.get("matched_macro_reports") == 1
                    and parsed.get("profile_lines") == 26
                    and parsed.get("phase_counts") == expected_counts,
                    f"{key}: one complete schema-matching profile block required")
            profile_rows.append(parsed)
        else:
            require("DEADPAN_CORE_WORK_PROFILE " not in stderr,
                    f"{key}: instrumentation output appeared while disabled")

    for kind in KINDS:
        rows = [row for row in trials if row["kind"] == kind]
        metric_names = set(rows[0]["metrics"])
        require(all(set(row["metrics"]) == metric_names for row in rows), f"{kind}: metric schema differs")
        for metric in metric_names:
            values = sorted(float(row["metrics"][metric]) for row in rows)
            actual = summary["kinds"][kind][metric]
            expected = {"min": values[0], "median": values[1], "max": values[-1]}
            require(all(abs(float(actual[key]) - expected[key]) < 1e-6 for key in expected),
                    f"{kind}/{metric}: summary differs from raw trials")
    require(len(profile_rows) == 3, "expected three profiling-on parser results")
    for source_key, summary_key in (("nanoseconds", "phase_ms"),
                                    ("residual_nanoseconds", "residual_ms")):
        phase_names = set(profile_rows[0][source_key])
        require(all(set(row[source_key]) == phase_names for row in profile_rows),
                f"{source_key}: phase set differs between trials")
        for phase in phase_names:
            values = sorted(row[source_key][phase] / 1_000_000 for row in profile_rows)
            actual = summary[summary_key][phase]
            expected = {"min": values[0], "median": values[1], "max": values[-1]}
            require(all(abs(float(actual[key]) - expected[key]) < 1e-6 for key in expected),
                    f"{summary_key}/{phase}: summary differs from parsed trials")

    STAGE.mkdir(parents=True)
    payloads = [
        "build_profile.py", "check_profile.py", "measure_service.py", "test_profile_parser.py",
        "profile_schema.json", "parser-tests.json", "parser-tests.stdout", "parser-tests.stderr",
        "build-receipt.json", "build-driver.stdout", "build-driver.stderr", "build.stdout", "build.stderr",
        "clone-release-cache.stdout", "clone-release-cache.stderr", "format.stdout", "format.stderr",
        "formatted-overlay-audit.json", "original-source-identities.json",
        "instrumented-source-identities.json", "input-identities.json", "source-basis.json",
        "independent-review.json", "profile-owner.json", "measurement-owner.json",
        "binaries.json", "service-trials.json", "service-summary.json", "measurement-commands.json",
        "measurement-driver.stdout", "measurement-driver.stderr",
        "worker-notes/INSTRUMENTATION.md", "worker-notes/changed-files.txt",
        "worker-notes/instrumentation.patch", "worker-notes/schema.json", "worker-notes/source-review.json",
        "worker-notes/source-hashes.json", "worker-notes/worker-artifact-hashes.json",
        "worker-notes/before-source-hashes.json",
        "package-first-failure.stderr", "package-first-failure.py", "package-first-failure.json",
    ]
    for rel in payloads:
        copy_payload(ROOT / rel, rel)
    copy_payload(Path(__file__), "package_profile.py")
    copy_payload(CONTROL_MANIFEST, "qualified-control-binaries.json")
    (STAGE / "worker-notes" / "instrumentation.patch").rename(
        STAGE / "worker-notes" / "instrumentation-preformat.patch")
    (STAGE / "worker-notes" / "instrumentation-patch-note.txt").write_text(
        "instrumentation-preformat.patch is the original worker patch. "
        "formatted-overlay.patch is generated from baseline snapshots to the "
        "formatted source used for the release build.\n")
    (STAGE / "source-snapshot-notes.md").write_text(
        "The before/ files are baseline snapshots and are checked against "
        "original-source-identities.json. The after/ files are formatted release "
        "sources and are checked against instrumented-source-identities.json. "
        "The new work_profile.rs module has no before-version.\n")
    for path in CHANGED:
        for version in ("before", "after"):
            source_file = ROOT / "worker-notes" / version / (path + ".txt")
            if version == "after":
                source_file = SOURCE / path
            destination = STAGE / "sources" / version / (path + ".txt")
            destination.parent.mkdir(parents=True, exist_ok=True)
            if source_file.is_file():
                expected = original.get(path) if version == "before" else final.get(path)
                require(expected == sha(source_file), f"{version} source snapshot mismatch: {path}")
                shutil.copyfile(source_file, destination)
            else:
                require(version == "before" and path == "crates/deadpan-core/src/work_profile.rs"
                        and path not in original,
                        f"missing or unexpected {version} source snapshot: {source_file}")
                destination.write_text("New source file; no baseline existed.\n")
    diff_lines = []
    for path in CHANGED:
        before = STAGE / "sources" / "before" / (path + ".txt")
        after = STAGE / "sources" / "after" / (path + ".txt")
        before_text = before.read_text() if path in original and before.is_file() else ""
        after_text = after.read_text()
        diff_lines.extend(difflib.unified_diff(before_text.splitlines(keepends=True),
                                               after_text.splitlines(keepends=True),
                                               fromfile="a/" + path, tofile="b/" + path))
    (STAGE / "formatted-overlay.patch").write_text("".join(diff_lines))
    for trial in trials:
        key = "{}-{}".format(trial["round"], trial["kind"])
        for suffix in ("make.stdout", "make.stderr", "input-before.json", "input-after.json", "pre-run.json"):
            copy_payload(ROOT / (key + "-" + suffix), "trials/" + key + "/" + suffix)
        for suffix in ("stdout", "stderr"):
            copy_payload(ROOT / (key + "." + suffix), "trials/" + key + "/" + suffix)
        if trial["kind"] == "on":
            for suffix in ("stdout", "stderr"):
                copy_payload(ROOT / (key + "-parse." + suffix), "trials/" + key + "/parse." + suffix)
        copy_tree_files(ROOT / (key + "-report"), "trials/" + key + "/report")

    inventory = []
    for path in sorted(STAGE.rglob("*"), key=lambda p: p.as_posix()):
        if path.is_file() and path != STAGE / "inventory.json":
            inventory.append({"path": path.relative_to(STAGE).as_posix(),
                              "bytes": path.stat().st_size, "sha256": sha(path)})
    manifest = {"total_files": len(inventory), "total_bytes": sum(row["bytes"] for row in inventory),
                "files": inventory}
    write_json(manifest, STAGE / "inventory.json")
    for row in inventory:
        path = STAGE / row["path"]
        require(path.stat().st_size == row["bytes"] and sha(path) == row["sha256"],
                f"staged payload verification failed: {row['path']}")
    inventory_hash = sha(STAGE / "inventory.json")
    require(not DEST.exists(), f"destination appeared during packaging: {DEST}")
    STAGE.rename(DEST)
    print(json.dumps({"destination": str(DEST), "total_files": manifest["total_files"],
                      "total_bytes": manifest["total_bytes"], "inventory_sha256": inventory_hash,
                      "trial_count": len(trials), "diagnostic_only": True}, sort_keys=True))

if __name__ == "__main__":
    main()
