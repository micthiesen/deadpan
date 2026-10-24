#!/usr/bin/env python3
"""Independent offline reader. Writes only its own scratch receipt; never extracts TARs."""
import collections
import gzip
import hashlib
import io
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import stat
import struct
import sys
import tarfile
import traceback

ROOT = Path("/tmp/deadpan-follow-creep-2026-10-24")
REPO = Path("/Users/michael/Code/deadpan")
PRIMARY = REPO / "tools/media-qualification/evidence/2026-10-24-follow-creep"
EXPECTED_INVENTORY = "7d51eb530df1abb5577de6972f14f3147c6b91567aa917cc4941cfb147e0fcf2"
MAX_BYTES = 268435456
OUT = ROOT / "independent-verification.json"
result = {
    "schema_version": 1,
    "scope": "Offline independent byte, archive, source, retained binary and raw evidence verification",
    "status": "running",
    "findings": [],
    "limitations": [
        "No collector, compiler, test binary, app, decoder, exporter or Git command was executed.",
        "Retained reports prove their recorded results and identities, not independent DSP, listening, tracking accuracy, physical keyboard/display behavior, latency targets or requirement completion.",
        "The current complete Rust map is a collection-time witness. Eleven runtime source pins do not establish that every earlier build compiled the entire current map.",
        "Final fmt logs are empty. Their bytes are verified, but empty logs alone do not independently establish a process exit code.",
        "Mutable build output paths are not substituted for the retained executed-copy identities.",
    ],
}


def require(ok, message):
    if not ok:
        raise AssertionError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def identity(path):
    require(stat.S_ISREG(path.lstat().st_mode), f"not a regular file: {path}")
    h = hashlib.sha256()
    n = 0
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            n += len(block)
            h.update(block)
    return {"bytes": n, "sha256": h.hexdigest()}


def safe_rel(value):
    p = PurePosixPath(value)
    require(isinstance(value, str) and value and not p.is_absolute()
            and p.as_posix() == value and all(x not in ("", ".", "..") for x in p.parts),
            f"unsafe relative path: {value!r}")
    return p


def tree(root):
    require(stat.S_ISDIR(root.lstat().st_mode), f"not a directory: {root}")
    out = {}
    for current, dirs, files in os.walk(root, followlinks=False):
        for name in dirs:
            require(stat.S_ISDIR((Path(current) / name).lstat().st_mode), "linked/special directory")
        for name in files:
            p = Path(current) / name
            out[p.relative_to(root).as_posix()] = identity(p)
    return dict(sorted(out.items()))


def bounded_gunzip(data):
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as f:
        out = f.read(MAX_BYTES + 1)
        require(len(out) <= MAX_BYTES and not f.read(1), "oversized gzip")
        return out


def walk_dicts(obj):
    if isinstance(obj, dict):
        yield obj
        for v in obj.values():
            yield from walk_dicts(v)
    elif isinstance(obj, list):
        for v in obj:
            yield from walk_dicts(v)


before = tree(PRIMARY)
try:
    require(before["files.json"]["sha256"] == EXPECTED_INVENTORY, "inventory pin")
    inventory = json.loads((PRIMARY / "files.json").read_bytes())
    require(set(before) == set(inventory) | {"files.json"}, "complete inventory path set")
    require(len(inventory) == 185, "payload count")
    require(sum(m["bytes"] for m in inventory.values()) == 7389224, "payload total")
    for path, meta in inventory.items():
        safe_rel(path)
        require(before[path] == meta, f"inventory mismatch: {path}")
    ledger = json.loads((PRIMARY / "encoding-ledger.json").read_bytes())["records"]
    decoded = {}
    rows = {}
    counts = collections.Counter()
    original_regular_bytes = 0
    for row in ledger:
        p = row["path"]
        require(p not in rows and p in inventory, f"duplicate/uninventoried ledger: {p}")
        rows[p] = row
        data = (PRIMARY / p).read_bytes()
        require(len(data) == row["stored_bytes"] and digest(data) == row["stored_sha256"], f"stored: {p}")
        require(row["encoding"] in ("identity", "gzip", "tar+gzip"), "unknown encoding")
        raw = data if row["encoding"] == "identity" else bounded_gunzip(data)
        require(len(raw) == row["decoded_bytes"] and digest(raw) == row["decoded_sha256"], f"decoded: {p}")
        counts[row["encoding"]] += 1
        decoded[p] = raw
        if row["encoding"] != "tar+gzip":
            require(identity(Path(row["source"])) == {"bytes": len(raw), "sha256": digest(raw)}, f"original source: {p}")
            original_regular_bytes += len(raw)
    require(len(rows) == 180, "encoding row count")
    generated = {"collection.json", "current-rust-source-map.json", "retained-binary-identities.json",
                 "encoding-ledger.json", "archive-ledger.json"}
    require(set(inventory) == set(rows) | generated, "ledger coverage")

    def raw_source(name):
        hits = [p for p, r in rows.items() if Path(r["source"]).name == name]
        require(len(hits) == 1, f"ambiguous/missing source {name}")
        return decoded[hits[0]]

    def source_json(name):
        return json.loads(raw_source(name))

    archives = json.loads((PRIMARY / "archive-ledger.json").read_bytes())["archives"]
    require(len(archives) == 5, "archive count")
    archived = {}
    archive_counts = []
    total_members = total_authored = total_omitted = 0
    for a in archives:
        expected = {m["path"]: {"bytes": m["bytes"], "sha256": m["sha256"]} for m in a["members"]}
        require(len(expected) == len(a["members"]), "duplicate member ledger")
        member_bytes = {}
        with tarfile.open(fileobj=io.BytesIO(decoded[a["path"]]), mode="r:") as tf:
            for member in tf:
                safe_rel(member.name)
                require(member.isfile() and not member.issparse() and member.size <= MAX_BYTES, "nonregular/oversized TAR member")
                require(member.name not in member_bytes and member.name in expected, "duplicate/unexpected TAR member")
                require(not member.name.endswith(("-wal", "-shm")), "SQLite metadata in authored TAR")
                raw = tf.extractfile(member).read(MAX_BYTES + 1)
                require(len(raw) == member.size and {"bytes": len(raw), "sha256": digest(raw)} == expected[member.name],
                        f"archive member mismatch: {a['path']}:{member.name}")
                member_bytes[member.name] = raw
        require(set(member_bytes) == set(expected), "missing TAR members")
        omitted = a["omitted_sidecars"]
        require(not set(expected) & set(omitted), "omission overlaps members")
        witness = dict(expected)
        for name, meta in omitted.items():
            safe_rel(name)
            require(name.endswith(("-wal", "-shm")) and name[:-4] in expected, "unpaired omitted sidecar")
            base = name[:-4]
            require(base.endswith("project.sqlite") and base + "-wal" in omitted and base + "-shm" in omitted,
                    "incomplete SQLite sidecar pair")
            require(omitted[base + "-wal"]["bytes"] == 0 and omitted[base + "-shm"]["bytes"] == 32768,
                    "nonempty WAL/unexpected SHM")
            witness[name] = {"bytes": meta["bytes"], "sha256": meta["sha256"]}
        live = tree(Path(a["root"]))
        require(live == dict(sorted(witness.items())), f"source-tree witness: {a['path']}")
        require(len(live) == a["source_files"] and sum(x["bytes"] for x in live.values()) == a["source_bytes"], "source tree totals")
        archived[a["path"]] = member_bytes
        total_members += len(expected)
        total_authored += sum(x["bytes"] for x in expected.values())
        total_omitted += len(omitted)
        archive_counts.append({"path": a["path"], "members": len(expected), "authored_bytes": sum(x["bytes"] for x in expected.values()),
                               "omitted_sidecars": len(omitted), "source_files": len(live), "source_bytes": a["source_bytes"]})
    require(total_members == 82 and total_omitted == 14, "archive aggregate counts")
    collection = json.loads((PRIMARY / "collection.json").read_bytes())
    require(original_regular_bytes + sum(a["source_bytes"] for a in archives)
            == collection["raw_retained_bytes_including_omitted_sidecars"], "raw retained byte total")
    plan_raw = (PRIMARY / "packaging/collect-plan.json").read_bytes()
    plan = json.loads(plan_raw)
    require(digest(plan_raw) == collection["plan_sha256"] == "8dba03a4fa8d4515a0cc8232cc725cd7b9d1e51b05ef3ea86b328fc4b84c7608", "plan pin")
    require(digest(decoded["packaging/collect.py.txt"]) == collection["script_sha256"]
            == "2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13", "collector pin")
    require(plan["raw_byte_limit"] == MAX_BYTES, "bounded collection plan")
    rust_map = json.loads((PRIMARY / "current-rust-source-map.json").read_bytes())
    current = {}
    for directory, dirs, files in os.walk(REPO, followlinks=False):
        dirs[:] = sorted(d for d in dirs if d not in {".git", ".claude"}
                         and not (d == "target" and (Path(directory) / "Cargo.toml").is_file()))
        for name in files:
            if name.endswith(".rs"):
                p = Path(directory) / name
                current[p.relative_to(REPO).as_posix()] = identity(p)["sha256"]
    require(len(current) == 2062 and current == rust_map, "complete current Rust map")
    source_files = source_json("source-files.json")
    require(len(source_files) == len(set(source_files)) == 30, "snapshot path count")
    for path in source_files:
        hits = [p for p, r in rows.items() if r["source"] == str(REPO / path)]
        require(len(hits) == 1 and hits[0].startswith("source/"), f"source snapshot {path}")
        require(digest(decoded[hits[0]]) == rust_map[path], f"snapshot to map {path}")
    freeze_raw = raw_source("runtime-source-freeze.json")
    freeze = json.loads(freeze_raw)
    require(len(freeze) == 11 and digest(freeze_raw) == collection["runtime_freeze_sha256"]
            == plan["runtime_source_freeze"]["sha256"], "runtime freeze")
    require(freeze == source_json("media-final-source-before.json") == source_json("media-final-source-after.json"),
            "media source before/after")
    require(all(current[p] == h for p, h in freeze.items()), "runtime current source pins")
    require(len(plan["build_inputs"]) == 6, "build input count")
    for path, h in plan["build_inputs"].items():
        require(identity(REPO / path)["sha256"] == h, f"current build input {path}")
        hits = [p for p, r in rows.items() if r["source"] == str(REPO / path)]
        require(len(hits) == 1 and hits[0].startswith("inputs/") and digest(decoded[hits[0]]) == h,
                f"retained build input {path}")
    binaries = json.loads((PRIMARY / "retained-binary-identities.json").read_bytes())
    require(len(binaries) == 16 and binaries == collection["binary_hashes_checked_at_collection"], "retained binary map")
    for path, meta in binaries.items():
        require(identity(Path(path)) == meta, f"retained binary changed: {path}")
    for name in ("final-native-binaries.json", "media-final-invocation.json", "media-first-invocation.json"):
        inv = source_json(name)
        parts = inv if name == "final-native-binaries.json" else dict(inv["binaries"], preview_export=inv["test"])
        for base, item in parts.items():
            copies = [Path(p) for p, m in binaries.items() if Path(p).name == base and m["sha256"] == item["sha256"]]
            require(copies, f"missing retained execution counterpart {name}:{base}")
            if "executed" in item or "retained" in item:
                p = item.get("executed", item.get("retained"))
                require(p in binaries and binaries[p]["sha256"] == item["sha256"], "executed-copy binding")
    final_app = source_json("final-native-binaries.json")["deadpan-app"]["sha256"]
    require(final_app == "2eb7d88f3b56ade6d98e6a347ea7ee1f2fa7d1988a7e46607d66cef3ca26b5b5", "final native app")
    for name in ("media-first-terminal.json", "media-final-terminal.json", "native-release-second-terminal.json"):
        terminal = source_json(name)
        require(terminal["exit_code"] == 0 and terminal["binaries_unchanged"] is True, f"terminal {name}")
    require(source_json("media-final-terminal.json")["runtime_sources_unchanged"] is True, "final media source guard")
    require(source_json("media-final-invocation.json")["runtime_source_freeze_sha256"] == digest(freeze_raw)
            == source_json("native-release-second-invocation.json")["runtime_source_freeze_sha256"], "invocation source freeze")
    native = []
    for lane in ("native-first", "native-debug-second", "native-release-second"):
        report = json.loads(decoded[f"native/{lane}/report.json.gz"])
        require(report["metadata"]["physical_display_measured"] is False
                and report["metadata"]["gpu_readback_in_timing_run"] is False, "native measurement boundary")
        scenarios = {s["name"]: s for s in report["scenarios"]}
        require(set(scenarios) == {"kestrel-shortcuts", "zoom"}, "native scenario set")
        k = scenarios["kestrel-shortcuts"]["checks"]
        require(len(k) == 1 and k[0]["passed"] is True, "Kestrel check")
        require(k[0]["actual"]["reserved_bindings"] == 62 and k[0]["actual"]["routing_cases"] == 21885504
                and k[0]["actual"]["conflicts"] == []
                and k[0]["actual"]["live_source_sha256"] == k[0]["actual"]["source_sha256"], "Kestrel raw counters")
        z = scenarios["zoom"]
        failures = [f["message"] for f in z["findings"] if f["severity"] == "failure"]
        n = {"lane": lane, "failed": report["failed"], "zoom_checks": len(z["checks"]),
             "failed_checks": sum(c["passed"] is not True for c in z["checks"]),
             "failure_findings": [x[:230] for x in failures],
             "warning_findings": sum(f["severity"] == "warning" for f in z["findings"]),
             "binary_sha256": report["metadata"]["binary_sha256"]}
        if lane == "native-release-second":
            require(report["failed"] is False and len(z["checks"]) == 213 and not failures
                    and all(c["passed"] is True for c in z["checks"]), "final native checks")
            require(report["metadata"]["build_profile"] == "release" and report["metadata"]["binary_sha256"] == final_app,
                    "final native metadata")
        elif lane == "native-first":
            require(report["failed"] is True and n["failed_checks"] == 1 and any("later endpoint" in x for x in failures),
                    "first native failure retained")
        else:
            require(report["failed"] is True and any("Timed out waiting for Committed revision changed" in x for x in failures),
                    "debug native failure retained")
        native.append(n)
    # Parse the harness summaries and individual test result lines, not a derived count table.
    summary_re = re.compile(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out")
    test_expectations = {
        "framing-boundaries-final.stdout": (23, 0, 0),
        "core-plan-first.stdout": (2020, 0, 0),
        "app-cli-unit-first.stdout": (1826, 0, 17),
        "core-follow-first.stdout": (26, 0, 0),
        "structural-second.stdout": (18, 0, 0),
        "framing-cli-all-second.stdout": (12, 0, 0),
        "media-final.stdout": (1, 0, 0),
        "media-first.stdout": (1, 0, 0),
    }
    tests = {}
    for name, expected in test_expectations.items():
        text = raw_source(name).decode()
        summaries = [m.groups() for m in summary_re.finditer(text)]
        require(summaries and all(s[0] == "ok" for s in summaries), f"raw success summaries {name}")
        totals = tuple(sum(int(s[i]) for s in summaries) for i in (1, 2, 3))
        require(totals == expected, f"raw count {name}: {totals}")
        passed_lines = re.findall(r"^test (.+) \.\.\. ok$", text, re.M)
        ignored_lines = re.findall(r"^test (.+) \.\.\. ignored(?:, (.*))?$", text, re.M)
        require(len(passed_lines) == totals[0] and len(ignored_lines) == totals[2], f"raw named test count {name}")
        tests[name] = {"summaries": len(summaries), "passed": totals[0], "failed": totals[1], "ignored": totals[2],
                       "filtered": [int(s[5]) for s in summaries]}
        if ignored_lines:
            tests[name]["ignored_tests"] = [{"name": x, "reason": y} for x, y in ignored_lines]
        if name.startswith("media-"):
            require(passed_lines == [source_json(name.replace(".stdout", "-invocation.json"))["test_name"]], "actual media test name")
    require(tests["media-final.stdout"]["filtered"] == [66], "final real-media filtered count")
    historical = {}
    for row in ledger:
        if row["path"].startswith("scratch/") and Path(row["source"]).name.endswith(".stdout"):
            text = decoded[row["path"]].decode(errors="replace")
            failed = [m.groups() for m in summary_re.finditer(text) if m.group(1) == "FAILED"]
            if failed:
                historical[Path(row["source"]).name] = [
                    {"passed": int(s[1]), "failed": int(s[2]), "ignored": int(s[3]), "filtered": int(s[5])} for s in failed]
    require(historical, "historical failed tests absent")
    strict = raw_source("clippy-final.stderr").decode()
    require("Finished \x60dev\x60 profile" in strict and not re.search(r"^error(?:\[|:)", strict, re.M), "final Clippy completion log")
    require(not raw_source("clippy-final.stdout"), "unexpected final Clippy stdout")
    fmt = {n: len(raw_source(n)) for n in ("fmt-final.stdout", "fmt-final.stderr", "fmt-confirmed.stdout", "fmt-confirmed.stderr")}
    require(not any(fmt.values()), "final fmt diagnostic output")

    media_results = []
    for archive_path, label in (("archives/actual-media.tar.gz", "first"), ("archives/actual-media-final.tar.gz", "final")):
        files = archived[archive_path]
        j = lambda p: json.loads(files[p])
        report = j("follow-creep-verification.json")
        require(report == j("follow-creep-verification.stdout.json"), "verification raw stdout differs")
        require(report["passed"] is True and report["failures"] == [], "media verification pass")
        pictures = report["pictures"]
        require(len(pictures) == 120 and all(p["passed"] is True and p["flags"] == [] for p in pictures), "120 actual picture results")
        require([p["output_frame"] for p in pictures] == list(range(120)), "picture range")
        require(report["summary"]["pictures_checked"] == 120 and report["summary"]["pictures_failed"] == 0
                and report["summary"]["nonzero_offsets"] == 0, "picture summary consistency")
        require(len(report["audio"]) == 1, "audio window count")
        audio = report["audio"][0]
        require(audio["passed"] is True and audio["flags"] == [] and audio["project_samples"] == [0, 192192]
                and audio["window"] == [0, 192192] and audio["uncovered_samples"] == 0
                and audio["offset_status"] == "verified_zero" and audio["measured_offset_samples"] == 0, "full audio verification")
        require(all(s["status"] == "verified_zero" and s["peak_lag"] == 0 for s in audio["segments"]), "audio segment offsets")
        require(report["movie"]["expected_audio_samples"] == 192192 and report["movie"]["expected_video_frames"] == 120
                and report["movie"]["video_frames"] == 120 and report["movie"]["picture_anomalies"] == [], "actual movie counters")
        events = [json.loads(line) for line in files["follow-creep-render.stdout.jsonl"].splitlines()]
        receipts = [d for d in walk_dicts(events) if "movie_sha256" in d]
        require(receipts and all(d == receipts[0] for d in receipts), "render publication receipt")
        receipt = receipts[0]
        movie = files["exports/follow-creep.mp4"]
        published_report = files["exports/" + Path(receipt["report"]).name]
        require(len(movie) == receipt["movie_bytes"] == report["movie"]["bytes"]
                and digest(movie) == receipt["movie_sha256"], "actual MP4 identity")
        require(len(published_report) == receipt["report_bytes"] and digest(published_report) == receipt["report_sha256"], "published render report identity")
        pcm_before = j("fixture/follow-creep-before-pcm.json")
        pcm_after = j("fixture/follow-creep-after-pcm.json")
        require(pcm_before["canonical"] == pcm_after and pcm_before["samples"] == 192192
                and pcm_before["channels"] == 2 and pcm_before["independent_DSP_claim"] is False, "complete PCM bit equality")
        nonzero = {}
        for key in ("authored", "limited"):
            pairs = pcm_after[key]
            require(len(pairs) == 192192 and all(len(pair) == 2 for pair in pairs), "stereo PCM dimensions")
            values = [[struct.unpack(">f", struct.pack(">I", b))[0] for b in pair] for pair in pairs]
            require(all(math.isfinite(x) for pair in values for x in pair), "nonfinite PCM")
            require(any(abs(x) > 0.1 for pair in values for x in pair), "vacuous silent PCM")
            nonzero[key] = sum(any(abs(x) > 1e-6 for x in pair) for pair in values)
        gains = pcm_after["limiter_gain"]
        require(len(gains) == 192192 and all(math.isfinite(struct.unpack(">d", struct.pack(">Q", b))[0]) for b in gains),
                "finite full limiter gain bits")
        authored = j("fixture/follow-creep-authored-evidence.json")
        require(len(authored["poses"]) == 120 and all(p["actual"] == p["expected"] for p in authored["poses"]), "raw authored pose oracle equality")
        no_rev = lambda d: {k: v for k, v in d.items() if k != "revision_id"}
        require(no_rev(authored["before"]) == no_rev(authored["undone"])
                and no_rev(authored["after"]) == no_rev(authored["restored"]), "undo/redo canonical documents")
        require(authored["history"] == {"replayed": 6, "revisions": 7, "verified_by_receipt": 0}, "seven revision replay")
        portable = j("fixture/follow-creep-portable-copy.json")["portable_copy"]
        require(portable["verified_history"] == {"replayed": 0, "revisions": 7, "verified_by_receipt": 7}, "portable history")
        summary = j("follow-creep-summary.json")
        require(summary == source_json(f"media-{label}-result.json"), "raw media result counterpart")
        require(summary["passed"] is True and summary["failures"] == [] and summary["frames"] == 120
                and summary["canonical_samples_compared"] == 192192 and summary["canonical_channels"] == 2
                and summary["authored_limited_and_limiter_gain_bits_equal"] is True, "media summary consistency")
        require(all(summary[k] is False and authored[k] is False for k in ("independent_DSP_claim", "listening_claim", "tracking_accuracy_claim")),
                "unsupported media claim")
        media_results.append({"lane": label, "movie_sha256": digest(movie), "movie_bytes": len(movie),
                              "published_report_sha256": digest(published_report), "verification_sha256": digest(files["follow-creep-verification.json"]),
                              "pictures": 120, "stereo_samples": 192192, "offset": 0, "pcm_all_bits_equal": True,
                              "pairs_above_1e_minus_6": nonzero, "portable_revisions": 7})
    require(media_results[0]["movie_sha256"] != media_results[1]["movie_sha256"], "unexpected identical first/final movies")
    result.update(status="no_findings", findings=[], inventory_sha256=EXPECTED_INVENTORY,
                  primary_files=len(before), inventoried_payloads=len(inventory), inventoried_bytes=7389224,
                  encoding_records=len(rows), encoding_counts=dict(counts), archives=archive_counts,
                  archive_regular_members=total_members, archive_authored_bytes=total_authored, omitted_sqlite_sidecars=total_omitted,
                  raw_retained_bytes_including_omitted_sidecars=collection["raw_retained_bytes_including_omitted_sidecars"],
                  source_snapshots=30, current_rust_paths=2062, runtime_source_freeze_paths=11,
                  current_rust_map_sha256=before["current-rust-source-map.json"]["sha256"],
                  runtime_source_freeze_sha256=digest(freeze_raw), build_inputs=plan["build_inputs"],
                  retained_binary_count=len(binaries), final_native_app_sha256=final_app,
                  collector_sha256=collection["script_sha256"], plan_sha256=collection["plan_sha256"],
                  raw_tests=tests, retained_historical_failed_test_summaries=historical,
                  native_reports=native, media_exports=media_results,
                  strict_log={"completion_line": strict.splitlines()[-1], "diagnostic_error_lines": 0},
                  fmt_log_bytes=fmt)
except Exception as exc:
    result["status"] = "findings"
    result["findings"].append({"message": str(exc), "type": type(exc).__name__})
    traceback.print_exc()
finally:
    after = tree(PRIMARY)
    result["primary_tree_unchanged"] = before == after
    result["inventory_unchanged"] = before.get("files.json") == after.get("files.json")
    if before != after:
        result["status"] = "findings"
        result["findings"].append({"message": "Primary bytes/path set changed during verifier"})
    result["verifier_sha256"] = identity(Path(__file__))["sha256"]
    with OUT.open("x") as f:
        json.dump(result, f, indent=2, sort_keys=True)
        f.write("\n")
    print(json.dumps({k: result.get(k) for k in ("status", "findings", "primary_files", "inventoried_payloads",
                                               "inventoried_bytes", "inventory_sha256", "primary_tree_unchanged",
                                               "archive_regular_members", "omitted_sqlite_sidecars")}, sort_keys=True))
sys.exit(0 if result["status"] == "no_findings" else 1)
