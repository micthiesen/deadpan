#!/usr/bin/env python3
"""Independent offline audit of terminal Hold captured-geometry qualification."""
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

S = Path("/tmp/deadpan-hold-follow-creep-2026-10-24")
R = Path("/Users/michael/Code/deadpan")
PINS = {}


def need(ok, message):
    if not ok:
        raise AssertionError(message)


def meta(path):
    p = Path(path)
    need(stat.S_ISREG(p.lstat().st_mode), f"not regular: {p}")
    h = hashlib.sha256()
    n = 0
    with p.open("rb") as f:
        for b in iter(lambda: f.read(1024 * 1024), b""):
            h.update(b)
            n += len(b)
    return {"bytes": n, "sha256": h.hexdigest()}


def read(path):
    p = Path(path)
    if not p.is_absolute():
        p = S / p
    m = meta(p)
    need(m["bytes"] <= 268435456, "oversized data input")
    PINS[str(p)] = m
    return p.read_bytes()


def obj(path):
    return json.loads(read(path))


def receipt(name, runner_sha, expected_exit=0):
    d = obj(name + ".json")
    need(d["cwd"] == str(R) and d["exit_code"] == expected_exit and "finished_at" in d,
         f"incomplete/incorrect terminal: {name}")
    need(d["seconds"] >= 0, f"negative elapsed: {name}")
    if "runner_sha256" in d:
        need(d["runner_sha256"] == runner_sha, f"runner bytes: {name}")
    need(d["source_before"] == d["source_after"] and d["sources_unchanged"] is True,
         f"changed receipt scope: {name}")
    need(d["binary_before"] == d["binary_after"] and d["binaries_unchanged"] is True,
         f"changed binary scope: {name}")
    for p, m in d["streams"].items():
        read(p)
        need(meta(p) == m, f"raw stream bytes: {name}")
    for p, m in d["binary_before"].items():
        need(meta(p) == m, f"executed retained bytes: {name}")
    return d


def tests(path):
    text = read(path).decode()
    summaries = re.findall(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", text, re.M)
    need(summaries, f"missing raw summary: {path}")
    passed = re.findall(r"^test (.+) \.\.\. ok$", text, re.M)
    ignored = re.findall(r"^test (.+) \.\.\. ignored(?:, (.*))?$", text, re.M)
    total = tuple(sum(int(m[i]) for m in summaries) for i in (1, 2, 3))
    need(len(passed) == total[0] and len(ignored) == total[2], f"named test counts: {path}")
    return {
        "passed": total[0], "failed": total[1], "ignored": total[2],
        "summaries": len(summaries), "statuses": [m[0] for m in summaries],
        "filtered": [int(m[5]) for m in summaries],
        "ignored_tests": [{"name": n, "reason": reason} for n, reason in ignored],
        "passed_names": passed,
    }


def dictionaries(value):
    if isinstance(value, dict):
        yield value
        for v in value.values():
            yield from dictionaries(v)
    elif isinstance(value, list):
        for v in value:
            yield from dictionaries(v)


def main():
    result = {
        "schema_version": 1, "status": "running", "findings": [],
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "base": "efd84b88324d1b71dfe339b19c097c376e64783f",
        "limitations": [
            "No product, Cargo, test, decoder, worker, model, publishing or Git command was executed by this auditor.",
            "Final scope is twenty source paths. Earlier sixteen/seventeen/nineteen-path receipts are retained with their actual scope, not described as final-twenty runs.",
            "The two renderer tests include a separately pinned 960-case geometry cross-oracle. This is not an independent DSP, encoded-audio or listening qualification.",
            "Native input/offscreen Metal is scripted, not physical display/input, OS picker/IME or VoiceOver qualification. No latency or requirement completion is inferred.",
            "Movie verification uses the shared committed picture/limited-audio reference. Full sample-window coverage is not a bit-identical encoded PCM claim.",
            "Picture-neighbor observability limits and screenshot/layout warnings remain explicit. The first semantic replay passed before Root's visual label-overlap correction.",
        ],
    }
    source_before = {}
    try:
        final, first = obj("final-identities.json"), obj("first-identities.json")
        paths = obj("source-files.json")
        need(len(paths) == len(set(paths)) == len(final["sources"]) == 20 and set(paths) == set(final["sources"]), "final twenty paths")
        source_before = {p: meta(R / p) for p in paths}
        need(source_before == final["sources"], "current final twenty")
        for label, ids in (("first", first), ("final", final)):
            for p, m in ids["sources"].items():
                snap = S / ("source-" + label) / (p + ".txt")
                read(snap)
                need(meta(snap) == m, f"retained {label} snapshot")
            for name, m in ids["binaries"].items():
                need(meta(m["executed"]) == {"bytes": m["bytes"], "sha256": m["sha256"]}, f"retained {label} {name}")
        need(len(final["binaries"]) == 5, "five sibling executables")
        app = final["binaries"]["deadpan-app"]["sha256"]
        need(app == "e46f0e4fca6c22be1f4eeb43de05040c56be3e32b465023c7c79b855bb27a126", "final App pin")
        read("run.py")
        read("media.py")
        runner = meta(S / "run.py")["sha256"]
        runs = {}
        final_labels = ["app-units-final-second", "release-final-second", "clippy-final-second", "format-final-second", "cli-hold-final",
                        "native-hold-final", "native-faces-final", "native-targets-final", "native-zoom-final",
                        "native-points-final", "media-final"]
        absolute_final = {str(R / p): m for p, m in final["sources"].items()}
        for n in final_labels:
            runs[n] = receipt(n, runner)
            need(runs[n]["source_before"] == absolute_final, f"actual final20 basis: {n}")
        for n in ("core-plan-final", "renderer-capture-oracle", "worker-cli-review-1", "camera-units-final",
                  "app-units-final", "native-hold-first", "release-first", "release-final", "clippy-final"):
            runs[n] = receipt(n, runner)
        runs["commands-write"] = receipt("commands-write", runner, expected_exit=101)
        scope_counts = {n: len(d["source_before"]) for n, d in runs.items()}
        need(scope_counts["core-plan-final"] == 16 and scope_counts["renderer-capture-oracle"] == 17
             and scope_counts["worker-cli-review-1"] == 17 and scope_counts["native-hold-first"] == 19, "historical actual scope counts")
        relevant = {str(R / p) for p in paths if p.startswith(("crates/deadpan-core/", "crates/deadpan-plan/"))}
        scope_note = obj("core-plan-scope-note.json")
        need(len(relevant) == 9 and relevant == set(scope_note["core_plan_paths"]), "complete relevant Core/Plan set")
        need(all(runs["core-plan-final"]["source_before"][p] == absolute_final[p] for p in relevant), "Core/Plan inputs final-equivalent")
        renderer_pins = obj("renderer-oracle-source-pins.json")
        need(len(renderer_pins["files"]) == 2 and all(final["sources"][p] == m for p, m in renderer_pins["files"].items()),
             "separate two renderer source pins")
        need(renderer_pins["files"]["crates/deadpan-render/tests/follow_letterbox/captured.rs"]["sha256"]
             == "58a601c4a7ceec93b9959435219fd1ac2a3a6266c906f3f32c8c961505ebb4c9", "reviewed 960-case source")
        for p in paths:
            if p.startswith("crates/deadpan-cli/"):
                need(runs["cli-hold-final"]["source_before"][str(R / p)] == absolute_final[str(R / p)], "fresh strengthened CLI source final-equivalent")
        unit = obj("app-unit-final-second-terminal.json")
        need(unit["unchanged"] is True and unit["terminal"] == {k: unit["observed_live"][k] for k in ("bytes", "sha256")}
             and meta(unit["observed_live"]["path"]) == unit["terminal"]
             and unit["terminal"]["sha256"] == "16747f441dc7a902f6fb184c365f70d84ac07d670520c659d290eef52db3e16e", "actual final App unit executable")
        raw_tests = {}
        for n, expected in (("core-plan-final", (2047, 0, 0)), ("renderer-capture-oracle", (2, 0, 0)),
                            ("worker-cli-review-1", (1, 0, 0)), ("cli-hold-final", (1, 0, 0)), ("camera-units-final", (37, 0, 0)),
                            ("app-units-final", (1304, 0, 11)), ("app-units-final-second", (1304, 0, 11)),
                            ("plan-focused-2", (5, 0, 0)), ("plan-picture-all", (127, 0, 0))):
            d = tests(n + ".stdout")
            need((d["passed"], d["failed"], d["ignored"]) == expected and set(d["statuses"]) == {"ok"}, f"raw test totals: {n}")
            raw_tests[n] = d
        need(sorted(raw_tests["app-units-final-second"]["ignored_tests"], key=lambda x: x["name"]) ==
             sorted(raw_tests["app-units-final"]["ignored_tests"], key=lambda x: x["name"]), "same existing opt-in ignores")
        need(raw_tests["cli-hold-final"]["passed_names"] == raw_tests["worker-cli-review-1"]["passed_names"]
             and raw_tests["cli-hold-final"]["filtered"] == [57], "same strengthened CLI case actually executed on final20")
        read("plan-run.py")
        for n in ("plan-focused-2", "plan-picture-all"):
            d = obj(n + ".json")
            need(d["exit_code"] == 0 and d["cwd"] == str(R) and "finished_at" in d
                 and d["sources_unchanged"] is True and d["source_before"] == d["source_after"],
                 "actual Plan-run terminal/source schema")
            read(n + ".stdout")
            read(n + ".stderr")
            need(len(d["source_before"]) == 5 and all(final["sources"][p] == m for p, m in d["source_before"].items()), "actual five-path Plan input scope")
        native = []
        for n, scenario, count, ids in (
            ("native-hold-first", "hold-follow-creep", 64, first),
            ("native-hold-final", "hold-follow-creep", 67, final), ("native-faces-final", "faces", 69, final),
            ("native-targets-final", "targets", 41, final), ("native-zoom-final", "zoom", 213, final),
            ("native-points-final", "points", 34, final),
        ):
            report = obj(n + "/report.json")
            need(report["failed"] is False and report["metadata"]["build_profile"] == "release"
                 and report["metadata"]["binary_sha256"] == ids["binaries"]["deadpan-app"]["sha256"], "native raw result/binary")
            need(report["metadata"]["physical_display_measured"] is False and report["metadata"]["gpu_readback_in_timing_run"] is False, "native physical limit")
            scenes = {x["name"]: x for x in report["scenarios"]}
            need(set(scenes) == {"kestrel-shortcuts", scenario} and len(scenes[scenario]["checks"]) == count, "native exact scenario/count")
            need(all(c["passed"] is True for x in scenes.values() for c in x["checks"])
                 and not any(f["severity"] == "failure" for x in scenes.values() for f in x["findings"]), "native raw success checks")
            k = scenes["kestrel-shortcuts"]["checks"]
            need(len(k) == 1 and k[0]["actual"]["reserved_bindings"] == 62 and k[0]["actual"]["routing_cases"] == 21885504
                 and k[0]["actual"]["conflicts"] == [], "Kestrel raw counts")
            argv = runs[n]["command"]
            need(meta(argv[0])["sha256"] == report["metadata"]["binary_sha256"], "executed native identity")
            need(meta(argv[argv.index("--kestrel-source") + 1])["sha256"] == k[0]["actual"]["live_source_sha256"]
                 == k[0]["actual"]["source_sha256"], "live Kestrel source binding")
            scene = scenes[scenario]
            native.append({"name": n, "checks": count, "kestrel_checks": 1,
                           "warnings": [f["message"].split(": [{")[0].split(": [[")[0] for f in scene["findings"] if f["severity"] == "warning"],
                           "skips": scene["skipped"], "binary_sha256": report["metadata"]["binary_sha256"]})
        commands = obj("media-final/commands.json")
        need([c["name"] for c in commands] == ["dump", "validate", "render", "verify", "copy", "portable-validate",
                                              "portable-dump", "checkpoint", "portable-checkpoint"]
             and all(c["exit_code"] == 0 for c in commands), "nine real media commands")
        for c in commands:
            need(meta(c["command"][0])["sha256"] == final["binaries"]["deadpan-cli"]["sha256"], "actual media CLI identity")
            read("media-final/" + c["name"] + ".stdout")
            read("media-final/" + c["name"] + ".stderr")
        document = obj("media-final/dump.stdout")
        need(document == obj("media-final/portable-dump.stdout"), "exact portable authored document")
        valid = obj("media-final/validate.stdout")
        need(valid == obj("media-final/portable-validate.stdout") and valid["valid"] is True
             and valid["revision_id"] == document["revision_id"], "real Store validation")
        holds = [n for n in document["nodes"].values() if n["kind"]["type"] == "hold"]
        need(len(holds) == 1 and holds[0]["kind"]["recipe"]["picture_context"]["canvases"], "actual captured Hold recipe")
        framing = holds[0]["framing"]["value"]
        need(framing["type"] == "follow" and framing["target"] == "target-2"
             and framing["scale"] == {"numerator": "11", "denominator": "10"}
             and framing["scale_transition"] == {"factor": {"numerator": "2", "denominator": "1"}, "curve": "linear"}, "actual Hold live Follow creep")
        need(document["targets"]["target-1"]["region"] == {"center": [580000, 500000]}
             and document["targets"]["target-2"]["region"] == {"center": [680000, 500000]}
             and not document["targets"]["target-2"].get("corrections", []), "saved point target bases")
        v = obj("media-final/verification.json")
        need(v == obj("media-final/verify.stdout") and v["passed"] is True and v["failures"] == []
             and v["range"] == [0, 132] and v["frame_rate"] == [30000, 1001]
             and v["revision_id"] == document["revision_id"], "actual emitted verification")
        need(len(v["pictures"]) == 132 and all(p["passed"] is True and p["flags"] == [] for p in v["pictures"])
             and [p["output_frame"] for p in v["pictures"]] == list(range(132)), "complete actual picture rows")
        observations = collections.Counter(p[k]["status"] for p in v["pictures"] for k in ("previous_index", "next_index") if p[k] is not None)
        need(observations == {"observable": 216, "unobservable": 46}
             and v["summary"]["index_observable_comparisons"] == 216 and v["summary"]["index_unobservable_comparisons"] == 46, "temporal observability counts")
        a = v["audio"]
        need(len(a) == 1 and a[0]["passed"] is True and a[0]["flags"] == [] and a[0]["window"] == a[0]["project_samples"] == [0, 211411]
             and a[0]["uncovered_samples"] == 0 and a[0]["offset_status"] == "verified_zero" and a[0]["measured_offset_samples"] == 0
             and all(x["peak_lag"] == 0 and x["status"] == "verified_zero" for x in a[0]["segments"]), "full zero-offset audio coverage")
        movie = meta(S / "media-final/hold-follow-creep.mp4")
        need(movie == {"bytes": 44541, "sha256": "276e0755b2fbd8c8904e549695442bb514648f147893b2ae6fad8b9f1a2d70e7"}, "actual movie bytes")
        m = v["movie"]
        need(m["bytes"] == movie["bytes"] and m["video_frames"] == m["expected_video_frames"] == 132 and m["picture_anomalies"] == []
             and m["audio"]["gaps"] == [] and m["audio"]["presented_end"] == m["expected_audio_samples"] == 211411
             and m["audio"]["first_pts"] == -2048 and m["audio"]["physical_samples"] == 214016
             and m["audio_edits"] == {"edits": 1, "empty_edits": 0, "media_time": 2048, "presented_media_ticks": 211411}
             and m["video_edits"] == {"edits": 1, "empty_edits": 0, "media_time": 0, "presented_media_ticks": 132132}, "actual video/AAC edit metadata")
        mr = obj("media-final/result.json")
        need(mr == json.loads(read("media-final.stdout")) and mr["movie_sha256"] == movie["sha256"]
             and mr["movie_bytes"] == movie["bytes"] and mr["expected_frames"] == 132 and mr["expected_audio_samples"] == 211411
             and mr["portable_document_equal"] is True and mr["commands"] == 9, "media raw summary counterpart")
        pubs = [d for line in read("media-final/render.stdout").splitlines() for d in dictionaries(json.loads(line)) if "movie_sha256" in d]
        need(pubs and all(d == pubs[0] for d in pubs) and pubs[0]["movie_sha256"] == movie["sha256"]
             and pubs[0]["movie_bytes"] == movie["bytes"], "render publication receipt/movie binding")
        pub = pubs[0]
        need(meta(S / "media-final" / Path(pub["report"]).name) == {"bytes": pub["report_bytes"], "sha256": pub["report_sha256"]}, "actual render report hash")
        need(obj("media-final/copy.stdout")["portable_copy"]["verified_history"]
             == {"replayed": 0, "revisions": 18, "verified_by_receipt": 18}, "portable full history receipt")
        history, db_pins = [], {}
        for root in (Path(commands[0]["command"][3]), S / "media-final/portable.deadpan"):
            db = root / "project.sqlite"
            wal = root / "project.sqlite-wal"
            need(not wal.exists() or wal.stat().st_size == 0, "nonempty WAL before immutable read")
            before = meta(db)
            with sqlite3.connect(db.as_uri() + "?mode=ro&immutable=1", uri=True) as con:
                need(con.execute("PRAGMA quick_check").fetchall() == [("ok",)], "read-only SQLite integrity")
                tables = {t: sorted(con.execute('SELECT * FROM "' + t + '"').fetchall(), key=repr)
                          for t in ("revisions", "revision_patches", "history", "history_receipt", "redo", "state", "transaction_steps")}
            need(meta(db) == before, "SQLite bytes unchanged")
            history.append(tables)
            db_pins[str(db)] = before
        need(history[0] == history[1] and len(history[0]["revisions"]) == 18, "all retained history rows equal")
        diagnostics = {}
        for p in sorted(S.iterdir()):
            if p.suffix in (".stdout", ".stderr") and not p.name.startswith("audit-hold-evidence"):
                lines = [x for x in read(p).decode(errors="replace").splitlines()
                         if x.startswith(("error:", "error[", "warning:", "warning[", "test result: FAILED"))]
                if lines:
                    diagnostics[p.name] = lines
        need(read("format-final-second.stdout") == read("format-final-second.stderr") == b"", "clean final format streams")
        reviews = obj("review-notes.json")
        need(reviews["base"] == result["base"], "retained review base")
        visual = obj("visual-review.json")
        for p, pin in visual["images"].items():
            read(p)
            need(meta(S / p) == pin, "Root inspected image byte identity")
        result.update(status="no_findings", findings=[], final_source_count=20, final_sources=final["sources"],
                      final_binaries=final["binaries"], final_identities_sha256=meta(S / "final-identities.json")["sha256"],
                      actual_receipt_scope_counts=scope_counts, core_plan_relevant_unchanged_paths=sorted(relevant),
                      renderer_geometry_case_dimensions=[4, 5, 3, 2, 4, 2], renderer_geometry_cases=960,
                      renderer_separate_source_pins=renderer_pins["files"], tests=raw_tests, native_reports=native,
                      final_unit_executable=unit["terminal"], actual_media_commands=9, movie=movie,
                      picture_rows=132, picture_neighbor_observability=dict(observations), audio_window=[0, 211411], audio_offset=0,
                      actual_movie_metadata=m, portable_authored_document_exact_equal=True, portable_revisions=18,
                      portable_history_rows=len(history[0]["history"]), all_selected_history_tables_equal=True,
                      immutable_database_pins=db_pins, retained_historical_diagnostics=diagnostics,
                      source_review_notes_sha256=meta(S / "review-notes.json")["sha256"],
                      root_visual_review_sha256=meta(S / "visual-review.json")["sha256"],
                      root_visual_review=visual)
    except Exception as exc:
        traceback.print_exc()
        result["status"] = "findings"
        result["findings"].append({"type": type(exc).__name__, "message": str(exc)})
    finally:
        result["current_scoped_sources_unchanged"] = source_before == {p: meta(R / p) for p in source_before}
        if not result["current_scoped_sources_unchanged"]:
            result["status"] = "findings"
            result["findings"].append({"message": "Scoped source changed during audit"})
        result["input_file_pins"] = PINS
        result["preserved_first_attempt"] = {
            "path": "audit-history/first-cli-source-pin-mismatch",
            "cause": "Prior CLI test receipt binds pre-format bytes unavailable as a retained source snapshot; the fresh cli-hold-final pass binds final20 instead.",
            "preservation_manifest": meta(S / "audit-history/first-cli-source-pin-mismatch/preservation.json"),
            "product_failure": False,
        }
        result["verifier_sha256"] = meta(Path(__file__))["sha256"]
        result["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        with (S / "audit-hold-evidence.json").open("x") as f:
            json.dump(result, f, indent=2, sort_keys=True)
            f.write("\n")
        print(json.dumps({k: result.get(k) for k in ("status", "findings", "final_source_count", "picture_rows",
                                                    "portable_revisions", "portable_history_rows",
                                                    "current_scoped_sources_unchanged")}, sort_keys=True))
    return 0 if result["status"] == "no_findings" else 1


if __name__ == "__main__":
    sys.exit(main())
