#!/usr/bin/env python3
"""Bounded offline post-copy verification. Never extracts or executes payloads."""
import binascii
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
import struct
import sys
import tarfile
import traceback

S = Path("/tmp/deadpan-point-targets-2026-10-24")
R = Path("/Users/michael/Code/deadpan")
P = R / "tools/media-qualification/evidence/2026-10-24-point-targets"
INVENTORY = "1c1174e044fc2695361371ba532e5475cc09b0dc9bbdc8dd44bdcc951727da69"
AUDIT = "dde49811202b5af9674c7f5eb6307f0c7b89ef2a01e7de6c70fad72438125650"
LIMIT = 268435456
result = {"schema_version": 1, "status": "running", "findings": [],
          "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "limitations": [
              "Offline read-only verification; no collector, product, build, test, worker, model, decoder or Git command ran.",
              "Fifty-two final scoped source/input paths do not attest compilation of every repository source file. Older partial manifests and the Core/Plan scoped exception remain explicit.",
              "First and final native images have distinct provenance. PNG identity/integrity checks do not replace Root's image review or prove physical display/input behavior.",
              "Export references shared picture/limited-audio semantics, not independent DSP or listening. No inference, real-person detection/tracking accuracy, latency or requirement completion is inferred.",
              "Two pinned portable SQLite metadata sidecars are omitted under the retained collector's empty-WAL policy; zero authored/history/media omissions, not literal zero omissions.",
          ]}


def need(ok, message):
    if not ok:
        raise AssertionError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


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


def safe(name):
    p = PurePosixPath(name)
    need(name and not p.is_absolute() and str(p) == name
         and all(x not in ("", ".", "..") for x in p.parts), f"unsafe path: {name!r}")
    need(not name.endswith(".rs"), "live .rs payload instead of snapshot")


def tree(root):
    root = Path(root)
    need(stat.S_ISDIR(root.lstat().st_mode), "not directory")
    out = {}
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs:
            need(stat.S_ISDIR((Path(directory) / name).lstat().st_mode), "linked/special directory")
        for name in files:
            p = Path(directory) / name
            out[p.relative_to(root).as_posix()] = meta(p)
    return dict(sorted(out.items()))


def gunzip(data):
    need(data[:4] == b"\x1f\x8b\x08\x00" and data[4:8] == b"\0\0\0\0", "noncanonical gzip header")
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as f:
        raw = f.read(LIMIT + 1)
        need(len(raw) <= LIMIT and not f.read(1), "oversized gzip")
    return raw


def dictionaries(value):
    if isinstance(value, dict):
        yield value
        for v in value.values():
            yield from dictionaries(v)
    elif isinstance(value, list):
        for v in value:
            yield from dictionaries(v)


old_names = ["audit-point-evidence.py", "audit-point-evidence.json", "audit-point-evidence.stdout", "audit-point-evidence.stderr"]
old_before = {n: meta(S / n) for n in old_names}
before = tree(P)
source_before = {}
try:
    need(before["files.json"]["sha256"] == INVENTORY, "exact inventory pin")
    inventory = json.loads((P / "files.json").read_bytes())
    need(len(inventory) == 254 and set(before) == set(inventory) | {"files.json"}, "exact Primary path set")
    need(sum(m["bytes"] for m in inventory.values()) == 8104783, "stored payload byte total")
    for name, m in inventory.items():
        safe(name)
        need(before[name] == m, f"inventory hash/size {name}")
    ledger = json.loads((P / "encoding-ledger.json").read_bytes())
    need(isinstance(ledger, list) and len(ledger) == 251, "encoding ledger schema/count")
    rows, decoded, counts = {}, {}, collections.Counter()
    regular_bytes = 0
    for row in ledger:
        name = row["path"]
        safe(name)
        need(name not in rows and name in inventory, "duplicate/uninventoried encoding")
        rows[name] = row
        stored = (P / name).read_bytes()
        need(len(stored) == row["stored_bytes"] and sha(stored) == row["stored_sha256"], f"stored identity {name}")
        need(row["encoding"] in ("identity", "gzip", "tar+gzip"), "unknown encoding")
        raw = stored if row["encoding"] == "identity" else gunzip(stored)
        need(len(raw) == row["decoded_bytes"] and sha(raw) == row["decoded_sha256"], f"decoded identity {name}")
        decoded[name] = raw
        counts[row["encoding"]] += 1
        if row["encoding"] != "tar+gzip":
            need(meta(row["source"]) == {"bytes": len(raw), "sha256": sha(raw)}, f"original byte source {name}")
            need(raw[:4] not in {b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
                                 b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\x7fELF"}, "executable binary payload")
            regular_bytes += len(raw)
    need(set(inventory) == set(rows) | {"encoding-ledger.json", "archive-ledger.json", "collection.json"}, "complete encoding coverage")
    need(counts == {"identity": 230, "gzip": 18, "tar+gzip": 3}, "encoding totals")

    def scratch(name):
        found = [p for p, r in rows.items() if Path(r["source"]).resolve() == (S / name).resolve()]
        need(len(found) == 1, f"missing/ambiguous scratch file {name}")
        return decoded[found[0]]

    def obj(name):
        return json.loads(scratch(name))

    archive_ledger = json.loads((P / "archive-ledger.json").read_bytes())
    need(isinstance(archive_ledger, list) and len(archive_ledger) == 3, "archive ledger schema/count")
    archives, archive_summary = {}, []
    archive_source_bytes = authored_bytes = omitted_bytes = 0
    omitted_records = {}
    for a in archive_ledger:
        expected = {m["path"]: {"bytes": m["bytes"], "sha256": m["sha256"]} for m in a["members"]}
        need(len(expected) == len(a["members"]), "duplicate member ledger")
        members, order = {}, []
        with tarfile.open(fileobj=io.BytesIO(decoded[a["path"]]), mode="r:") as tf:
            for member in tf:
                safe(member.name)
                need(member.isfile() and not member.issparse() and 0 <= member.size <= LIMIT, "unsafe/oversized TAR member")
                need(member.name not in members and member.name in expected, "duplicate/unexpected member")
                need((member.uid, member.gid, member.uname, member.gname, member.mode, member.mtime)
                     == (0, 0, "", "", 0o644, 0), "nondeterministic TAR metadata")
                need(set(member.pax_headers) <= {"path"} and
                     ("path" not in member.pax_headers or member.pax_headers["path"] == member.name), "unsafe PAX metadata")
                raw = tf.extractfile(member).read(LIMIT + 1)
                need({"bytes": len(raw), "sha256": sha(raw)} == expected[member.name] and len(raw) == member.size,
                     f"actual member bytes {member.name}")
                members[member.name] = raw
                order.append(member.name)
        need(order == sorted(order) and set(members) == set(expected), "deterministic complete TAR order")
        omitted = a["omitted_sidecars"]
        witness = dict(expected)
        for name, m in omitted.items():
            safe(name)
            need(name not in expected and name.endswith(("-wal", "-shm")), "authored omission")
            base = name[:-4]
            need(base.endswith("project.sqlite") and base in expected
                 and base + "-wal" in omitted and base + "-shm" in omitted, "unpaired omitted metadata")
            need(omitted[base + "-wal"]["bytes"] == 0 and omitted[base + "-shm"]["bytes"] == 32768,
                 "nonempty WAL/unexpected SHM")
            witness[name] = {"bytes": m["bytes"], "sha256": m["sha256"]}
            omitted_records[a["path"] + ":" + name] = m
            omitted_bytes += m["bytes"]
        need(tree(a["root"]) == dict(sorted(witness.items())), "complete original archive tree including omissions")
        need(len(witness) == a["source_files"] and sum(m["bytes"] for m in witness.values()) == a["source_bytes"], "archive source totals")
        # Retained checkpoint metadata must also be paired with an empty WAL.
        retained_sidecars = []
        for name in members:
            if name.endswith(("-wal", "-shm")):
                base = name[:-4]
                need(base in members and base + "-wal" in members and base + "-shm" in members, "unpaired retained sidecars")
                need(len(members[base + "-wal"]) == 0 and len(members[base + "-shm"]) == 32768, "retained nonempty WAL")
                retained_sidecars.append(name)
        archives[a["path"]] = members
        nbytes = sum(x["bytes"] for x in expected.values())
        authored_bytes += nbytes
        archive_source_bytes += a["source_bytes"]
        archive_summary.append({"path": a["path"], "regular_members": len(members), "retained_member_bytes": nbytes,
                                "source_files": a["source_files"], "source_bytes": a["source_bytes"],
                                "omitted_sidecars": len(omitted), "retained_checkpoint_sidecars": retained_sidecars})
    need([x["regular_members"] for x in archive_summary] == [52, 30, 13]
         and len(omitted_records) == 2 and omitted_bytes == 32768, "archive aggregate counts")
    collection = json.loads((P / "collection.json").read_bytes())
    plan = json.loads(decoded["packaging/collect-plan.json"])
    need(regular_bytes + archive_source_bytes == collection["raw_payload_bytes"] == 86960816
         and collection["raw_payload_bytes"] <= plan["raw_byte_limit"] == LIMIT, "256 MiB budget including omitted metadata")
    need(sha(decoded["packaging/collect-plan.json"]) == collection["plan_sha256"]
         and sha(decoded["packaging/collect.py.txt"]) == collection["script_sha256"], "collector and plan binding")
    prior = obj("audit-point-evidence.json")
    need(sha(scratch("audit-point-evidence.json")) == AUDIT == old_before["audit-point-evidence.json"]["sha256"]
         and prior["status"] == "no_findings" and prior["findings"] == [], "frozen embedded independent raw audit")
    for path, m in prior["input_file_pins"].items():
        matches = [n for n, r in rows.items() if Path(r["source"]).resolve() == Path(path).resolve()]
        need(all({"bytes": len(decoded[n]), "sha256": sha(decoded[n])} == m for n in matches), "prior audited payload changed")
    final, first = obj("final-identities.json"), obj("first-identities.json")
    need(len(final["sources"]) == 52 and final["sources"] == prior["final_sources"], "52 source scope")
    source_before = {p: meta(R / p) for p in final["sources"]}
    need(source_before == final["sources"], "current final source bytes")
    for path, m in final["sources"].items():
        names = [n for n, r in rows.items() if r["source"] == str(R / path) and n.startswith("source/")]
        need(len(names) == 1 and {"bytes": len(decoded[names[0]]), "sha256": sha(decoded[names[0]])} == m, "final snapshot identity")
        need(meta(S / "source-final" / (path + ".txt")) == m, "scratch final snapshot")
    old_source = archives["archives/first-source.tar.gz"]
    need({p[:-4]: {"bytes": len(b), "sha256": sha(b)} for p, b in old_source.items()} == first["sources"], "first source map")
    for identity in (first, final):
        for name, m in identity["binaries"].items():
            need(meta(m["executed"]) == {"bytes": m["bytes"], "sha256": m["sha256"]}, "retained executable binding")
    for path, m in plan["inputs"].items():
        need(meta(R / path) == m, "current collected input")
    # Every selected screenshot is byte-identical to its source and a structurally valid PNG.
    pngs = {}
    for name, row in rows.items():
        if not name.startswith("screenshots/"):
            continue
        raw = decoded[name]
        need(raw[:8] == b"\x89PNG\r\n\x1a\n", "PNG signature")
        offset = 8
        dims = None
        ended = False
        while offset < len(raw):
            need(offset + 12 <= len(raw), "short PNG chunk")
            n = struct.unpack(">I", raw[offset:offset + 4])[0]
            kind = raw[offset + 4:offset + 8]
            need(n <= LIMIT and offset + n + 12 <= len(raw), "PNG chunk bounds")
            body = raw[offset + 8:offset + 8 + n]
            crc = struct.unpack(">I", raw[offset + 8 + n:offset + 12 + n])[0]
            need(binascii.crc32(kind + body) & 0xffffffff == crc, "PNG CRC")
            if kind == b"IHDR":
                dims = list(struct.unpack(">II", body[:8]))
                need(all(0 < d <= 8192 for d in dims), "bounded PNG dimensions")
            offset += n + 12
            if kind == b"IEND":
                need(n == 0 and offset == len(raw), "PNG trailing bytes")
                ended = True
        need(dims and ended, "complete PNG")
        pngs[name] = {"bytes": len(raw), "sha256": sha(raw), "dimensions": dims}
    need(len(pngs) == 4, "four selected native PNGs")
    native = []
    for name, scenario, count, ids in (
        ("native-points-first", "points", 33, first), ("native-points-final", "points", 34, final),
        ("native-targets-final", "targets", 41, final), ("native-faces-final", "faces", 66, final),
        ("native-zoom-final", "zoom", 213, final),
    ):
        candidates = [n for n, r in rows.items() if Path(r["source"]).resolve() == (S / name / "report.json").resolve()]
        need(len(candidates) == 1, "native report coverage")
        d = json.loads(decoded[candidates[0]])
        r = obj(name + ".json")
        need(d["failed"] is False and r["exit_code"] == 0 and r["sources_unchanged"] is True
             and r["binaries_unchanged"] is True, "native terminal/raw checks")
        need(d["metadata"]["binary_sha256"] == ids["binaries"]["deadpan-app"]["sha256"]
             and d["metadata"]["physical_display_measured"] is False, "native binary/limit")
        scenes = {x["name"]: x for x in d["scenarios"]}
        need(set(scenes) == {"kestrel-shortcuts", scenario} and len(scenes[scenario]["checks"]) == count, "exact native count")
        need(all(c["passed"] is True for x in scenes.values() for c in x["checks"])
             and not any(f["severity"] == "failure" for x in scenes.values() for f in x["findings"]), "native no failures")
        previous = next(x for x in prior["native_reports"] if x["name"] == name)
        warnings = [f["message"].split(": [{")[0].split(": [[")[0] for f in scenes[scenario]["findings"] if f["severity"] == "warning"]
        need(warnings == previous["warnings"] and scenes[scenario]["skipped"] == previous["skips"], "all native warnings/skips preserved")
        k = scenes["kestrel-shortcuts"]["checks"]
        need(len(k) == 1 and k[0]["actual"]["reserved_bindings"] == 62
             and k[0]["actual"]["routing_cases"] == 21885504 and k[0]["actual"]["conflicts"] == [], "Kestrel counts")
        native.append({"name": name, "checks": count, "kestrel": 1, "warnings": warnings, "skips": previous["skips"]})
    test_counts = {}
    for name, expected in (("core-plan", (2041, 0, 0)), ("app-units-final", (1304, 0, 11)),
                           ("cli-models-units", (719, 0, 7)), ("models-golden", (1, 0, 0))):
        raw = scratch(name + ".stdout").decode()
        summaries = re.findall(r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", raw, re.M)
        actual = tuple(sum(int(x[i]) for x in summaries) for i in range(3))
        need(actual == expected and obj(name + ".json")["exit_code"] == 0, "published raw test counts")
        test_counts[name] = list(actual)
    for name in ("clippy-final-second", "format-final-second", "release-source-check", "media-final"):
        r = obj(name + ".json")
        need(r["exit_code"] == 0 and r["source_before"] == r["source_after"] == {str(R / p): m for p, m in final["sources"].items()},
             "final terminal source basis")
        need(r["binary_before"] == r["binary_after"], "final binary guard")
        for path, m in r["streams"].items():
            need(meta(path) == m, "raw terminal stream hash")
    media = archives["archives/final-point-project-and-export.tar.gz"]
    get = lambda n: json.loads(media[n])
    commands = get("commands.json")
    need(len(commands) == 9 and all(c["exit_code"] == 0 for c in commands), "nine real media commands")
    original = get("dump.stdout")
    need(original == get("portable-dump.stdout") and get("validate.stdout") == get("portable-validate.stdout")
         and get("validate.stdout")["valid"] is True, "exact portable document and real Store validation")
    v = get("verification.json")
    need(v == get("verify.stdout") and v["passed"] is True and v["failures"] == []
         and v["frame_rate"] == [30000, 1001] and v["range"] == [0, 120], "actual full movie verification")
    need(len(v["pictures"]) == 120 and all(p["passed"] is True and p["flags"] == [] for p in v["pictures"])
         and [p["output_frame"] for p in v["pictures"]] == list(range(120)), "all actual picture rows")
    observable = collections.Counter(p[k]["status"] for p in v["pictures"] for k in ("previous_index", "next_index") if p[k] is not None)
    need(observable == {"observable": 202, "unobservable": 36}, "observability retained")
    audio = v["audio"]
    need(len(audio) == 1 and audio[0]["passed"] is True and audio[0]["flags"] == []
         and audio[0]["window"] == audio[0]["project_samples"] == [0, 192192]
         and audio[0]["uncovered_samples"] == 0 and audio[0]["offset_status"] == "verified_zero"
         and audio[0]["measured_offset_samples"] == 0
         and all(x["peak_lag"] == 0 and x["status"] == "verified_zero" for x in audio[0]["segments"]), "full zero-offset audio")
    need(v["movie"] == prior["actual_movie_metadata"] and v["movie"]["audio"]["gaps"] == [], "video/AAC edit metadata unchanged")
    movie = {"bytes": len(media["point-target.mp4"]), "sha256": sha(media["point-target.mp4"])}
    need(movie == prior["movie"] == {"bytes": 191679, "sha256": "8f179d12c912fecbd21e8c610457b0131054ec5ed4f99f95c1b907d0171dbdc0"}, "actual retained movie")
    pubs = [d for line in media["render.stdout"].splitlines() for d in dictionaries(json.loads(line)) if "movie_sha256" in d]
    need(pubs and all(d == pubs[0] for d in pubs) and pubs[0]["movie_sha256"] == movie["sha256"], "render/movie binding")
    need(sha(media[Path(pubs[0]["report"]).name]) == pubs[0]["report_sha256"], "render report bytes")
    need(get("copy.stdout")["portable_copy"]["verified_history"] == {"replayed": 0, "revisions": 10, "verified_by_receipt": 10},
         "portable full history receipt")
    original_archive = archives["archives/actual-native-point-project.tar.gz"]
    histories = []
    for root, members, key in (
        (Path(archive_ledger[2]["root"]), original_archive, "project.sqlite"),
        (Path(archive_ledger[1]["root"]), media, "portable.deadpan/project.sqlite"),
    ):
        db = root / key
        wal = db.with_name(db.name + "-wal")
        need(not wal.exists() or wal.stat().st_size == 0, "nonempty WAL before immutable read")
        m = meta(db)
        need(m == {"bytes": len(members[key]), "sha256": sha(members[key])}, "actual-original/portable SQLite retained")
        with sqlite3.connect(db.as_uri() + "?mode=ro&immutable=1", uri=True) as con:
            need(con.execute("PRAGMA quick_check").fetchall() == [("ok",)], "retained DB integrity")
            tables = {t: sorted(con.execute('SELECT * FROM "' + t + '"').fetchall(), key=repr)
                      for t in ("revisions", "revision_patches", "history", "history_receipt", "state", "redo", "transaction_steps")}
        need(meta(db) == m, "SQLite file unchanged")
        histories.append(tables)
    need(histories[0] == histories[1] and len(histories[0]["revisions"]) == 10
         and len(histories[0]["history"]) == 5, "all retained history tables equality/counts")
    result.update(status="no_findings", findings=[], inventory_sha256=INVENTORY,
                  primary_files=255, inventoried_payloads=254, inventoried_bytes=8104783,
                  encoding_records=251, encoding_counts=dict(counts), raw_payload_bytes_including_omitted_metadata=86960816,
                  archive_results=archive_summary, tar_regular_members=95, retained_tar_member_bytes=authored_bytes,
                  omitted_metadata=omitted_records, omitted_metadata_bytes=omitted_bytes, authored_omissions=0,
                  final_source_paths=52, selected_pngs=pngs, raw_native=native, raw_tests=test_counts,
                  actual_media_commands=9, movie=movie, picture_rows=120, audio_window=[0, 192192], audio_offset=0,
                  picture_neighbor_observability=dict(observable), portable_revisions=10, portable_history_rows=5,
                  all_selected_history_tables_equal=True, embedded_raw_audit_sha256=AUDIT,
                  collector_sha256=collection["script_sha256"], plan_sha256=collection["plan_sha256"])
except Exception as exc:
    traceback.print_exc()
    result["status"] = "findings"
    result["findings"].append({"type": type(exc).__name__, "message": str(exc)})
finally:
    after = tree(P)
    old_after = {n: meta(S / n) for n in old_names}
    source_after = {p: meta(R / p) for p in source_before}
    result["primary_tree_unchanged"] = before == after
    result["inventory_unchanged"] = before["files.json"] == after["files.json"]
    result["earlier_audit_files_unchanged"] = old_before == old_after
    result["current_scoped_sources_unchanged"] = source_before == source_after
    if before != after or old_before != old_after or source_before != source_after:
        result["status"] = "findings"
        result["findings"].append({"message": "Primary, old audit or scoped sources changed during verifier"})
    result["prior_audit_file_pins"] = old_before
    result["verifier_sha256"] = meta(Path(__file__))["sha256"]
    result["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (S / "package-verification.json").open("x") as f:
        json.dump(result, f, indent=2, sort_keys=True)
        f.write("\n")
    print(json.dumps({k: result.get(k) for k in ("status", "findings", "primary_files", "inventoried_payloads",
                                              "inventoried_bytes", "inventory_sha256", "tar_regular_members",
                                              "authored_omissions", "primary_tree_unchanged", "earlier_audit_files_unchanged")}, sort_keys=True))
sys.exit(0 if result["status"] == "no_findings" else 1)
