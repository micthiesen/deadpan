#!/usr/bin/env python3
"""Offline evidence reader only. Does not invoke any product/build/test command."""
import collections
import datetime
import hashlib
import json
from pathlib import Path
import re
import sqlite3
import stat
import sys
import traceback

S = Path("/tmp/deadpan-point-targets-2026-10-24")
R = Path("/Users/michael/Code/deadpan")
pins = {}
out = {
    "schema_version": 1, "status": "running", "findings": [],
    "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "limitations": [
        "No product, build, test, decoder, worker, model, publisher or Git command was executed by this audit.",
        "Fifty-two final paths are scoped source/input witnesses, not a complete repository compilation attestation. Older worker manifests are partial pre-format snapshots.",
        "Core/Plan's whole-receipt source guard is false; only its explicitly checked Core/Plan scope is used as unchanged evidence.",
        "The test-only Models .into() removal occurred after release-final and the App full-unit run. Final strict, CLI/Models, format and release-source-check bind the final tree.",
        "Native replay uses scripted input and offscreen shared rendering, not physical keyboard/display, OS picker/IME, VoiceOver or listening qualification.",
        "Export comparison uses the shared picture and limited-audio reference, not an independent picture/DSP oracle or bit-identical encoded audio proof.",
        "Thirty-six picture-neighbor comparisons are unobservable; all picture quality rows pass. No blanket temporal observability, latency, tracking accuracy or requirement completion is inferred.",
    ],
}


def need(ok, msg):
    if not ok:
        raise AssertionError(msg)


def meta(path):
    path = Path(path)
    need(stat.S_ISREG(path.lstat().st_mode), f"not regular: {path}")
    n = 0
    h = hashlib.sha256()
    with path.open("rb") as f:
        for b in iter(lambda: f.read(1024 * 1024), b""):
            n += len(b)
            h.update(b)
    return {"bytes": n, "sha256": h.hexdigest()}


def read(path):
    p = Path(path)
    if not p.is_absolute():
        p = S / p
    m = meta(p)
    need(m["bytes"] <= 268435456, "oversized data read")
    pins[str(p)] = m
    return p.read_bytes()


def obj(path):
    return json.loads(read(path))


def walk(value):
    if isinstance(value, dict):
        yield value
        for x in value.values():
            yield from walk(x)
    elif isinstance(value, list):
        for x in value:
            yield from walk(x)


def run(name, exit_code=0, unchanged=True):
    d = obj(name + ".json")
    need(d["cwd"] == str(R) and d["exit_code"] == exit_code and "finished_at" in d and d["seconds"] >= 0, f"terminal {name}")
    need(d["binary_before"] == d["binary_after"] and d["binaries_unchanged"] is True, f"binary guard {name}")
    need(d["sources_unchanged"] is unchanged and (d["source_before"] == d["source_after"]) is unchanged,
         f"source guard {name}")
    if "runner_sha256" in d:
        need(d["runner_sha256"] == runner_sha, f"runner identity {name}")
    for path, m in d["streams"].items():
        read(path)
        need(meta(path) == m, f"stream identity {name}")
    for path, m in d["binary_before"].items():
        need(meta(path) == m, f"retained executed binary {name}")
    return d


summary_re = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", re.M)


def rust_results(filename):
    text = read(filename).decode()
    summaries = [m.groups() for m in summary_re.finditer(text)]
    need(summaries, f"missing raw summaries {filename}")
    passed = re.findall(r"^test (.+) \.\.\. ok$", text, re.M)
    ignored = re.findall(r"^test (.+) \.\.\. ignored(?:, (.*))?$", text, re.M)
    totals = tuple(sum(int(m[i]) for m in summaries) for i in (1, 2, 3))
    need(len(passed) == totals[0] and len(ignored) == totals[2], f"named raw tests {filename}")
    return {
        "summaries": len(summaries), "passed": totals[0], "failed": totals[1], "ignored": totals[2],
        "filtered": [int(m[5]) for m in summaries], "statuses": [m[0] for m in summaries],
        "ignored_tests": [{"name": n, "reason": reason} for n, reason in ignored],
    }, passed


sources_before = {}
try:
    paths = obj("source-files.json")
    final = obj("final-identities.json")
    first = obj("first-identities.json")
    need(len(paths) == len(set(paths)) == len(final["sources"]) == 52 and set(paths) == set(final["sources"]), "52 final paths")
    sources_before = {p: meta(R / p) for p in paths}
    need(sources_before == final["sources"], "current final source identities")
    for label, identities in (("first", first), ("final", final)):
        for path, m in identities["sources"].items():
            snap = S / ("source-" + label) / (path + ".txt")
            read(snap)
            need(meta(snap) == m, f"snapshot {label}: {path}")
        for name, m in identities["binaries"].items():
            need(meta(m["executed"]) == {"bytes": m["bytes"], "sha256": m["sha256"]}, f"retained {label} {name}")
    need(len(final["binaries"]) == 5 and set(first["binaries"]) == set(final["binaries"]), "five executable identities")
    app_sha = final["binaries"]["deadpan-app"]["sha256"]
    need(app_sha == "a87afae7c9bd81c4bdded69bfda29d8e226072bb647c2267507fa15ad1a8386c", "final App")
    read("run.py")
    read("media.py")
    runner_sha = meta(S / "run.py")["sha256"]
    receipts = {}
    for n in ("cli-models-units", "clippy-final-second", "format-final-second", "release-source-check",
              "media-final", "native-points-final", "native-targets-final", "native-faces-final", "native-zoom-final"):
        receipts[n] = run(n)
        need(receipts[n]["source_before"] == {str(R / p): m for p, m in final["sources"].items()}, f"final source basis {n}")
    for n in ("app-units-final", "models-golden", "release-final", "native-points-first"):
        receipts[n] = run(n)
    receipts["core-plan"] = run("core-plan", unchanged=False)
    receipts["clippy-final"] = run("clippy-final", exit_code=101)
    receipts["commands-write"] = run("commands-write", exit_code=101)
    # Prove the narrowly scoped source exception from actual before/after maps.
    cp = receipts["core-plan"]
    note = obj("core-plan-scope-note.json")
    relevant = {str(R / p) for p in paths if p.startswith(("crates/deadpan-core/", "crates/deadpan-plan/"))}
    changed = {p for p in cp["source_before"] if cp["source_before"][p] != cp["source_after"][p]}
    need(relevant == set(note["relevant_paths"]) and len(relevant) == 6, "Core/Plan scope complete among final owned paths")
    need(changed == set(note["other_changed_paths"]) and not changed & relevant, "exact unrelated during-run changes")
    need(all(cp["source_before"][p] == cp["source_after"][p] == final["sources"][str(Path(p).relative_to(R))] for p in relevant),
         "Core/Plan relevant bytes unchanged and final")
    need(note["core_plan_inputs_unchanged"] is True and note["exit_code"] == 0, "scope note")
    # First snapshot preserves the exact removed test-only no-op conversion.
    model_test = "crates/deadpan-models/src/region_tests.rs"
    old = read(S / "source-first" / (model_test + ".txt"))
    new = read(S / "source-final" / (model_test + ".txt"))
    old_piece = b"size: [200_000, 200_000],\n            }\n            .into(),"
    new_piece = b"size: [200_000, 200_000],\n            },"
    need(old.count(old_piece) == 1 and old.replace(old_piece, new_piece) == new, "exact test-only .into removal")
    allowed_old = dict(final["sources"])
    allowed_old[model_test] = meta(S / "source-first" / (model_test + ".txt"))
    for n in ("app-units-final", "release-final"):
        need(receipts[n]["source_before"] == {str(R / p): m for p, m in allowed_old.items()}, f"only test-only old delta {n}")
    target = Path(receipts["release-source-check"]["environment"]["CARGO_TARGET_DIR"]) / "release"
    for name, m in final["binaries"].items():
        need(meta(target / name) == {"bytes": m["bytes"], "sha256": m["sha256"]}, "fresh build target and retained executable identity")
    live_unit = obj("app-unit-executable-terminal.json")
    need(live_unit["unchanged"] is True and live_unit["terminal"] == {
        k: live_unit["observed_live"][k] for k in ("bytes", "sha256")}, "App unit observed/terminal identity")
    need(meta(live_unit["observed_live"]["path"]) == live_unit["terminal"], "actual App unit executable")
    tests = {}
    for n, expected in (("core-plan", (2041, 0, 0)), ("app-units-final", (1304, 0, 11)),
                        ("cli-models-units", (719, 0, 7)), ("models-golden", (1, 0, 0))):
        result, names = rust_results(n + ".stdout")
        need((result["passed"], result["failed"], result["ignored"]) == expected
             and set(result["statuses"]) == {"ok"}, f"raw final test totals {n}")
        tests[n] = result
    need(tests["cli-models-units"]["summaries"] == 2, "CLI/Models separate harnesses")
    models_names = rust_results("models-golden.stdout")[1]
    need(len(models_names) == 1 and models_names[0].endswith("point_target_identity_bytes_match_the_python_bridge_golden"), "actual golden test")
    worker_names, worker_runs = [], []
    for manifest in ("worker-point-focused-results.json", "worker-point-regression-results.json"):
        for row in obj(manifest):
            need(row["exit"] == 0, "worker terminal")
            result, names = rust_results(row["stdout"])
            read(row["stderr"])
            need(result["failed"] == result["ignored"] == 0, "worker raw result")
            if not names:
                need(row.get("test_count") == 0 and "Incorrect filter" in row.get("note", ""), "zero-test discovery explicitly rejected")
            worker_names += names
            worker_runs.append({"stdout": row["stdout"], "passed": result["passed"], "filtered": result["filtered"],
                                "zero_test_discovery_only": not names})
    need(len(worker_names) == 67 and len(set(worker_names)) == 48, "worker total executions/distinct cases")
    py = obj("worker-python-point-freeze.json")
    for path, h in py["files"].items():
        if not path.endswith(".rs"):
            need(meta(R / path)["sha256"] == h == final["sources"][path]["sha256"], "current Python/fixture freeze")
    python_counts = []
    for check in py["checks"]:
        text = read(check["stderr"]).decode()
        stdout = read(check["stdout"])
        need(check["exit"] == 0 and hashlib.sha256(stdout).hexdigest() == check["stdout_sha256"]
             and hashlib.sha256(text.encode()).hexdigest() == check["stderr_sha256"], "Python raw stream identity")
        counts = re.findall(r"^Ran (\d+) tests? in ", text, re.M)
        need(counts == [str(check["tests"])] and re.search(r"\nOK\s*$", text), "Python actual test summary")
        python_counts += [int(counts[0])]
    need(python_counts == [39, 23], "Python 62")
    native = []
    for name, scenario, expected, identities in (
        ("native-points-first", "points", 33, first), ("native-points-final", "points", 34, final),
        ("native-targets-final", "targets", 41, final), ("native-faces-final", "faces", 66, final),
        ("native-zoom-final", "zoom", 213, final),
    ):
        d = obj(name + "/report.json")
        need(d["failed"] is False and d["metadata"]["build_profile"] == "release", "native passing release report")
        need(d["metadata"]["binary_sha256"] == identities["binaries"]["deadpan-app"]["sha256"], "native executed App identity")
        need(d["metadata"]["physical_display_measured"] is False and d["metadata"]["gpu_readback_in_timing_run"] is False,
             "native proof limit")
        scenarios = {x["name"]: x for x in d["scenarios"]}
        need(set(scenarios) == {"kestrel-shortcuts", scenario} and len(scenarios[scenario]["checks"]) == expected, "native exact count/set")
        need(all(c["passed"] is True for x in scenarios.values() for c in x["checks"])
             and not any(f["severity"] == "failure" for x in scenarios.values() for f in x["findings"]), "native raw successes")
        k = scenarios["kestrel-shortcuts"]["checks"]
        need(len(k) == 1 and k[0]["actual"]["reserved_bindings"] == 62
             and k[0]["actual"]["routing_cases"] == 21885504 and k[0]["actual"]["conflicts"] == []
             and k[0]["actual"]["source_sha256"] == k[0]["actual"]["live_source_sha256"], "Kestrel counters/source consistency")
        argv = receipts[name]["command"]
        need(meta(argv[0])["sha256"] == d["metadata"]["binary_sha256"], "invocation/report identity")
        need(meta(argv[argv.index("--kestrel-source") + 1])["sha256"] == k[0]["actual"]["live_source_sha256"], "actual local Kestrel")
        scene = scenarios[scenario]
        native.append({"name": name, "checks": expected, "kestrel_checks": 1,
                       "warnings": [f["message"].split(": [{")[0].split(": [[")[0] for f in scene["findings"] if f["severity"] == "warning"],
                       "skips": scene["skipped"], "binary_sha256": d["metadata"]["binary_sha256"]})
    commands = obj("media-final/commands.json")
    need([c["name"] for c in commands] == ["dump", "validate", "render", "verify", "copy", "portable-validate", "portable-dump", "checkpoint", "portable-checkpoint"]
         and all(c["exit_code"] == 0 for c in commands), "nine actual media commands")
    for c in commands:
        need(meta(c["command"][0])["sha256"] == final["binaries"]["deadpan-cli"]["sha256"], "actual media CLI identity")
        read("media-final/" + c["name"] + ".stdout")
        read("media-final/" + c["name"] + ".stderr")
    original = obj("media-final/dump.stdout")
    portable = obj("media-final/portable-dump.stdout")
    need(original == portable, "exact portable authored document")
    target_record = original["targets"]["target-1"]
    need(target_record["region"] == {"center": [530000, 480000]}
         and target_record["corrections"] == [{"at": 30030, "region": {"center": [610000, 550000]}}],
         "actual saved/corrected point without synthetic size")
    validate = obj("media-final/validate.stdout")
    need(validate == obj("media-final/portable-validate.stdout") and validate["valid"] is True
         and validate["revision_id"] == original["revision_id"], "actual Store validation")
    v = obj("media-final/verification.json")
    need(v == obj("media-final/verify.stdout") and v["passed"] is True and v["failures"] == []
         and v["range"] == [0, 120] and v["frame_rate"] == [30000, 1001]
         and v["revision_id"] == original["revision_id"], "actual emitted verification")
    need(len(v["pictures"]) == 120 and all(p["passed"] is True and p["flags"] == [] for p in v["pictures"])
         and [p["output_frame"] for p in v["pictures"]] == list(range(120)), "complete picture rows")
    observable = collections.Counter(p[side]["status"] for p in v["pictures"] for side in ("previous_index", "next_index") if p[side] is not None)
    need(observable == {"observable": 202, "unobservable": 36}
         and v["summary"]["index_observable_comparisons"] == 202
         and v["summary"]["index_unobservable_comparisons"] == 36
         and v["summary"]["pictures_without_index_neighbors"] == 0, "actual observability counts")
    audio = v["audio"]
    need(len(audio) == 1 and audio[0]["window"] == audio[0]["project_samples"] == [0, 192192]
         and audio[0]["uncovered_samples"] == 0 and audio[0]["passed"] is True and audio[0]["flags"] == []
         and audio[0]["offset_status"] == "verified_zero" and audio[0]["measured_offset_samples"] == 0
         and all(p["peak_lag"] == 0 and p["status"] == "verified_zero" for p in audio[0]["segments"]), "complete zero-offset audio window")
    m = v["movie"]
    need(m["expected_audio_samples"] == m["audio"]["presented_end"] == 192192 and m["audio"]["gaps"] == []
         and m["audio"]["first_pts"] == -2048 and m["audio"]["physical_samples"] == 194560
         and m["audio_edits"] == {"edits": 1, "empty_edits": 0, "media_time": 2048, "presented_media_ticks": 192192}
         and m["video_edits"] == {"edits": 1, "empty_edits": 0, "media_time": 0, "presented_media_ticks": 120120}
         and m["video_frames"] == m["expected_video_frames"] == 120 and m["picture_anomalies"] == [], "actual video/AAC edit metadata")
    movie = meta(S / "media-final/point-target.mp4")
    need(movie == {"bytes": 191679, "sha256": "8f179d12c912fecbd21e8c610457b0131054ec5ed4f99f95c1b907d0171dbdc0"}, "actual movie identity")
    media_result = obj("media-final/result.json")
    need(media_result == json.loads(read("media-final.stdout")) and media_result["movie_sha256"] == movie["sha256"]
         and media_result["movie_bytes"] == movie["bytes"] and media_result["commands"] == 9
         and media_result["portable_document_equal"] is True, "media summary counterpart")
    pubs = [d for line in read("media-final/render.stdout").splitlines() for d in walk(json.loads(line)) if "movie_sha256" in d]
    need(pubs and all(d == pubs[0] for d in pubs), "publication receipt")
    pub = pubs[0]
    need(pub["movie_sha256"] == movie["sha256"] and pub["movie_bytes"] == movie["bytes"]
         and meta(S / "media-final" / Path(pub["report"]).name) == {"bytes": pub["report_bytes"], "sha256": pub["report_sha256"]},
         "actual render report/movie binding")
    history_receipt = obj("media-final/copy.stdout")["portable_copy"]["verified_history"]
    need(history_receipt == {"replayed": 0, "revisions": 10, "verified_by_receipt": 10}, "portable history receipt")
    histories = []
    db_pins = {}
    for directory in (Path(commands[0]["command"][3]), S / "media-final/portable.deadpan"):
        db = directory / "project.sqlite"
        wal = directory / "project.sqlite-wal"
        need(not wal.exists() or wal.stat().st_size == 0, "nonempty WAL before immutable read")
        db_before = meta(db)
        with sqlite3.connect(db.as_uri() + "?mode=ro&immutable=1", uri=True) as con:
            need(con.execute("PRAGMA quick_check").fetchall() == [("ok",)], "SQLite integrity")
            tables = {t: sorted(con.execute('SELECT * FROM "' + t + '"').fetchall(), key=repr)
                      for t in ("revisions", "revision_patches", "history", "history_receipt", "redo", "state", "transaction_steps")}
        need(meta(db) == db_before, "SQLite read mutated file")
        histories.append(tables)
        db_pins[str(db)] = db_before
    need(histories[0] == histories[1] and len(histories[0]["revisions"]) == 10, "all retained history rows equal")
    diagnostics = {}
    for p in sorted(S.iterdir()):
        if p.suffix in (".stdout", ".stderr") and not p.name.startswith("audit-point-evidence"):
            text = read(p).decode(errors="replace")
            lines = [x for x in text.splitlines() if x.startswith(("error:", "error[", "warning:", "warning[", "test result: FAILED"))]
            if lines:
                diagnostics[p.name] = lines
    need("useless conversion" in read("clippy-final.stderr").decode(), "retained initial Clippy failure")
    need("Finished" in read("clippy-final-second.stderr").decode(), "final strict completion")
    need(read("format-final-second.stdout") == read("format-final-second.stderr") == b"", "final format clean logs")
    out.update(
        status="no_findings", findings=[], final_source_count=52, final_source_types=dict(collections.Counter(Path(p).suffix for p in paths)),
        final_sources=final["sources"], final_binaries=final["binaries"], final_identities_sha256=meta(S / "final-identities.json")["sha256"],
        core_plan_scope={"relevant_unchanged_paths": sorted(relevant), "other_changed_during_run": sorted(changed),
                         "whole_scope_guard": False, "relevant_scope_unchanged": True},
        test_only_models_delta={"path": model_test, "before_sha256": hashlib.sha256(old).hexdigest(),
                                "after_sha256": hashlib.sha256(new).hexdigest(), "exact_useless_into_removal": True},
        tests=tests, worker_rust_executions=67, worker_rust_distinct_tests=48, worker_runs=worker_runs,
        python_test_counts=python_counts, python_total_tests=62, native_reports=native,
        actual_media_commands=9, movie=movie, picture_rows=120, picture_neighbor_observability=dict(observable),
        audio_window=[0, 192192], measured_audio_offset=0, frame_rate=[30000, 1001], actual_movie_metadata=m,
        portable_authored_document_equal=True, portable_revision_count=10, portable_history_row_count=len(histories[0]["history"]),
        original_portable_all_selected_history_tables_equal=True, immutable_database_pins=db_pins,
        retained_historical_diagnostics=diagnostics, original_unit_executable=live_unit["terminal"],
    )
except Exception as exc:
    traceback.print_exc()
    out["status"] = "findings"
    out["findings"].append({"type": type(exc).__name__, "message": str(exc)})
finally:
    out["current_scoped_sources_unchanged"] = sources_before == {p: meta(R / p) for p in sources_before}
    if not out["current_scoped_sources_unchanged"]:
        out["status"] = "findings"
        out["findings"].append({"message": "Scoped source changed during audit"})
    out["input_file_pins"] = pins
    out["verifier_sha256"] = meta(Path(__file__))["sha256"]
    out["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (S / "audit-point-evidence.json").open("x") as f:
        json.dump(out, f, indent=2, sort_keys=True)
        f.write("\n")
    print(json.dumps({k: out.get(k) for k in ("status", "findings", "final_source_count", "worker_rust_distinct_tests",
                                            "python_total_tests", "picture_rows", "portable_revision_count",
                                            "current_scoped_sources_unchanged")}, sort_keys=True))
sys.exit(0 if out["status"] == "no_findings" else 1)
