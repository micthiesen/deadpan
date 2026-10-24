#!/usr/bin/env python3
"""Independent bounded post-publication reader. Never extracts or executes payloads."""
import collections
import datetime
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import sqlite3
import stat
import sys
import tarfile
import traceback

S = Path("/tmp/deadpan-camera-target-controls-2026-10-24")
R = Path("/Users/michael/Code/deadpan")
P = R / "tools/media-qualification/evidence/2026-10-24-camera-face-picker"
EXPECTED = "eff0d0c2542c329a1cedd82bbcff1100be9fa5aceb115823284d7bb3ce7ab98e"
LIMIT = 268435456
record = {
    "schema_version": 1, "status": "running", "findings": [],
    "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "scope": "Independent read-only verification of immutable Camera face-picker Primary",
    "limitations": [
        "No package payload, collector, product executable, build, decoder, exporter or Git operation was executed.",
        "The final ten qualified source paths and one proved, unexecuted diagnostic-string amendment are checked. No complete repository compilation attestation is inferred.",
        "Twelve positive face proposals are scripted; actual Vision zero-face counter-fixture cases do not establish real-person detector or tracking accuracy.",
        "The recorded export checks use shared picture and limited-audio references, not an independent DSP oracle or listening test.",
        "Raw replay warnings and skips remain, including the original stale targets skip. No physical-input/display, latency or requirement-completion claim is made.",
    ],
}


def require(ok, message):
    if not ok:
        raise AssertionError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def meta(path):
    path = Path(path)
    require(stat.S_ISREG(path.lstat().st_mode), f"not regular: {path}")
    h = hashlib.sha256()
    size = 0
    with path.open("rb") as f:
        for b in iter(lambda: f.read(1024 * 1024), b""):
            h.update(b)
            size += len(b)
    return {"bytes": size, "sha256": h.hexdigest()}


def safe(path):
    p = PurePosixPath(path)
    require(path and not p.is_absolute() and str(p) == path
            and all(x not in (".", "..", "") for x in p.parts), f"unsafe path {path!r}")
    require(not path.endswith(".rs"), f"live Rust source retained: {path}")
    return p


def tree(root):
    require(stat.S_ISDIR(root.lstat().st_mode), f"not directory: {root}")
    items = {}
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs:
            require(stat.S_ISDIR((Path(directory) / name).lstat().st_mode), "linked/special directory")
        for name in files:
            path = Path(directory) / name
            items[path.relative_to(root).as_posix()] = meta(path)
    return dict(sorted(items.items()))


def gunzip(data):
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as f:
        raw = f.read(LIMIT + 1)
        require(len(raw) <= LIMIT and not f.read(1), "oversized gzip")
    return raw


def dictionaries(obj):
    if isinstance(obj, dict):
        yield obj
        for value in obj.values():
            yield from dictionaries(value)
    elif isinstance(obj, list):
        for value in obj:
            yield from dictionaries(value)


old_names = ["audit-evidence.py", "evidence-audit.json", "evidence-audit.md", "evidence-audit.stdout", "evidence-audit.stderr"]
old_before = {n: meta(S / n) for n in old_names}
before = tree(P)
try:
    require(before["files.json"]["sha256"] == EXPECTED, "inventory pin")
    inventory = json.loads((P / "files.json").read_bytes())
    require(len(inventory) == 138 and set(before) == set(inventory) | {"files.json"}, "complete Primary path set")
    require(sum(x["bytes"] for x in inventory.values()) == 6157873, "payload byte count")
    for path, m in inventory.items():
        safe(path)
        require(before[path] == m, f"payload inventory mismatch {path}")
    ledger = json.loads((P / "encoding-ledger.json").read_bytes())
    require(isinstance(ledger, list) and len(ledger) == 135, "actual encoding ledger schema/count")
    rows, decoded, counts = {}, {}, collections.Counter()
    total_regular = 0
    for row in ledger:
        path = row["path"]
        safe(path)
        require(path not in rows and path in inventory, f"duplicate/uninventoried row {path}")
        rows[path] = row
        stored = (P / path).read_bytes()
        require(len(stored) == row["stored_bytes"] and sha(stored) == row["stored_sha256"], f"stored encoding {path}")
        require(row["encoding"] in ("identity", "gzip", "tar+gzip"), "unknown encoding")
        raw = stored if row["encoding"] == "identity" else gunzip(stored)
        require(len(raw) == row["decoded_bytes"] and sha(raw) == row["decoded_sha256"], f"decoded encoding {path}")
        counts[row["encoding"]] += 1
        decoded[path] = raw
        if row["encoding"] != "tar+gzip":
            require(meta(row["source"]) == {"bytes": len(raw), "sha256": sha(raw)}, f"original decoded source {path}")
            require(raw[:4] not in {b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
                                    b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\x7fELF"}, "executable binary payload")
            total_regular += len(raw)
    require(set(inventory) == set(rows) | {"collection.json", "encoding-ledger.json", "archive-ledger.json"}, "ledger complete coverage")
    require(counts == {"identity": 120, "gzip": 13, "tar+gzip": 2}, "encoding counts")

    def from_scratch(name):
        found = [p for p, r in rows.items() if Path(r["source"]).resolve() == (S / name).resolve()]
        require(len(found) == 1, f"missing/ambiguous scratch counterpart {name}")
        return decoded[found[0]]

    def obj(name):
        return json.loads(from_scratch(name))

    archives = json.loads((P / "archive-ledger.json").read_bytes())
    require(isinstance(archives, list) and len(archives) == 2, "actual archive schema/count")
    tar_files, archive_results = {}, []
    authored_bytes = 0
    for a in archives:
        expected = {m["path"]: {"bytes": m["bytes"], "sha256": m["sha256"]} for m in a["members"]}
        require(len(expected) == len(a["members"]), "duplicate TAR ledger")
        actual = {}
        with tarfile.open(fileobj=io.BytesIO(decoded[a["path"]]), mode="r:") as tf:
            for member in tf:
                safe(member.name)
                require(member.isfile() and not member.issparse() and 0 <= member.size <= LIMIT,
                        f"nonregular/oversized TAR member {member.name}")
                require(member.name not in actual and member.name in expected, "duplicate/unexpected TAR member")
                raw = tf.extractfile(member).read(LIMIT + 1)
                require(len(raw) == member.size and {"bytes": len(raw), "sha256": sha(raw)} == expected[member.name],
                        f"TAR member identity {member.name}")
                actual[member.name] = raw
        require(set(actual) == set(expected), "TAR complete path set")
        omitted = a["omitted_sidecars"]
        require(omitted == {}, "unexpected omissions in this package")
        require(tree(Path(a["root"])) == dict(sorted(expected.items())), "archive source-tree witness")
        require(a["source_files"] == len(actual) and a["source_bytes"] == sum(x["bytes"] for x in expected.values()), "archive source totals")
        # Check the actual retained checkpoint metadata rather than assuming every sidecar was omitted.
        sidecars = {}
        for name, raw in actual.items():
            if name.endswith(("-wal", "-shm")):
                base = name[:-4]
                require(base in actual and base + "-wal" in actual and base + "-shm" in actual, "unpaired retained SQLite sidecar")
                require(len(actual[base + "-wal"]) == 0 and len(actual[base + "-shm"]) == 32768,
                        "nonempty retained WAL or unexpected SHM")
                sidecars[name] = {"bytes": len(raw), "sha256": sha(raw)}
        tar_files[a["path"]] = actual
        authored_bytes += a["source_bytes"]
        archive_results.append({"path": a["path"], "members": len(actual), "source_bytes": a["source_bytes"],
                                "omitted_sidecars": 0, "retained_sidecars": sidecars})
    require([x["members"] for x in archive_results] == [9, 30], "archive membership totals")
    collection = json.loads((P / "collection.json").read_bytes())
    plan = json.loads(decoded["packaging/collect-plan.json"])
    require(sha(decoded["packaging/collect-plan.json"]) == collection["plan_sha256"], "plan identity")
    require(sha(decoded["packaging/collect.py.txt"]) == collection["script_sha256"]
            == "320e79dbf00568d68d7657e3c31d968402772533071529e7c9c494f9aacc6f0e", "collector identity")
    require(total_regular + authored_bytes == collection["raw_payload_bytes"] == 100788674
            and plan["raw_byte_limit"] == LIMIT, "raw bounded aggregate")
    require(len(plan["inputs"]) == 7, "six build inputs plus one fixture")
    for path, m in plan["inputs"].items():
        require(meta(R / path) == m, f"current input {path}")
        found = [p for p, r in rows.items() if r["source"] == str(R / path)]
        require(len(found) == 1 and found[0].startswith("inputs/"), f"retained input {path}")
    prior = obj("evidence-audit.json")
    require(sha(from_scratch("evidence-audit.json")) == "9a4de1c9677277ada71b626daa0c16733305039e6861898f8c60d768beff1b89"
            and prior["status"] == "no_findings" and prior["findings"] == [], "original independent audit frozen")
    require(old_before["evidence-audit.json"]["sha256"] == sha(from_scratch("evidence-audit.json")), "scratch original audit")
    for source, m in prior["input_file_pins"].items():
        retained = [p for p, r in rows.items() if Path(r["source"]).resolve() == Path(source).resolve()]
        if retained:
            require(all({"bytes": len(decoded[p]), "sha256": sha(decoded[p])} == m for p in retained),
                    f"prior-audited input changed in package {source}")
    final = obj("final-identities.json")
    first = obj("first-identities.json")
    paths = obj("source-files.json")
    require(len(paths) == len(set(paths)) == 10 and final["sources"] == prior["final_sources"], "final source snapshot basis")
    for path in paths:
        require(meta(R / path) == final["sources"][path], f"current qualified source {path}")
        candidates = [p for p, r in rows.items() if r["source"] == str(R / path)]
        require(len(candidates) == 1 and candidates[0].startswith("source/"), "published qualified snapshot")
    for phase in (first, final):
        for name, m in phase["binaries"].items():
            require(meta(m["executed"]) == {"bytes": m["bytes"], "sha256": m["sha256"]}, f"retained binary {name}")
    first_archive = tar_files["archives/first-source.tar.gz"]
    require({p[:-4]: {"bytes": len(raw), "sha256": sha(raw)} for p, raw in first_archive.items()} == first["sources"],
            "first source TAR identity map")
    correction = obj("post-run-metadata-correction.json")
    old = from_scratch("targets-before-metadata.rs.txt")
    new = from_scratch("targets-after-metadata.rs.txt")
    old_text = b"Face/person proposals are not implemented; the ,z target punch-in and :zoom target= are covered by the zoom scenario."
    new_text = b"Detected face proposals and their Camera picker are covered by the faces scenario; target punch-in and :zoom target= are covered by zoom."
    require(old.count(old_text) == 1 and old.replace(old_text, new_text) == new, "exact post-run metadata replacement")
    require(sha(old) == correction["before_sha256"] and sha(new) == correction["after_sha256"]
            and correction["new_wording_executed_in_retained_replays"] is False
            and meta(R / correction["path"])["sha256"] == sha(new), "unexecuted post-run source binding")
    require(plan["post_run_sources"] == {correction["path"]: meta(R / correction["path"])}, "distinct eleventh source plan")
    # Re-parse raw final results from published bytes as well as comparing their prior audit pins.
    native_counts = {}
    for name, scenario, expected, phase in (
        ("native-faces-first", "faces", 66, first), ("native-faces-final", "faces", 66, final),
        ("native-targets-second", "targets", 41, final), ("native-zoom-final", "zoom", 213, final),
    ):
        report = json.loads(decoded["native/" + name + ".json.gz"])
        run = obj(name + ".json")
        require(report["failed"] is False and run["exit_code"] == 0
                and run["source_before"] == run["source_after"]
                and run["binary_before"] == run["binary_after"], "raw native success and guards")
        require(report["metadata"]["binary_sha256"] == phase["binaries"]["deadpan-app"]["sha256"]
                and report["metadata"]["physical_display_measured"] is False, "native binary and physical limit")
        scenes = {x["name"]: x for x in report["scenarios"]}
        require(set(scenes) == {"kestrel-shortcuts", scenario} and len(scenes[scenario]["checks"]) == expected, "native scenario count")
        require(all(c["passed"] is True for x in scenes.values() for c in x["checks"])
                and not any(f["severity"] == "failure" for x in scenes.values() for f in x["findings"]), "native raw check results")
        k = scenes["kestrel-shortcuts"]["checks"]
        require(len(k) == 1 and k[0]["actual"]["reserved_bindings"] == 62
                and k[0]["actual"]["routing_cases"] == 21885504 and k[0]["actual"]["conflicts"] == [], "native Kestrel counters")
        native_counts[name] = {"checks": expected, "kestrel": 1,
                              "warnings": sum(f["severity"] == "warning" for f in scenes[scenario]["findings"]),
                              "skips": scenes[scenario]["skipped"]}
    test_counts = {}
    for name, expected in (("units-final-third", (1301, 0, 11)), ("camera-final", (35, 0, 0)),
                           ("faces-service-final-second", (4, 0, 0)), ("commands-final", (1, 0, 0)),
                           ("units-final", (1298, 1, 11)), ("commands-reference", (0, 1, 0))):
        text = from_scratch(name + ".stdout").decode()
        matches = re.findall(r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", text, re.M)
        totals = tuple(sum(int(m[i]) for m in matches) for i in range(3))
        require(totals == expected, f"published raw test count {name}")
        test_counts[name] = list(totals)
    for name in ("units-final-third", "faces-service-final-second", "clippy-final-second", "format-final-second", "media-final"):
        r = obj(name + ".json")
        require(r["exit_code"] == 0 and r["sources_unchanged"] is True and r["binaries_unchanged"] is True,
                f"published successful terminal {name}")
        for source, m in r["streams"].items():
            require(meta(source) == m, f"terminal raw streams {name}")
    media = tar_files["archives/final-camera-project-and-export.tar.gz"]
    get_media = lambda n: json.loads(media[n])
    commands = get_media("commands.json")
    require(len(commands) == 9 and all(x["exit_code"] == 0 for x in commands), "nine real CLI commands")
    original = get_media("dump.stdout")
    require(original == get_media("portable-dump.stdout"), "portable exact document")
    require(get_media("validate.stdout") == get_media("portable-validate.stdout")
            and get_media("validate.stdout")["valid"] is True, "actual Store validate output")
    verification = get_media("verification.json")
    require(verification == get_media("verify.stdout") and verification["passed"] is True and verification["failures"] == [],
            "decoder verification report")
    require(verification["frame_rate"] == [30000, 1001] and verification["range"] == [0, 120]
            and len(verification["pictures"]) == 120
            and all(x["passed"] is True and x["flags"] == [] for x in verification["pictures"]), "all 120 pictures")
    require([x["output_frame"] for x in verification["pictures"]] == list(range(120)), "complete picture range")
    audio = verification["audio"]
    require(len(audio) == 1 and audio[0]["passed"] is True and audio[0]["flags"] == []
            and audio[0]["window"] == audio[0]["project_samples"] == [0, 192192]
            and audio[0]["uncovered_samples"] == 0 and audio[0]["offset_status"] == "verified_zero"
            and audio[0]["measured_offset_samples"] == 0
            and all(x["peak_lag"] == 0 and x["status"] == "verified_zero" for x in audio[0]["segments"]), "full zero-offset audio")
    require(verification["movie"]["audio"]["gaps"] == [] and verification["movie"]["audio"]["presented_end"] == 192192,
            "audio clock and no gap")
    movie_sha = sha(media["face-picker.mp4"])
    require(movie_sha == prior["movie"]["sha256"] == "7c2036efceb5ae97179e8b2132944ec3b702dd178f7b42e9ac802f10fc87e798"
            and len(media["face-picker.mp4"]) == 128183, "published actual movie")
    receipts = [x for line in media["render.stdout"].splitlines() for x in dictionaries(json.loads(line)) if "movie_sha256" in x]
    require(receipts and all(x == receipts[0] for x in receipts), "render receipt consistency")
    pub = receipts[0]
    require(pub["movie_sha256"] == movie_sha and pub["movie_bytes"] == 128183
            and sha(media[Path(pub["report"]).name]) == pub["report_sha256"] == prior["render_report_sha256"], "movie and render report binding")
    portable_history = get_media("copy.stdout")["portable_copy"]["verified_history"]
    require(portable_history == {"replayed": 0, "revisions": 13, "verified_by_receipt": 13}, "copy history receipt")
    # Immutable read-only inspection; TAR bytes already equal this source DB byte-for-byte.
    db = S / "media-final/portable.deadpan/project.sqlite"
    require(meta(db) == {"bytes": len(media["portable.deadpan/project.sqlite"]),
                        "sha256": sha(media["portable.deadpan/project.sqlite"])}, "published SQLite database identity")
    with sqlite3.connect(db.as_uri() + "?mode=ro&immutable=1", uri=True) as con:
        require(con.execute("PRAGMA quick_check").fetchall() == [("ok",)], "published database integrity")
        require(con.execute("SELECT COUNT(*) FROM revisions").fetchone() == (13,)
                and con.execute("SELECT COUNT(*) FROM history").fetchone() == (7,), "published revision/history counts")
    record.update(
        status="no_findings", findings=[], inventory_sha256=EXPECTED,
        primary_files=139, payloads=138, payload_bytes=6157873,
        encoding_records=135, encodings=dict(counts), decoded_regular_plus_authored_bytes=100788674,
        archives=archive_results, tar_members=39, tar_source_bytes=authored_bytes, omitted_sidecars=0,
        retained_checkpoint_sidecars=2, live_rust_source_files=0,
        qualified_source_paths=10, post_run_unexecuted_metadata_paths=1, six_build_inputs_plus_fixture=7,
        source_binaries_and_previous_audit_equal=True, old_audit_sha256=old_before["evidence-audit.json"]["sha256"],
        native=native_counts, tests=test_counts, actual_media_commands=9,
        movie_sha256=movie_sha, movie_bytes=128183, pictures=120, audio_samples=192192, measured_offset=0,
        portable_revisions=13, portable_history_rows=7,
        collector_sha256=collection["script_sha256"], plan_sha256=collection["plan_sha256"],
    )
except Exception as exc:
    traceback.print_exc()
    record["status"] = "findings"
    record["findings"].append({"type": type(exc).__name__, "message": str(exc)})
finally:
    after = tree(P)
    old_after = {n: meta(S / n) for n in old_names}
    record["primary_tree_unchanged"] = before == after
    record["inventory_unchanged"] = before["files.json"] == after["files.json"]
    record["earlier_audit_files_unchanged"] = old_before == old_after
    if before != after or old_before != old_after:
        record["status"] = "findings"
        record["findings"].append({"message": "Primary or prior audit changed during verification"})
    record["prior_audit_file_pins"] = old_before
    record["verifier_sha256"] = meta(Path(__file__))["sha256"]
    record["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (S / "package-verification.json").open("x") as f:
        json.dump(record, f, indent=2, sort_keys=True)
        f.write("\n")
    lines = ["# Camera face-picker package verification", "", "Status: " + record["status"] + ".", ""]
    if record["findings"]:
        lines += ["- " + x["message"] for x in record["findings"]]
    else:
        lines += [
            "Verified all 138 inventoried payloads, 6,157,873 bytes, plus files.json. Primary and all earlier audit files were unchanged.",
            "Verified 135 encoding rows: 120 identity, 13 ordinary gzip and two TAR gzip. The TARs contain 39 regular members, no omitted files, and one retained checkpoint metadata pair with empty WAL and 32,768-byte SHM.",
            "The ten qualified source snapshots, unexecuted metadata-only eleventh path, six build inputs plus the fixture, retained binaries, raw reports and earlier independent audit match their byte sources. No live .rs file or executable binary was copied.",
            "Published raw results preserve the passing final checks and earlier failures/warnings. The actual movie, all 120 pictures, full zero-offset [0,192192) audio, exact portable authored document and 13-revision/seven-history-row database agree with the previous audit.",
            "No collector, product, build, export, decoder or Git command was executed. Scripted positive faces, shared picture/DSP references and native physical-input/latency limits remain explicit.",
        ]
    lines += ["", "Exact inventory, script, plan and prior-audit pins are in package-verification.json.", ""]
    with (S / "package-verification.md").open("x") as f:
        f.write("\n".join(lines))
    print(json.dumps({k: record.get(k) for k in ("status", "findings", "primary_files", "payloads", "payload_bytes",
                                              "inventory_sha256", "tar_members", "primary_tree_unchanged",
                                              "earlier_audit_files_unchanged")}, sort_keys=True))
sys.exit(0 if record["status"] == "no_findings" else 1)
