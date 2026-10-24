#!/usr/bin/env python3
"""Read retained Camera evidence only. Never launches product or build commands."""
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import stat
import sys
import traceback

S = Path("/tmp/deadpan-camera-target-controls-2026-10-24")
R = Path("/Users/michael/Code/deadpan")
pins = {}
audit = {
    "schema_version": 1, "status": "running", "findings": [],
    "scope": "Independent offline audit of Camera face-picker retained qualification evidence before packaging",
    "limitations": [
        "No build, product binary, native worker, media decoder/exporter, publisher or Git command was executed by this audit.",
        "The positive twelve-face observations are scripted against the counter fixture; actual Vision zero-face results are exercised. This establishes neither real-person detection nor tracking accuracy.",
        "Export verification uses the shared picture and limited-audio reference. It is not an independent picture/DSP oracle or a listening test.",
        "Native input is scripted. Physical keyboard/display, OS picker interaction, VoiceOver, IME and audio-device behavior are not qualified; no latency or requirement-completion claim is inferred.",
        "Ten final source paths and scoped runtime receipts are bound. This is not a claim about every repository source file or all historical builds sharing one source snapshot.",
        "The first faces report passed semantic checks while Root's image inspection found overlapping badges and a stale completion notice. Final evidence is kept distinct from that pre-polish build.",
    ],
}


def check(ok, message):
    if not ok:
        raise AssertionError(message)


def meta(path):
    path = Path(path)
    check(stat.S_ISREG(path.lstat().st_mode), f"not regular: {path}")
    h = hashlib.sha256()
    n = 0
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            n += len(block)
            h.update(block)
    return {"bytes": n, "sha256": h.hexdigest()}


def read(path):
    p = Path(path)
    if not p.is_absolute():
        p = S / p
    m = meta(p)
    check(m["bytes"] <= 268435456, f"oversized data file: {p}")
    pins[str(p)] = m
    return p.read_bytes()


def obj(path):
    return json.loads(read(path))


def local_sources(items):
    return {str(R / k): v for k, v in items.items()}


def receipt(name, expected_sources=None, exit_code=0, rehash_binaries=True):
    d = obj(name + ".json")
    check(d["exit_code"] == exit_code, f"exit {name}")
    check(d["cwd"] == str(R) and "finished_at" in d and d["seconds"] >= 0, f"terminal {name}")
    check(d["source_before"] == d["source_after"] and d["sources_unchanged"] is True, f"source guard {name}")
    check(d["binary_before"] == d["binary_after"] and d["binaries_unchanged"] is True, f"binary guard {name}")
    if expected_sources is not None:
        check(d["source_before"] == local_sources(expected_sources), f"wrong source basis {name}")
    if "runner_sha256" in d:
        check(d["runner_sha256"] == runner["sha256"], f"runner pin {name}")
    for path, m in d["streams"].items():
        read(path)
        check(meta(path) == m, f"raw stream hash {name}")
    if rehash_binaries:
        for path, m in d["binary_before"].items():
            check(meta(path) == m, f"retained execution bytes {name}: {path}")
    return d


def nested_dicts(value):
    if isinstance(value, dict):
        yield value
        for v in value.values():
            yield from nested_dicts(v)
    elif isinstance(value, list):
        for v in value:
            yield from nested_dicts(v)


sources_before = {}
try:
    paths = obj("source-files.json")
    first = obj("first-identities.json")
    final = obj("final-identities.json")
    runner = meta(S / "run.py")
    read("run.py")
    read("media.py")
    check(len(paths) == len(set(paths)) == len(final["sources"]) == 10, "final source set")
    check(set(paths) == set(final["sources"]) and len(first["sources"]) == 9, "first/final source sets")
    sources_before = {p: meta(R / p) for p in paths}
    check(sources_before == final["sources"], "current sources match final snapshot")
    for phase, identities in (("first", first), ("final", final)):
        for path, m in identities["sources"].items():
            snapshot = S / ("source-" + phase) / (path + ".txt")
            read(snapshot)
            check(meta(snapshot) == m, f"retained {phase} source snapshot: {path}")
        for name, d in identities["binaries"].items():
            check(meta(d["executed"]) == {"bytes": d["bytes"], "sha256": d["sha256"]},
                  f"retained {phase} binary: {name}")
    check(set(first["binaries"]) == set(final["binaries"]) and len(final["binaries"]) == 5, "five bound executables")
    final_app = final["binaries"]["deadpan-app"]["sha256"]
    check(final_app == "9aec8c29f87b9a97dece459c3bd8dc66d70ce712865eba75a108ac7cfae69e5d", "final App pin")
    check(first["binaries"]["deadpan-app"]["sha256"] == "7919f72ce229558e5dbe704d721480116224375758b61160872b5dfd957971b3", "first App pin")
    check(all(first["binaries"][n]["sha256"] == final["binaries"][n]["sha256"]
              for n in final["binaries"] if n != "deadpan-app"), "four non-App executable identities stable")
    receipts = {}
    for name in ("native-faces-final", "native-targets-second", "native-zoom-final", "units-final-third",
                 "faces-service-final-second", "release-build-final", "clippy-final-second",
                 "format-final-second", "media-final"):
        receipts[name] = receipt(name, final["sources"])
    receipts["native-faces-first"] = receipt("native-faces-first", first["sources"])
    receipts["camera-final"] = receipt("camera-final", first["sources"])
    receipts["commands-final"] = receipt("commands-final", first["sources"])
    # Earlier failed runs retain their actual failure exit and streams. No success is inferred.
    for name in ("units-final", "commands-reference"):
        receipts[name] = receipt(name, exit_code=101, rehash_binaries=False)
    check(receipts["units-final-third"]["command"][0] in receipts["units-final-third"]["binary_before"],
          "direct unit test executable pinned")
    unit_bin = receipts["units-final-third"]["binary_before"][receipts["units-final-third"]["command"][0]]
    check(unit_bin["sha256"] == "ae9bfbe3c4716beb89b77da4a0b1edaaf5726e0c18822b4f32ac2878d26215ee", "final direct unit executable")
    check(receipts["clippy-final-second"]["command"] ==
          ["cargo", "clippy", "--locked", "-p", "deadpan-app", "--all-targets", "--features", "ui-harness", "--", "-D", "warnings"],
          "strict command scope")
    check(receipts["format-final-second"]["command"] == ["cargo", "fmt", "--all", "--", "--check"], "format command")
    summary_pattern = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", re.M)
    tests = {}
    for name, expected in {
        "units-final-third": (1301, 0, 11), "camera-final": (35, 0, 0),
        "faces-service-final-second": (4, 0, 0), "commands-final": (1, 0, 0),
        "units-final": (1298, 1, 11), "commands-reference": (0, 1, 0),
    }.items():
        text = read(name + ".stdout").decode()
        results = [m.groups() for m in summary_pattern.finditer(text)]
        check(results, f"missing raw test summary {name}")
        totals = tuple(sum(int(m[i]) for m in results) for i in (1, 2, 3))
        check(totals == expected, f"raw test counts {name}: {totals}")
        passed = re.findall(r"^test (.+) \.\.\. ok$", text, re.M)
        ignored = re.findall(r"^test (.+) \.\.\. ignored(?:, (.*))?$", text, re.M)
        check(len(passed) == totals[0] and len(ignored) == totals[2], f"named test totals {name}")
        tests[name] = {"passed": totals[0], "failed": totals[1], "ignored": totals[2],
                       "filtered": [int(m[5]) for m in results],
                       "summary_statuses": [m[0] for m in results]}
        if ignored:
            tests[name]["ignored_tests"] = [{"name": n, "reason": r} for n, r in ignored]
        if name == "faces-service-final-second":
            check("project::tests::targets::faces::a_face_completion_does_not_replace_a_later_save_notice" in passed,
                  "notice ownership regression actually ran")
        if name == "units-final":
            check("commands_reference" in text and "FAILED" in text, "historical stale commands failure retained")
    check(sorted(tests["units-final-third"]["ignored_tests"], key=lambda x: x["name"]) ==
          sorted(tests["units-final"]["ignored_tests"], key=lambda x: x["name"]), "existing ignored test list")
    strict = read("clippy-final-second.stderr").decode()
    check("Finished \x60dev\x60 profile" in strict and not re.search(r"^error(?:\[|:)", strict, re.M), "strict successful log")
    check(read("format-final-second.stdout") == read("format-final-second.stderr") == b"", "format clean streams")
    native = []
    face_report = None
    for name, scenario, expected, basis in (
        ("native-faces-first", "faces", 66, first),
        ("native-faces-final", "faces", 66, final),
        ("native-targets-second", "targets", 41, final),
        ("native-zoom-final", "zoom", 213, final),
    ):
        d = obj(name + "/report.json")
        run = receipts[name]
        check(d["failed"] is False and d["metadata"]["build_profile"] == "release", f"native pass {name}")
        check(d["metadata"]["binary_sha256"] == basis["binaries"]["deadpan-app"]["sha256"], f"report App identity {name}")
        check(meta(run["command"][0])["sha256"] == d["metadata"]["binary_sha256"], "invoked and reported App")
        check(d["metadata"]["physical_display_measured"] is False and d["metadata"]["gpu_readback_in_timing_run"] is False,
              "native proof boundary")
        scenarios = {x["name"]: x for x in d["scenarios"]}
        check(set(scenarios) == {"kestrel-shortcuts", scenario}, "native exact scenario set")
        for x in scenarios.values():
            check(all(c["passed"] is True for c in x["checks"])
                  and not any(f["severity"] == "failure" for f in x["findings"]), "native raw failures")
        k = scenarios["kestrel-shortcuts"]["checks"]
        check(len(k) == 1 and k[0]["actual"]["conflicts"] == []
              and k[0]["actual"]["reserved_bindings"] == 62
              and k[0]["actual"]["routing_cases"] == 21885504
              and k[0]["actual"]["source_sha256"] == k[0]["actual"]["live_source_sha256"], "raw Kestrel audit")
        kestrel_path = run["command"][run["command"].index("--kestrel-source") + 1]
        check(meta(kestrel_path)["sha256"] == k[0]["actual"]["live_source_sha256"], "actual local Kestrel source")
        x = scenarios[scenario]
        check(len(x["checks"]) == expected, "native check count")
        native.append({"name": name, "scenario_checks": expected, "kestrel_checks": 1,
                       "warning_count": sum(f["severity"] == "warning" for f in x["findings"]),
                       "warning_summaries": [f["message"].split(": [{")[0].split(": [[")[0] for f in x["findings"] if f["severity"] == "warning"],
                       "skipped": x["skipped"], "binary_sha256": d["metadata"]["binary_sha256"]})
        if name == "native-faces-final":
            face_report = x
    checks = {c["name"]: c for c in face_report["checks"]}
    zero = checks["The real detector finds no face in the fixture and :zoom target=face:2 refuses without an edit"]["actual"]
    check(zero["faces"] == "Some(Some(Found([])))" and zero["revision_changed"] is False and zero["targets"] == 0,
          "actual real detector zero result")
    proposals = checks["Background proposals preserve the existing digits and create no edit"]["actual"]
    check(proposals["faces_pending"] is False and proposals["faces_status"].startswith("12 face proposals.")
          and proposals["picker_pages"] == 2, "twelve proposals and paging")
    check(sum(p["face"] is not None for p in proposals["picker"]) == 12, "actual twelve proposal entries")
    # Check the saved state, raw CLI validation, actual file and full decoder verification.
    media = obj("media-final/commands.json")
    command_names = ["dump", "validate", "render", "verify", "copy", "portable-validate", "portable-dump", "checkpoint", "portable-checkpoint"]
    check([r["name"] for r in media] == command_names and all(r["exit_code"] == 0 for r in media), "nine actual CLI commands")
    cli_path = final["binaries"]["deadpan-cli"]["executed"]
    check(all(Path(r["command"][0]).resolve() == Path(cli_path).resolve() for r in media), "all media commands use bound CLI")
    for row in media:
        check(row["seconds"] >= 0, "media terminal")
        read("media-final/" + row["name"] + ".stdout")
        read("media-final/" + row["name"] + ".stderr")
    original = obj("media-final/dump.stdout")
    portable = obj("media-final/portable-dump.stdout")
    check(original == portable, "portable exact authored document")
    valid = obj("media-final/validate.stdout")
    portable_valid = obj("media-final/portable-validate.stdout")
    check(valid == portable_valid and valid["valid"] is True
          and valid["revision_id"] == original["revision_id"] and valid["duration_frames"] == 120, "actual Store validation")
    source_nodes = [n for n in original["nodes"].values() if n["kind"]["type"] == "source"]
    check(len(source_nodes) == 1 and len(original["targets"]) == 2, "actual saved target state")
    framing = source_nodes[0]["framing"]["value"]
    check(framing["type"] == "follow" and framing["target"] == "face-2"
          and framing["scale_transition"]["curve"] == "smoothstep"
          and original["targets"]["face-2"]["label"] == "Face 6 (face-2)", "accepted target retains Follow transition")
    verify = obj("media-final/verification.json")
    check(verify == obj("media-final/verify.stdout"), "raw verification stdout equals report")
    check(verify["passed"] is True and verify["failures"] == [] and verify["frame_rate"] == [30000, 1001]
          and verify["range"] == [0, 120] and verify["revision_id"] == original["revision_id"], "verified range/rate/head")
    check(len(verify["pictures"]) == 120 and all(p["passed"] is True and p["flags"] == [] for p in verify["pictures"])
          and [p["output_frame"] for p in verify["pictures"]] == list(range(120)), "every picture actual result")
    check(verify["summary"]["pictures_failed"] == 0 and verify["summary"]["nonzero_offsets"] == 0, "verification summary recomputed")
    audio = verify["audio"]
    check(len(audio) == 1 and audio[0]["passed"] is True and audio[0]["flags"] == []
          and audio[0]["window"] == audio[0]["project_samples"] == [0, 192192]
          and audio[0]["uncovered_samples"] == 0 and audio[0]["offset_status"] == "verified_zero"
          and audio[0]["measured_offset_samples"] == 0
          and all(p["status"] == "verified_zero" and p["peak_lag"] == 0 for p in audio[0]["segments"]), "full zero-offset audio")
    check(verify["movie"]["audio"]["gaps"] == [] and verify["movie"]["audio"]["presented_end"] == 192192
          and verify["movie"]["video_frames"] == verify["movie"]["expected_video_frames"] == 120
          and verify["movie"]["expected_audio_samples"] == 192192
          and verify["movie"]["picture_anomalies"] == [], "movie clocks/gaps")
    movie = S / "media-final/face-picker.mp4"
    movie_meta = meta(movie)
    check(movie_meta["sha256"] == "7c2036efceb5ae97179e8b2132944ec3b702dd178f7b42e9ac802f10fc87e798", "actual fresh movie")
    check(movie_meta["bytes"] == verify["movie"]["bytes"] == 128183, "movie bytes")
    summary = obj("media-final/result.json")
    check(summary == json.loads(read("media-final.stdout")) and summary["movie_sha256"] == movie_meta["sha256"]
          and summary["portable_document_equal"] is True and summary["commands"] == 9, "raw media summary counterparts")
    events = [json.loads(line) for line in read("media-final/render.stdout").splitlines()]
    publish = [x for x in nested_dicts(events) if "movie_sha256" in x]
    check(publish and all(x == publish[0] for x in publish), "render receipt")
    pub = publish[0]
    check(pub["movie_sha256"] == movie_meta["sha256"] and pub["movie_bytes"] == movie_meta["bytes"], "render/movie hash binding")
    report_path = S / "media-final" / Path(pub["report"]).name
    check(meta(report_path) == {"bytes": pub["report_bytes"], "sha256": pub["report_sha256"]}, "actual render report binding")
    read(report_path)
    copied = obj("media-final/copy.stdout")["portable_copy"]
    check(copied["verified_history"] == {"replayed": 0, "revisions": 13, "verified_by_receipt": 13}, "portable receipt history")
    # Immutable, read-only SQLite inspection confirms retained history rows match the copy.
    project_path = Path(media[0]["command"][3])
    portable_path = S / "media-final/portable.deadpan"
    history = []
    for directory in (project_path, portable_path):
        db = directory / "project.sqlite"
        wal = directory / "project.sqlite-wal"
        check(not wal.exists() or wal.stat().st_size == 0, "nonempty WAL before immutable history read")
        before_db = meta(db)
        with sqlite3.connect(db.as_uri() + "?mode=ro&immutable=1", uri=True) as con:
            check(con.execute("PRAGMA quick_check").fetchall() == [("ok",)], "read-only SQLite integrity")
            tables = {}
            for table in ("history", "revisions", "revision_patches", "history_receipt", "state", "redo", "transaction_steps"):
                tables[table] = sorted(con.execute('SELECT * FROM "' + table + '"').fetchall(), key=repr)
        check(meta(db) == before_db, "SQLite inspection changed file")
        history.append(tables)
    check(history[0] == history[1] and len(history[0]["revisions"]) == 13 and len(history[0]["history"]) == 7,
          "portable actual history equality")
    correction = obj("post-run-metadata-correction.json")
    old_metadata = read("targets-before-metadata.rs.txt")
    new_metadata = read("targets-after-metadata.rs.txt")
    old_string = b"Face/person proposals are not implemented; the ,z target punch-in and :zoom target= are covered by the zoom scenario."
    new_string = b"Detected face proposals and their Camera picker are covered by the faces scenario; target punch-in and :zoom target= are covered by zoom."
    check(old_metadata.count(old_string) == 1 and old_metadata.replace(old_string, new_string) == new_metadata,
          "exact single-string post-run amendment")
    check(hashlib.sha256(old_metadata).hexdigest() == correction["before_sha256"]
          and hashlib.sha256(new_metadata).hexdigest() == correction["after_sha256"]
          and meta(R / correction["path"])["sha256"] == correction["after_sha256"], "post-run snapshot and current binding")
    check(correction["path"] not in paths and correction["exactly_one_string_replacement"] is True
          and correction["new_wording_executed_in_retained_replays"] is False, "metadata amendment qualification boundary")
    audit.update(
        status="no_findings", findings=[],
        first_identities_sha256=meta(S / "first-identities.json")["sha256"],
        final_identities_sha256=meta(S / "final-identities.json")["sha256"],
        first_source_count=9, final_sources=final["sources"], final_source_count=10,
        first_app_sha256=first["binaries"]["deadpan-app"]["sha256"],
        final_app_sha256=final_app, bound_executables_per_build=5, stable_non_app_executables=4,
        final_direct_unit_binary=unit_bin,
        tests=tests, native_reports=native,
        raw_cli_commands=9, movie=movie_meta, pictures_verified=120,
        audio_samples_verified=192192, audio_offset=0, frame_rate=[30000, 1001],
        portable_document_exact_equal=True, portable_revision_rows=13, portable_history_rows=7,
        actual_original_and_portable_history_rows_equal=True,
        render_report_sha256=pub["report_sha256"],
        verification_sha256=meta(S / "media-final/verification.json")["sha256"],
        post_run_metadata_only_amendment=correction,
        observations=[
            "The targets scenario has stale legacy skipped wording saying face/person proposals are not implemented. Separate faces checks do exercise the new picker; this wording is not treated as evidence that those checks were skipped.",
            "An eleventh Rust path received exactly one diagnostic-string replacement after runtime qualification. Both versions and the exact transformation are bound; its new wording was not executed in these retained runs.",
            "First and final face semantic reports both pass. Root's recorded visual-polish issue and notice-ownership fix remain material provenance; semantic pass alone does not prove badge legibility.",
            "Camera-focused 35-test run uses the pre-polish nine-source basis. The final four service tests and complete 1,301-unit run use the final ten-source basis.",
            "The initial complete unit run failed one stale commands-reference assertion. The commands writer intentionally exited 101, then the commands test and later full run passed; all raw failed streams remain bound.",
        ],
    )
except Exception as exc:
    traceback.print_exc()
    audit["status"] = "findings"
    audit["findings"].append({"type": type(exc).__name__, "message": str(exc)})
finally:
    sources_after = {p: meta(R / p) for p in sources_before}
    audit["reviewed_current_sources_unchanged"] = sources_before == sources_after
    if sources_before != sources_after:
        audit["status"] = "findings"
        audit["findings"].append({"message": "Scoped repository source changed during audit"})
    audit["input_file_pins"] = pins
    audit["verifier_sha256"] = meta(Path(__file__))["sha256"]
    with (S / "evidence-audit.json").open("x") as f:
        json.dump(audit, f, indent=2, sort_keys=True)
        f.write("\n")
    lines = [
        "# Camera face-picker evidence audit", "",
        "Status: " + audit["status"] + ".", "",
        "Read-only offline verification. No product or build command was executed.", "",
    ]
    if audit["findings"]:
        lines.extend("- " + x["message"] for x in audit["findings"])
    else:
        lines.extend([
            "Final ten source snapshots and scoped receipts match current source. First and final retained App/helper bytes are verified separately.",
            "One additional Rust path has a proved single diagnostic-string amendment after the runs. Its corrected wording is not claimed to have been executed.",
            "Raw results: 1,301 App unit tests passed, 11 existing ignored; 35 pre-polish Camera tests; four final face-service tests. Final native faces/targets/Zoom have 66/41/213 checks plus one Kestrel check each, with their warnings and skips preserved.",
            "Nine real CLI commands passed. The actual 128,183-byte movie verifies all 120 pictures and [0,192192) audio samples at 30000/1001 with zero offsets/gaps/failures. Portable authored document and 13 revision rows match, with seven actual history rows.",
            "The twelve-face positive data is scripted. The real counter-fixture Vision zero-face result, earlier stale-doc test failure and pre-polish visual issue remain explicit. Shared picture/DSP references do not establish independent DSP, listening, tracking accuracy, physical input or latency qualification.",
        ])
    lines.extend(["", "See evidence-audit.json for exact pins, ignored names, warnings, skips and proof limits.", ""])
    with (S / "evidence-audit.md").open("x") as f:
        f.write("\n".join(lines))
    print(json.dumps({k: audit.get(k) for k in ("status", "findings", "final_source_count", "pictures_verified",
                                               "audio_samples_verified", "portable_revision_rows",
                                               "reviewed_current_sources_unchanged")}, sort_keys=True))
sys.exit(0 if audit["status"] == "no_findings" else 1)
