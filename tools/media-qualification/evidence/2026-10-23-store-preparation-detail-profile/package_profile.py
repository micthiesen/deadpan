#!/usr/bin/env python3
"""Bounded, gated packager for the isolated Store preparation timing profile.

Root runs this only after the measurement driver is terminal. It refuses to
stage output until every required build/parser/trial receipt is complete.
No binaries, source archive, source tree, target tree, or fixture DB is copied.
"""
from __future__ import annotations
import gzip, hashlib, json, shutil, difflib, importlib.util
from pathlib import Path

ROOT = Path("/tmp/deadpan-store-preparation-detail-profile-2026-10-23")
DEST = Path("/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-store-preparation-detail-profile")
STAGE = DEST.with_name(DEST.name + ".staging")
SOURCE = ROOT / "source"
KINDS = ("root-control", "off", "on")
CHANGED = [
    "crates/deadpan-app/src/project/service.rs",
    "crates/deadpan-app/src/project/service/macros.rs",
    "crates/deadpan-store/src/compound.rs",
    "crates/deadpan-store/src/lib.rs",
    "crates/deadpan-store/src/prepared_compound_profile.rs",
]
INPUT_KEYS = [
    "Cargo.lock", "Cargo.toml", "rust-toolchain.toml", ".cargo/config.toml",
    "crates/deadpan-app/src/navigation/registry/baseline-routing.txt",
    "docs/COMMANDS.md",
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
    data = src.read_bytes()
    if compress_large and len(data) > 100_000 and src.suffix in {".json", ".txt", ".log", ".html"}:
        with dst.with_suffix(dst.suffix + ".gz").open("wb") as raw:
            with gzip.GzipFile(filename="", mode="wb", compresslevel=9, fileobj=raw, mtime=0) as f:
                f.write(data)
    else:
        shutil.copyfile(src, dst)

def copy_tree_files(src, rel):
    require(src.is_dir(), f"missing report directory: {src}")
    files = [p for p in src.rglob("*") if p.is_file()]
    require(files, f"empty report directory: {src}")
    for f in files:
        copy_payload(f, str(Path(rel) / f.relative_to(src)))

def canonical_json(value, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")

def main():
    require(not DEST.exists(), f"destination already exists: {DEST}")
    require(not STAGE.exists(), f"staging destination already exists: {STAGE}")

    build = read_json(ROOT / "build-receipt.json")
    names = [r["name"] for r in build]
    require(names == ["clone-release-cache", "format", "build"], f"incomplete build receipt: {names}")
    require(all(r.get("exit") == 0 for r in build), "one or more build steps failed")
    parser = read_json(ROOT / "parser-tests.json")
    require(parser.get("synthetic_only") is True and parser.get("valid_shape_passed") is True,
            "parser synthetic checks did not pass")
    require(parser.get("invalid_case_count") == 15 and len(parser.get("invalid_shapes_rejected", [])) == 15,
            "parser invalid-case count mismatch")
    bins = read_json(ROOT / "binaries.json")
    require(bins.get("deadpan-app", {}).get("sha256"), "missing built app identity")
    for name, record in bins.items():
        executable = Path(record["executed"])
        require(executable.is_file() and sha(executable) == record["sha256"],
                f"executed binary identity mismatch: {name}")
    control_manifest_path = Path("/tmp/deadpan-compact-json-size-qualification-2026-10-23/release-native-binaries.json")
    control_bins = read_json(control_manifest_path)
    for name, record in control_bins.items():
        executable = Path(record["executed"])
        require(executable.is_file() and sha(executable) == record["sha256"],
                f"qualified control binary identity mismatch: {name}")

    original = read_json(ROOT / "original-source-identities.json")
    final = read_json(ROOT / "instrumented-source-identities.json")
    overlay = read_json(ROOT / "formatted-overlay-audit.json")
    require(sorted(overlay.get("changed", [])) == sorted(CHANGED), "source overlay differs from five-file assignment")
    require(overlay.get("unchanged_core_and_cli") and overlay.get("inputs_unchanged"), "overlay guard failed")
    require({p for p in final if final[p] != original.get(p)} == set(CHANGED), "source map changed outside assigned overlay")
    live_rust = {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob("*.rs")}
    require(live_rust == final, "formatted live Rust source set differs from full source map")
    inputs = read_json(ROOT / "input-identities.json")
    require(set(inputs) == set(INPUT_KEYS), "unexpected qualified-input identity set")
    require(all(sha(SOURCE / p) == h for p, h in inputs.items()), "qualified input guard failed")

    require((ROOT / "service-summary.json").is_file(), "measurement incomplete: service-summary.json absent")
    summary = read_json(ROOT / "service-summary.json")
    trials = read_json(ROOT / "service-trials.json")
    require(len(trials) == 9, f"expected 9 complete trials, got {len(trials)}")
    require(summary.get("diagnostic_only") is True and summary.get("no_percentile_or_target_pass") is True,
            "summary diagnostic-scope flags missing")
    require(summary.get("instrumentation_overhead_unqualified") is True
            and summary.get("no_production_optimization_in_this_profile") is True,
            "summary overstates scope or optimization status")
    require(summary.get("n_per_kind") == 3, "summary sample count mismatch")
    require(all(sum(t.get("kind") == k for t in trials) == 3 for k in KINDS),
            "expected three trials per kind")
    require([(t['round'], t['kind']) for t in trials] == [(1,'root-control'),(1,'off'),(1,'on'),(2,'off'),(2,'on'),(2,'root-control'),(3,'on'),(3,'root-control'),(3,'off')], 'rotated trial order changed')
    require(all(t['load'][0] <= 3 for t in trials), 'quiet-load gate failed')
    commands = read_json(ROOT / 'measurement-commands.json')
    require(len(commands) == 21 and all(c['exit'] == 0 for c in commands), 'measurement commands incomplete or failed')
    profile_spec = importlib.util.spec_from_file_location("profile_check", ROOT / "check_profile.py")
    profile_check = importlib.util.module_from_spec(profile_spec)
    profile_spec.loader.exec_module(profile_check)
    expected_counts = {row["name"]: row["expected_count"] for row in read_json(ROOT / "profile_schema.json")["phases"]}
    profile_rows = []
    for trial in trials:
        key = f'{trial["round"]}-{trial["kind"]}'
        require(trial.get("checks") == 7 and trial.get("kestrel_checks") == 1, f"{key}: check counts mismatch")
        require(all(trial.get(flag) is True for flag in [
            "source_and_binary_guards", "input_guards", "authored_input_unchanged", "wal_empty"
        ]), f"{key}: guard failure")
        before = read_json(ROOT / f"{key}-input-before.json")
        after = read_json(ROOT / f"{key}-input-after.json")
        require(before.get("project.sqlite", {}).get("sha256") == after.get("project.sqlite", {}).get("sha256"),
                f"{key}: authored database changed")
        require({k:v for k,v in before.items() if k not in SIDECARS}
                == {k:v for k,v in after.items() if k not in SIDECARS}, f"{key}: authored files changed")
        require(after.get("project.sqlite-wal", {}).get("bytes", 0) == 0, f"{key}: WAL not empty")
        report = read_json(ROOT / f"{key}-report" / "report.json")
        require("failed" in report and not report["failed"], f"{key}: report missing or has failed checks")
        expected_binary = control_bins["deadpan-app"] if trial["kind"] == "root-control" else bins["deadpan-app"]
        require(trial.get("binary_sha256") == expected_binary["sha256"],
                f"{key}: trial app identity differs from retained binary manifest")
        require(report.get("metadata", {}).get("binary_sha256") == trial.get("binary_sha256"),
                f"{key}: binary/report identity mismatch")
        require(report.get("metadata", {}).get("fixture_sha256") == before["project.sqlite"]["sha256"],
                f"{key}: fixture identity mismatch")
        workload = next(s for s in report["scenarios"] if s["name"] == "large-first-pause")
        kestrel = next(s for s in report["scenarios"] if s["name"] == "kestrel-shortcuts")
        require(len(workload.get("checks", [])) == 7 and all(c["passed"] for c in workload["checks"]),
                f"{key}: workload checks failed")
        require(len(kestrel.get("checks", [])) == 1 and all(c["passed"] for c in kestrel["checks"]),
                f"{key}: Kestrel check failed")
        stderr = (ROOT / f"{key}.stderr").read_text()
        if trial["kind"] == "on":
            parsed = read_json(ROOT / f"{key}-parse.stdout")
            require(parsed.get("matched_macro_reports") == 1 and parsed.get("profile_lines") == 35,
                    f"{key}: profile parse incomplete")
            raw_parsed = profile_check.parse_profile(stderr)
            require(parsed == raw_parsed, f"{key}: saved parse differs from captured stderr")
            require(parsed == trial.get("profile"), f"{key}: trial profile differs from captured stderr")
            require(parsed.get("matched_macro_reports") == 1 and parsed.get("profile_lines") == 35,
                    f"{key}: expected one complete profile block")
            require(parsed.get("phase_counts") == expected_counts and len(expected_counts) == 33,
                    f"{key}: phase multiplicities differ from schema")
            profile_rows.append(parsed)
        else:
            require("DEADPAN_COMPOUND_PREPARATION_DETAIL_PROFILE " not in stderr,
                    f"{key}: profiling output while disabled")

    for kind in KINDS:
        rows = [r for r in trials if r["kind"] == kind]
        metric_names = set(rows[0]["metrics"])
        require(all(set(r["metrics"]) == metric_names for r in rows), f"{kind}: metric schema differs")
        for metric in metric_names:
            vals = sorted(float(r["metrics"][metric]) for r in rows)
            given = summary["kinds"][kind][metric]
            expected = {"min": vals[0], "median": vals[1], "max": vals[-1]}
            require(all(abs(float(given[k]) - expected[k]) < 1e-6 for k in expected),
                    f"{kind}/{metric}: summary differs from raw trial values")
    require(len({r["round"] for r in trials}) == 3, "expected three alternating trial rounds")
    require(len(profile_rows) == 3, "expected three parsed profiling-on reports")
    for source_key, target_key, scale in (
        ("nanoseconds", "phase_ms", 1_000_000),
        ("residual_nanoseconds", "residual_ms", 1_000_000),
    ):
        phase_names = set(profile_rows[0][source_key])
        require(all(set(row[source_key]) == phase_names for row in profile_rows),
                f"{source_key}: phase set changed")
        for phase in phase_names:
            vals = sorted(row[source_key][phase] / scale for row in profile_rows)
            given = summary[target_key][phase]
            expected = {"min": vals[0], "median": vals[1], "max": vals[-1]}
            require(all(abs(float(given[k]) - expected[k]) < 1e-6 for k in expected),
                    f"{target_key}/{phase}: summary differs from raw profiles")

    STAGE.mkdir(parents=True)
    payloads = [
        "build_profile.py", "check_profile.py", "measure_service.py", "test_profile_parser.py",
        "profile_schema.json", "parser-tests.json", "parser-tests.stdout", "parser-tests.stderr",
        "build-receipt.json", "build-driver.stdout", "build-driver.stderr", "build.stdout", "build.stderr",
        "clone-release-cache.stdout", "clone-release-cache.stderr", "format.stdout", "format.stderr",
        "formatted-overlay-audit.json", "original-source-identities.json", "instrumented-source-identities.json",
        "input-identities.json", "source-basis.json", "independent-review.json", "profile-owner.json",
        "measurement-owner.json", "binaries.json", "service-trials.json", "service-summary.json",
        "measurement-commands.json", "worker-notes/INSTRUMENTATION.md", "worker-notes/changed-files.txt",
        "measurement-driver.stdout", "measurement-driver.stderr",
        "worker-notes/instrumentation.patch", "worker-notes/phases.json", "worker-notes/source-hashes.json",
        "worker-notes/worker-artifact-hashes.json", "worker-notes/before-source-hashes.json",
    ]
    for rel in payloads:
        copy_payload(ROOT / rel, rel)
    copy_payload(Path(__file__), 'package_profile.py')
    copy_payload(Path(__file__).parent / 'REPORT-outline.md', 'packaging-draft-review.md')
    copy_payload(control_manifest_path, "qualified-control-binaries.json")
    (STAGE / "worker-notes" / "instrumentation.patch").rename(STAGE / "worker-notes" / "instrumentation-preformat.patch")
    (STAGE / "worker-notes" / "instrumentation-patch-note.txt").write_text(
        "instrumentation-preformat.patch is the worker's original pre-format patch. "
        "formatted-overlay.patch is regenerated from baseline snapshots to the "
        "formatted source actually built.\n")
    (STAGE / "source-snapshot-notes.md").write_text(
        "The before/ copies are original baseline source snapshots, checked against "
        "original-source-identities.json. The after/ copies are from the formatted, "
        "built source tree, checked against instrumented-source-identities.json. "
        "The prepared_compound_profile.rs module was newly added, so no before-source existed.\n")
    for path in CHANGED:
        for version in ("before", "after"):
            src = ROOT / "worker-notes" / version / f"{path}.txt"
            if version == "after":
                src = SOURCE / path
            dest = STAGE / "sources" / version / f"{path}.txt"
            dest.parent.mkdir(parents=True, exist_ok=True)
            if src.is_file():
                if version == "before":
                    require(original.get(path) == sha(src), f"baseline source snapshot mismatch: {path}")
                else:
                    require(final.get(path) == sha(src), f"formatted source snapshot mismatch: {path}")
                shutil.copyfile(src, dest)
            else:
                require(version == "before" and path == "crates/deadpan-store/src/prepared_compound_profile.rs"
                        and path not in original,
                        f"missing source snapshot: {src}")
                dest.write_text("No before-version existed; this module was newly added by the instrumentation overlay.\n")
    diff_lines = []
    for path in CHANGED:
        before_path = STAGE / "sources" / "before" / f"{path}.txt"
        after_path = STAGE / "sources" / "after" / f"{path}.txt"
        before_text = before_path.read_text() if path in original and before_path.exists() else ""
        after_text = after_path.read_text()
        diff_lines.extend(difflib.unified_diff(
            before_text.splitlines(keepends=True), after_text.splitlines(keepends=True),
            fromfile=f"a/{path}", tofile=f"b/{path}"))
    (STAGE / "formatted-overlay.patch").write_text("".join(diff_lines))
    for trial in trials:
        key = f'{trial["round"]}-{trial["kind"]}'
        for suffix in ("make.stdout", "make.stderr", "input-before.json", "input-after.json", "pre-run.json"):
            copy_payload(ROOT / f"{key}-{suffix}", f"trials/{key}/{suffix}")
        for suffix in ("stdout", "stderr"):
            copy_payload(ROOT / f"{key}.{suffix}", f"trials/{key}/{suffix}")
        if trial["kind"] == "on":
            for suffix in ("stdout", "stderr"):
                copy_payload(ROOT / f"{key}-parse.{suffix}", f"trials/{key}/parse.{suffix}")
        copy_tree_files(ROOT / f"{key}-report", f"trials/{key}/report")

    inventory = []
    for p in sorted(STAGE.rglob("*"), key=lambda x: x.as_posix()):
        if p.is_file() and p != STAGE / "inventory.json":
            inventory.append({"path": p.relative_to(STAGE).as_posix(),
                              "bytes": p.stat().st_size, "sha256": sha(p)})
    manifest = {"total_files": len(inventory), "total_bytes": sum(x["bytes"] for x in inventory),
                "files": inventory}
    canonical_json(manifest, STAGE / "inventory.json")
    for row in inventory:
        p = STAGE / row["path"]
        require(p.stat().st_size == row["bytes"] and sha(p) == row["sha256"],
                f"staged payload verification failed: {row['path']}")
    inv_hash = sha(STAGE / "inventory.json")
    require(not DEST.exists(), f"destination appeared during packaging: {DEST}")
    STAGE.rename(DEST)
    print(json.dumps({"destination": str(DEST), "total_files": manifest["total_files"],
                      "total_bytes": manifest["total_bytes"], "inventory_sha256": inv_hash,
                      "trial_count": len(trials), "diagnostic_only": True}, sort_keys=True))

if __name__ == "__main__":
    main()
