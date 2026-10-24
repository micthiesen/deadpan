#!/usr/bin/env python3
"""Independent offline audit of terminal ordinary Sequence qualification."""
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

S = Path("/tmp/deadpan-sequence-follow-creep-2026-10-24")
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
    out = {
        "schema_version": 1, "status": "running", "findings": [],
        "base": "fea6b081775165f9830ba14d938576d346fec6c8",
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "limitations": [
            "Offline read-only audit. No product, Cargo, test, worker, model, decoder, publisher or Git command was executed.",
            "Fourteen final source paths are scoped witnesses, not a complete repository compilation claim.",
            "One release candidate was exercised. First/final sources and all five executable byte identities are equal; no duplicate Sequence workflow run is claimed.",
            "Historical worker test executable hashes are recorded observations of mutable Cargo products, not archived binary bytes or substituted runtime copies.",
            "Native input and offscreen Metal are scripted. No physical display/input, OS picker/IME, VoiceOver, listening or latency qualification is inferred.",
            "Movie comparison uses shared committed picture/limited-audio semantics, not independent DSP or encoded PCM bit equality.",
            "Six picture-neighbor comparisons are unobservable. All warnings, skips, eleven existing opt-in ignores and the initial CLI fixture failure remain explicit.",
            "Focused Plan's 287-path src basis predates one Core cfg(test) file update. Its two owned Plan tests are current and both pass in final Core/Plan2049; this is recorded rather than claiming all287 old hashes remain current.",
            "Only affected Hold/Zoom regressions were rerun; no new face/person/tracking accuracy or unrelated workflow qualification is claimed.",
        ],
    }
    before = {}
    try:
        final, first = obj("final-identities.json"), obj("first-identities.json")
        paths = obj("source-files.json")
        need(len(paths) == len(set(paths)) == 14 and set(paths) == set(final["sources"]), "final14 scope")
        before = {p: meta(R / p) for p in paths}
        need(before == final["sources"] == first["sources"], "current/first/final source equality")
        for phase, ids in (("first", first), ("final", final)):
            for p, m in ids["sources"].items():
                snap = S / ("source-" + phase) / (p + ".txt")
                read(snap)
                need(meta(snap) == m, f"{phase} snapshot bytes")
            for name, m in ids["binaries"].items():
                need(meta(m["executed"]) == {"bytes": m["bytes"], "sha256": m["sha256"]}, f"{phase} retained runtime binary")
        need(set(first["binaries"]) == set(final["binaries"]) and len(final["binaries"]) == 5
             and all({k: first["binaries"][n][k] for k in ("bytes", "sha256")} ==
                     {k: final["binaries"][n][k] for k in ("bytes", "sha256")} for n in final["binaries"]), "five candidate byte identities")
        eq = obj("candidate-equivalence.json")
        need(eq["source_paths"] == 14 and eq["all_source_bytes_identical"] is True
             and eq["all_five_binary_bytes_identical"] is True, "candidate-equivalence actual proof")
        need(final["binaries"]["deadpan-app"]["sha256"] == "ed85fa1bac7a809b5ba4c5b77d758ff08c5a3fdd8ad9a8338034c48bc0623434"
             and final["binaries"]["deadpan-cli"]["sha256"] == "923245545410e43a4d1248c03206a594e90ffff1ff0d83312ccdc69744d1c700", "final App/CLI pins")
        read("run.py")
        read("media.py")
        runner = meta(S / "run.py")["sha256"]
        runs = {}
        for n in ("core-plan-final", "app-units-final", "release-final", "clippy-final", "format-first",
                  "app-check-first", "cli-framing-final", "native-sequence-first", "native-hold-final",
                  "native-zoom-final", "media-final"):
            runs[n] = receipt(n, runner)
            need(runs[n]["source_before"] == {str(R / p): m for p, m in final["sources"].items()}, f"actual final14 receipt: {n}")
        runs["commands-write"] = receipt("commands-write", runner, expected_exit=101)
        unit = obj("app-unit-executable-terminal.json")
        need(unit["unchanged"] is True and unit["terminal"] == {k: unit["observed_live"][k] for k in ("bytes", "sha256")}
             and meta(unit["observed_live"]["path"]) == unit["terminal"]
             and unit["terminal"]["sha256"] == "e64c4181da7f7039c06033dae4ce09e2a66bfbad8eb52881102db02840f75300", "full App unit executable live/terminal binding")
        results = {}
        for n, expected in (("core-plan-final", (2049, 0, 0)), ("app-units-final", (1304, 0, 11)),
                            ("cli-framing-final", (13, 0, 0)), ("worker-core-1", (2, 0, 0)),
                            ("worker-cli-1", (0, 1, 0)), ("worker-cli-2", (1, 0, 0)),
                            ("plan-focused-1", (2, 0, 0))):
            d = tests(n + ".stdout")
            need((d["passed"], d["failed"], d["ignored"]) == expected, f"raw test totals: {n}")
            need(set(d["statuses"]) == ({"FAILED"} if n == "worker-cli-1" else {"ok"}), "raw test status")
            results[n] = d
        need(any("sequence_follow_creep" in n for n in results["cli-framing-final"]["passed_names"])
             and any("hold_follow_creep" in n for n in results["cli-framing-final"]["passed_names"])
             and results["cli-framing-final"]["filtered"] == [46], "new/existing framing integration cases actually ran")
        worker = obj("worker-sequence-final-freeze.json")
        need(worker["base"] == out["base"], "worker scope base")
        worker_basis = {}
        worker_observations = {}
        case = "crates/deadpan-cli/tests/project_commands/macros/framing/sequence_follow_creep.rs"
        for n, exit_code in (("worker-core-1", 0), ("worker-cli-1", 101), ("worker-cli-2", 0)):
            d = obj(n + ".json")
            need(d["cwd"] == str(R) and d["exit_code"] == exit_code and "finished_at" in d
                 and d["owned_unchanged"] is True and d["owned_before"] == d["owned_after"]
                 and d["basis_unchanged"] is True and d["basis_before"] == d["basis_after"], "worker terminal/owned/basis scope")
            need(len(d["basis_before"]) == 477 and all(meta(R / p)["sha256"] == h for p, h in d["basis_before"].items()), "current production basis477")
            worker_basis[n] = len(d["basis_before"])
            for suffix, h in d["stream_pins"].items():
                need(meta(S / (n + suffix))["sha256"] == h, "worker raw stream pins")
            changed_from_final = [p for p, h in d["owned_before"].items() if final["sources"][p]["sha256"] != h]
            need(changed_from_final == ([] if n == "worker-cli-2" else [case]), "worker changes confined to initial CLI fixture")
            worker_observations[n] = {"owned_paths": len(d["owned_before"]), "production_basis_paths": 477,
                                      "later_owned_changes": changed_from_final, "observed_test_binary_hashes": d["executed_binary_pins"]}
        for p, h in worker["files"].items():
            need(final["sources"][p]["sha256"] == h
                 and meta(S / "worker-source-final" / (p + ".txt"))["sha256"] == h, "formatted final worker source copy")
        failure = worker["retained_failure"]
        need(meta(S / failure["source_copy"])["sha256"] == failure["source_sha256"]
             == obj("worker-cli-1.json")["owned_before"][case] and failure["production_defect"] is False,
             "actual initial failed fixture source retained")
        read(failure["source_copy"])
        pd = obj("plan-focused-1.json")
        need(pd["exit_code"] == 0 and "finished_at" in pd and pd["owned_sources_unchanged"] is True
             and pd["production_basis_unchanged"] is True and pd["source_before"] == pd["source_after"]
             and pd["basis_before"] == pd["basis_after"] and len(pd["basis_before"]) == 287, "Plan actual scope/terminal")
        need(all(final["sources"][p] == m for p, m in pd["source_before"].items()), "current owned Plan test files")
        basis_delta = {p: {"focused": m, "current": meta(R / p)}
                       for p, m in pd["basis_before"].items() if meta(R / p) != m}
        test_only_path = "crates/deadpan-core/src/semantic/planner/tests/framing.rs"
        need(set(basis_delta) == {test_only_path}
             and basis_delta[test_only_path]["current"] == final["sources"][test_only_path],
             "only later test-only Core basis delta")
        planner_module = read(R / "crates/deadpan-core/src/semantic/planner.rs")
        test_module = read(R / "crates/deadpan-core/src/semantic/planner/tests.rs")
        need(b"#[cfg(test)]\nmod tests;" in planner_module and b"mod framing;" in test_module,
             "Core planner framing file is nested under cfg(test)")
        focused_plan_names = results["plan-focused-1"]["passed_names"]
        need(all(results["core-plan-final"]["passed_names"].count(n) == 1 for n in focused_plan_names),
             "both exact Plan cases pass once in final current Core/Plan suite")
        plan_scope = {"owned": 2, "production_basis": 287, "basis_paths_still_identical": 286,
                      "later_cfg_test_only_delta": basis_delta,
                      "final_current_broad_suite_passes_both_exact_plan_cases": focused_plan_names}
        native = []
        for n, scenario, count in (("native-sequence-first", "sequence-follow-creep", 107),
                                  ("native-hold-final", "hold-follow-creep", 67), ("native-zoom-final", "zoom", 213)):
            d = obj(n + "/report.json")
            need(d["failed"] is False and d["metadata"]["build_profile"] == "release"
                 and d["metadata"]["binary_sha256"] == final["binaries"]["deadpan-app"]["sha256"]
                 and d["metadata"]["physical_display_measured"] is False
                 and d["metadata"]["gpu_readback_in_timing_run"] is False, "native metadata/pass")
            scenes = {x["name"]: x for x in d["scenarios"]}
            need(set(scenes) == {"kestrel-shortcuts", scenario} and len(scenes[scenario]["checks"]) == count, "actual native scenario/count")
            need(all(c["passed"] is True for x in scenes.values() for c in x["checks"])
                 and not any(f["severity"] == "failure" for x in scenes.values() for f in x["findings"]), "native raw successes")
            k = scenes["kestrel-shortcuts"]["checks"]
            need(len(k) == 1 and k[0]["actual"]["reserved_bindings"] == 62 and k[0]["actual"]["routing_cases"] == 21885504
                 and k[0]["actual"]["conflicts"] == [], "local Kestrel routing counts")
            argv = runs[n]["command"]
            need(meta(argv[0])["sha256"] == d["metadata"]["binary_sha256"]
                 and meta(argv[argv.index("--kestrel-source") + 1])["sha256"] == k[0]["actual"]["live_source_sha256"]
                 == k[0]["actual"]["source_sha256"], "invoked App and actual local Kestrel source")
            scene = scenes[scenario]
            native.append({"name": n, "checks": count, "kestrel": 1,
                           "warnings": [f["message"].split(": [{")[0].split(": [[")[0] for f in scene["findings"] if f["severity"] == "warning"],
                           "skips": scene["skipped"]})
        commands = obj("media-final/commands.json")
        need([c["name"] for c in commands] == ["dump", "validate", "render", "verify", "copy", "portable-validate",
                                              "portable-dump", "checkpoint", "portable-checkpoint"]
             and all(c["exit_code"] == 0 for c in commands), "nine real media commands")
        for c in commands:
            need(meta(c["command"][0])["sha256"] == final["binaries"]["deadpan-cli"]["sha256"], "actual CLI identity")
            read("media-final/" + c["name"] + ".stdout")
            read("media-final/" + c["name"] + ".stderr")
        document = obj("media-final/dump.stdout")
        need(document == obj("media-final/portable-dump.stdout"), "exact portable document")
        valid = obj("media-final/validate.stdout")
        need(valid == obj("media-final/portable-validate.stdout") and valid["valid"] is True
             and valid["revision_id"] == document["revision_id"], "real Store validation")
        groups = [(i, n) for i, n in document["nodes"].items() if n["kind"]["type"] == "sequence" and n.get("framing")]
        need(len(groups) == 1, "one authored group framing")
        owner, group = groups[0]
        need(owner != document["root"] and document["nodes"][document["root"]]["kind"]["children"] == [owner]
             and len(group["kind"]["children"]) == 2 and "clock" not in group["framing"], "ordinary selected Sequence owner/default clock")
        f = group["framing"]["value"]
        need(f["target"] == "target-1" and f["scale"] == f["fallback"]["scale"] == {"numerator": "11", "denominator": "10"}
             and f["scale_transition"] == {"factor": {"numerator": "2", "denominator": "1"}, "curve": "linear"}
             and all(not n.get("framing") for i, n in document["nodes"].items() if i != owner), "single group Follow, descendants preserved")
        need(document["targets"]["target-1"]["region"] == {"center": [580000, 500000]}
             and document["targets"]["target-1"]["corrections"] == [{"at": 80080, "region": {"center": [680000, 500000]}}], "per-provider Original target correction")
        v = obj("media-final/verification.json")
        need(v == obj("media-final/verify.stdout") and v["passed"] is True and v["failures"] == []
             and v["range"] == [0, 80] and v["frame_rate"] == [30000, 1001]
             and v["revision_id"] == document["revision_id"], "actual complete movie verification")
        need(len(v["pictures"]) == 80 and all(p["passed"] is True and p["flags"] == [] for p in v["pictures"])
             and [p["output_frame"] for p in v["pictures"]] == list(range(80)), "all picture result rows")
        observations = collections.Counter(p[k]["status"] for p in v["pictures"] for k in ("previous_index", "next_index") if p[k] is not None)
        need(observations == {"observable": 152, "unobservable": 6}
             and v["summary"]["index_observable_comparisons"] == 152 and v["summary"]["index_unobservable_comparisons"] == 6, "observability actual counts")
        a = v["audio"]
        need(len(a) == 1 and a[0]["passed"] is True and a[0]["flags"] == [] and a[0]["window"] == a[0]["project_samples"] == [0, 128128]
             and a[0]["uncovered_samples"] == 0 and a[0]["offset_status"] == "verified_zero" and a[0]["measured_offset_samples"] == 0
             and all(x["status"] == "verified_zero" and x["peak_lag"] == 0 for x in a[0]["segments"]), "full zero-offset audio window")
        movie = meta(S / "media-final/sequence-follow-creep.mp4")
        need(movie == {"bytes": 117671, "sha256": "f58bac4dd4e4dec609af135aa0958578ebe3bcb618cd0e089c1d2f086dc45611"}, "actual movie identity")
        m = v["movie"]
        need(m["bytes"] == movie["bytes"] and m["video_frames"] == m["expected_video_frames"] == 80 and m["picture_anomalies"] == []
             and m["audio"]["gaps"] == [] and m["audio"]["presented_end"] == m["expected_audio_samples"] == 128128
             and m["audio"]["first_pts"] == -2048 and m["audio"]["physical_samples"] == 131072
             and m["audio_edits"] == {"edits": 1, "empty_edits": 0, "media_time": 2048, "presented_media_ticks": 128128}
             and m["video_edits"] == {"edits": 1, "empty_edits": 0, "media_time": 0, "presented_media_ticks": 80080}, "video/AAC edit clocks")
        mr = obj("media-final/result.json")
        need(mr == json.loads(read("media-final.stdout")) and mr["movie_sha256"] == movie["sha256"]
             and mr["movie_bytes"] == movie["bytes"] and mr["expected_frames"] == 80
             and mr["expected_audio_samples"] == 128128 and mr["commands"] == 9 and mr["portable_document_equal"] is True, "raw media summary counterpart")
        pubs = [d for line in read("media-final/render.stdout").splitlines() for d in dictionaries(json.loads(line)) if "movie_sha256" in d]
        need(pubs and all(d == pubs[0] for d in pubs) and pubs[0]["movie_sha256"] == movie["sha256"]
             and pubs[0]["movie_bytes"] == movie["bytes"], "actual render publication receipt")
        pub = pubs[0]
        need(meta(S / "media-final" / Path(pub["report"]).name) == {"bytes": pub["report_bytes"], "sha256": pub["report_sha256"]}, "render report bytes")
        need(obj("media-final/copy.stdout")["portable_copy"]["verified_history"] == {"replayed": 0, "revisions": 26, "verified_by_receipt": 26}, "portable full history receipt")
        history, db_pins = [], {}
        for root in (Path(commands[0]["command"][3]), S / "media-final/portable.deadpan"):
            db = root / "project.sqlite"
            wal = root / "project.sqlite-wal"
            need(not wal.exists() or wal.stat().st_size == 0, "empty WAL before immutable inspection")
            before_db = meta(db)
            with sqlite3.connect(db.as_uri() + "?mode=ro&immutable=1", uri=True) as con:
                need(con.execute("PRAGMA quick_check").fetchall() == [("ok",)], "immutable SQLite integrity")
                tables = {t: sorted(con.execute('SELECT * FROM "' + t + '"').fetchall(), key=repr)
                          for t in ("revisions", "revision_patches", "history", "history_receipt", "redo", "state", "transaction_steps")}
            need(meta(db) == before_db, "SQLite file unchanged")
            history.append(tables)
            db_pins[str(db)] = before_db
        need(history[0] == history[1] and len(history[0]["revisions"]) == 26, "all retained history tables equal")
        reviews, visual = obj("review-notes.json"), obj("visual-review.json")
        need(reviews["base"] == out["base"], "retained scope review base")
        for p, pin in visual["images"].items():
            read(p)
            need(meta(S / p) == pin, "Root inspected image identity")
        diagnostics = {}
        for p in sorted(S.iterdir()):
            if p.suffix in (".stdout", ".stderr") and not p.name.startswith("audit-sequence-evidence"):
                lines = [x for x in read(p).decode(errors="replace").splitlines()
                         if x.startswith(("error:", "error[", "warning:", "warning[", "test result: FAILED"))]
                if lines:
                    diagnostics[p.name] = lines
        need(read("format-first.stdout") == read("format-first.stderr") == b"", "clean successful format streams")
        need("Finished" in read("clippy-final.stderr").decode(), "strict completion log")
        report_tests = {n: {k: val for k, val in d.items() if k != "passed_names" or d["passed"] <= 13} for n, d in results.items()}
        out.update(status="no_findings", findings=[], final_source_count=14, final_sources=final["sources"],
                   final_binaries=final["binaries"], final_identities_sha256=meta(S / "final-identities.json")["sha256"],
                   actual_candidate_equivalence=eq, standard_scope_paths=14, focused_worker_scopes=worker_observations,
                   plan_scope=plan_scope, tests=report_tests, native_reports=native,
                   actual_media_commands=9, movie=movie, picture_rows=80, picture_neighbor_observability=dict(observations),
                   audio_window=[0, 128128], audio_offset=0, actual_movie_metadata=m,
                   portable_authored_document_exact_equal=True, portable_revisions=26, portable_history_rows=len(history[0]["history"]),
                   all_selected_history_tables_equal=True, immutable_database_pins=db_pins, initial_cli_fixture_failure=failure,
                   unit_executable=unit["terminal"], root_visual_review=visual,
                   source_review_notes_sha256=meta(S / "review-notes.json")["sha256"],
                   retained_historical_diagnostics=diagnostics)
    except Exception as exc:
        traceback.print_exc()
        out["status"] = "findings"
        out["findings"].append({"type": type(exc).__name__, "message": str(exc)})
    finally:
        out["current_scoped_sources_unchanged"] = before == {p: meta(R / p) for p in before}
        if not out["current_scoped_sources_unchanged"]:
            out["status"] = "findings"
            out["findings"].append({"message": "Scoped source changed during audit"})
        out["input_file_pins"] = PINS
        out["preserved_first_audit_attempt"] = {
            "path": "audit-history/first-plan-test-basis-mismatch",
            "cause": "Overbroad current-basis assertion rejected an unrelated cfg(test) Core file update; focused Plan ownership is unchanged and final broad current suite passes both exact Plan cases.",
            "product_failure": False,
            "preservation_manifest": meta(S / "audit-history/first-plan-test-basis-mismatch/preservation.json"),
        }
        out["verifier_sha256"] = meta(Path(__file__))["sha256"]
        out["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        with (S / "audit-sequence-evidence.json").open("x") as f:
            json.dump(out, f, indent=2, sort_keys=True)
            f.write("\n")
        print(json.dumps({k: out.get(k) for k in ("status", "findings", "final_source_count", "picture_rows",
                                                "portable_revisions", "portable_history_rows", "current_scoped_sources_unchanged")}, sort_keys=True))
    return 0 if out["status"] == "no_findings" else 1


if __name__ == "__main__":
    sys.exit(main())


