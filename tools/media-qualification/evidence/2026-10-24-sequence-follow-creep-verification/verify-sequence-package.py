#!/usr/bin/env python3
"""Independent bounded offline reader of the published Sequence qualification pack."""
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

S = Path("/tmp/deadpan-sequence-follow-creep-2026-10-24")
R = Path("/Users/michael/Code/deadpan")
P = R / "tools/media-qualification/evidence/2026-10-24-sequence-follow-creep"
INV = "dac42373c6119a3baf5cd98869cf91beb01b66d676ab9a2763bdf7885bc8e7a9"
AUDIT = "1287a23f69e7003a5a815d12f05d4311bd3677f69c88eca16a39249e2073e0a4"
LIMIT = 268435456
result = {"schema_version": 1, "status": "running", "findings": [],
          "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "limitations": [
              "No collector, payload script, product, Cargo, test, worker, decoder, model or Git command executed.",
              "Fourteen scoped sources and relevant historical subsets are checked, not compilation of every repository file.",
              "Root's image review is bound by bytes; PNG integrity is not independent visual or physical-display qualification.",
              "Shared picture/limited-audio references do not establish independent DSP, listening, encoded bit equality, tracking accuracy or latency/requirement completion.",
              "The initial CLI fixture failure and initial offline Plan-basis scope stop are preserved; final broad Core/Plan execution provides the current binding. Only one release candidate/Sequence workflow is claimed.",
          ]}


def need(ok, message):
    if not ok:
        raise AssertionError(message)


def sha(b):
    return hashlib.sha256(b).hexdigest()


def meta(path):
    p = Path(path)
    need(stat.S_ISREG(p.lstat().st_mode), f"not regular: {p}")
    h = hashlib.sha256()
    size = 0
    with p.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
            size += len(block)
    return {"bytes": size, "sha256": h.hexdigest()}


def safe(name):
    p = PurePosixPath(name)
    need(name and not p.is_absolute() and str(p) == name
         and all(x not in ("", ".", "..") for x in p.parts), "unsafe relative path")
    need(not name.endswith(".rs"), "live Rust source instead of text snapshot")


def tree(root):
    root = Path(root)
    need(stat.S_ISDIR(root.lstat().st_mode), "not a directory")
    out = {}
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs:
            need(stat.S_ISDIR((Path(directory) / name).lstat().st_mode), "linked/special directory")
        for name in files:
            p = Path(directory) / name
            out[p.relative_to(root).as_posix()] = meta(p)
    return dict(sorted(out.items()))


def gunzip(b):
    need(b[:4] == b"\x1f\x8b\x08\x00" and b[4:8] == b"\0\0\0\0", "canonical gzip header")
    with gzip.GzipFile(fileobj=io.BytesIO(b)) as f:
        raw = f.read(LIMIT + 1)
        need(len(raw) <= LIMIT and not f.read(1), "oversized gzip")
    return raw


def dicts(o):
    if isinstance(o, dict):
        yield o
        for v in o.values():
            yield from dicts(v)
    elif isinstance(o, list):
        for v in o:
            yield from dicts(v)


old_names = ["audit-sequence-evidence.py", "audit-sequence-evidence.json", "audit-sequence-evidence.stdout", "audit-sequence-evidence.stderr"]
old_before = {n: meta(S / n) for n in old_names}
history_before = tree(S / "audit-history")
before = tree(P)
source_before = {}
try:
    need(before["files.json"]["sha256"] == INV, "exact inventory pin")
    inv = json.loads((P / "files.json").read_bytes())
    need(len(inv) == 134 and set(before) == set(inv) | {"files.json"}, "strict inventory membership")
    need(sum(x["bytes"] for x in inv.values()) == 8457096, "inventory total bytes")
    for name, m in inv.items():
        safe(name)
        need(before[name] == m, f"payload byte/hash {name}")
    ledger = json.loads((P / "encoding-ledger.json").read_bytes())
    need(isinstance(ledger, list) and len(ledger) == 131, "ledger schema/count")
    rows, data, counts = {}, {}, collections.Counter()
    raw_regular = 0
    for row in ledger:
        name = row["path"]
        safe(name)
        need(name not in rows and name in inv, "unique inventoried ledger entry")
        b = (P / name).read_bytes()
        need(len(b) == row["stored_bytes"] and sha(b) == row["stored_sha256"], "stored ledger identity")
        need(row["encoding"] in ("identity", "gzip", "tar+gzip"), "known encoding")
        raw = b if row["encoding"] == "identity" else gunzip(b)
        need(len(raw) == row["decoded_bytes"] and sha(raw) == row["decoded_sha256"], "decoded ledger identity")
        rows[name], data[name] = row, raw
        counts[row["encoding"]] += 1
        if row["encoding"] != "tar+gzip":
            need(meta(row["source"]) == {"bytes": len(raw), "sha256": sha(raw)}, f"original decoded source {name}")
            need(raw[:4] not in {b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
                                 b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\x7fELF"}, "executable binary retained")
            raw_regular += len(raw)
    need(counts == {"identity": 107, "gzip": 19, "tar+gzip": 5}, "encoding aggregate")
    need(set(inv) == set(rows) | {"collection.json", "encoding-ledger.json", "archive-ledger.json"}, "complete ledger coverage")

    def scratch(name):
        hits = [n for n, r in rows.items() if Path(r["source"]).resolve() == (S / name).resolve()]
        need(len(hits) == 1, f"unique scratch counterpart {name}")
        return data[hits[0]]

    def obj(name):
        return json.loads(scratch(name))

    aa = json.loads((P / "archive-ledger.json").read_bytes())
    need(isinstance(aa, list) and len(aa) == 5, "archive ledger")
    archives, summaries = {}, []
    archive_bytes = 0
    for a in aa:
        expected = {m["path"]: {"bytes": m["bytes"], "sha256": m["sha256"]} for m in a["members"]}
        need(len(expected) == len(a["members"]) and a["omitted_sidecars"] == {}, "no duplicate members or omissions")
        members, order = {}, []
        with tarfile.open(fileobj=io.BytesIO(data[a["path"]]), mode="r:") as tf:
            for m in tf:
                safe(m.name)
                need(m.isfile() and not m.issparse() and 0 <= m.size <= LIMIT, "safe regular bounded member")
                need(m.name not in members and m.name in expected, "unique expected member")
                need((m.uid, m.gid, m.uname, m.gname, m.mode, m.mtime) == (0, 0, "", "", 0o644, 0), "deterministic TAR metadata")
                need(set(m.pax_headers) <= {"path"} and ("path" not in m.pax_headers or m.pax_headers["path"] == m.name), "safe PAX metadata")
                b = tf.extractfile(m).read(LIMIT + 1)
                need({"bytes": len(b), "sha256": sha(b)} == expected[m.name] and len(b) == m.size, "actual TAR member bytes")
                members[m.name] = b
                order.append(m.name)
        need(order == sorted(order) and set(members) == set(expected), "complete deterministic member order")
        need(tree(a["root"]) == dict(sorted(expected.items())), "entire original archive tree retained")
        n = sum(x["bytes"] for x in expected.values())
        need(n == a["source_bytes"] and len(members) == a["source_files"], "archive tree totals")
        retained_sidecars = []
        for name in members:
            if name.endswith(("-wal", "-shm")):
                base = name[:-4]
                need(base in members and base + "-wal" in members and base + "-shm" in members, "paired retained checkpoint metadata")
                need(len(members[base + "-wal"]) == 0 and len(members[base + "-shm"]) == 32768, "empty retained WAL and bounded SHM")
                retained_sidecars.append(name)
        archives[a["path"]] = members
        archive_bytes += n
        summaries.append({"path": a["path"], "members": len(members), "bytes": n, "omissions": 0, "retained_sidecars": retained_sidecars})
    need([x["members"] for x in summaries] == [14, 6, 5, 30, 13] and archive_bytes == 4626097, "archive aggregate counts/bytes")
    collection = json.loads((P / "collection.json").read_bytes())
    plan = json.loads(data["packaging/collect-plan.json"])
    need(raw_regular + archive_bytes == collection["raw_payload_bytes"] == 114876734
         and collection["raw_payload_bytes"] <= plan["raw_byte_limit"] == LIMIT, "cumulative raw budget")
    need(sha(data["packaging/collect-plan.json"]) == collection["plan_sha256"]
         == "248c91607f4fcb140201e8f854a168de65445b53c9e8ffb53bec374deabfbfe4", "frozen plan")
    need(sha(data["packaging/collect.py.txt"]) == collection["script_sha256"]
         == "320e79dbf00568d68d7657e3c31d968402772533071529e7c9c494f9aacc6f0e", "collector bytes")
    need(sha(data["packaging/bundle-helpers.py.txt"]) ==
         "2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13", "pinned helper bytes")
    prior = obj("audit-sequence-evidence.json")
    need(sha(scratch("audit-sequence-evidence.json")) == AUDIT == old_before["audit-sequence-evidence.json"]["sha256"]
         and prior["status"] == "no_findings" and prior["findings"] == [], "embedded frozen raw audit")
    for path, m in prior["input_file_pins"].items():
        retained = [n for n, r in rows.items() if Path(r["source"]).resolve() == Path(path).resolve()]
        need(all({"bytes": len(data[n]), "sha256": sha(data[n])} == m for n in retained), "previously audited bytes unchanged")
    final, first = obj("final-identities.json"), obj("first-identities.json")
    need(len(final["sources"]) == 14 and len(first["sources"]) == 14 and final["sources"] == prior["final_sources"], "actual first/final scopes")
    source_before = {p: meta(R / p) for p in final["sources"]}
    need(source_before == final["sources"], "current final14")
    for p, m in final["sources"].items():
        hits = [n for n, r in rows.items() if r["source"] == str(R / p) and n.startswith("source/")]
        need(len(hits) == 1 and {"bytes": len(data[hits[0]]), "sha256": sha(data[hits[0]])} == m
             and meta(S / "source-final" / (p + ".txt")) == m, "final snapshot bytes")
    first_members = archives["archives/first-source.tar.gz"]
    need({p[:-4]: {"bytes": len(b), "sha256": sha(b)} for p, b in first_members.items()} == first["sources"], "first14 snapshot map")
    for ids in (first, final):
        for name, m in ids["binaries"].items():
            need(meta(m["executed"]) == {"bytes": m["bytes"], "sha256": m["sha256"]}, "retained executed binary")
    for p, m in plan["inputs"].items():
        need(meta(R / p) == m, "current collected build/document/fixture input")
    hist = archives["archives/audit-history.tar.gz"]
    prefix = "first-plan-test-basis-mismatch/"
    preservation = json.loads(hist[prefix + "preservation.json"])
    need(len(hist) == 5, "entire first audit attempt retained")
    for n, m in preservation["files"].items():
        b = hist[prefix + n]
        need({"bytes": len(b), "sha256": sha(b)} == m, "preserved first-attempt identity")
    stopped = json.loads(hist[prefix + "audit-sequence-evidence.json"])
    need(stopped["status"] == "findings"
         and stopped["findings"][0]["message"] == "current Plan test and production basis", "original audit stop remains explicit")
    full = obj("core-plan-final.json")
    need(full["exit_code"] == 0 and full["source_before"] == full["source_after"] == {str(R / p): m for p, m in final["sources"].items()},
         "final14 broad suite current binding")
    full_names = re.findall(r"^test (.+) \.\.\. ok$", scratch("core-plan-final.stdout").decode(), re.M)
    for name in prior["plan_scope"]["final_current_broad_suite_passes_both_exact_plan_cases"]:
        need(full_names.count(name) == 1, "both historical Plan cases pass once in final current broad suite")
    pd = obj("plan-focused-1.json")
    need(pd["source_before"] == pd["source_after"] and pd["owned_sources_unchanged"] is True
         and pd["basis_before"] == pd["basis_after"] and pd["production_basis_unchanged"] is True, "historical focused Plan guards")
    need(all(final["sources"][p] == m for p, m in pd["source_before"].items()), "current owned Plan tests")
    plan_delta = {p: {"focused": m, "current": meta(R / p)} for p, m in pd["basis_before"].items() if meta(R / p) != m}
    need(plan_delta == prior["plan_scope"]["later_cfg_test_only_delta"], "same explicit historical Core cfg(test) basis delta")
    worker = obj("worker-sequence-final-freeze.json")
    worker_members = archives["archives/worker-final-source.tar.gz"]
    need({p[:-4]: sha(b) for p, b in worker_members.items()} == worker["files"], "all six final worker snapshots retained")
    failure = worker["retained_failure"]
    need(sha(scratch(failure["source_copy"])) == failure["source_sha256"] and failure["production_defect"] is False,
         "actual initial CLI failure source retained")
    old_cli = obj("worker-cli-1.json")
    need(old_cli["exit_code"] == 101 and old_cli["owned_unchanged"] is True and old_cli["basis_unchanged"] is True,
         "initial CLI fixture failure terminal retained")
    need(re.findall(r"^test result: FAILED\. (\d+) passed; (\d+) failed; (\d+) ignored;",
                    scratch("worker-cli-1.stdout").decode(), re.M) == [("0", "1", "0")], "actual initial CLI failed result retained")
    eq = obj("candidate-equivalence.json")
    need(eq["source_paths"] == 14 and eq["all_source_bytes_identical"] is True and eq["all_five_binary_bytes_identical"] is True
         and first["sources"] == final["sources"], "one candidate source equivalence")
    need(all({k: first["binaries"][n][k] for k in ("bytes", "sha256")} ==
             {k: final["binaries"][n][k] for k in ("bytes", "sha256")} for n in final["binaries"]),
         "actual five executable candidate equivalence")
    unit = obj("app-unit-executable-terminal.json")
    need(unit["observed_live"] == obj("app-unit-executable-live.json") and unit["unchanged"] is True
         and unit["terminal"] == prior["unit_executable"] == meta(unit["observed_live"]["path"]), "actual unit live/terminal executable proof")
    visual = obj("visual-review.json")
    pngs = {}
    for name, row in rows.items():
        if not name.startswith("images/"):
            continue
        b = data[name]
        need(b[:8] == b"\x89PNG\r\n\x1a\n", "selected PNG signature")
        pos, dims, ended = 8, None, False
        while pos < len(b):
            need(pos + 12 <= len(b), "PNG chunk bounds")
            n = struct.unpack(">I", b[pos:pos + 4])[0]
            kind = b[pos + 4:pos + 8]
            need(pos + n + 12 <= len(b) and n <= LIMIT, "bounded PNG body")
            body = b[pos + 8:pos + 8 + n]
            need(binascii.crc32(kind + body) & 0xffffffff == struct.unpack(">I", b[pos + 8 + n:pos + 12 + n])[0], "PNG CRC")
            if kind == b"IHDR":
                dims = list(struct.unpack(">II", body[:8]))
                need(all(0 < x <= 8192 for x in dims), "bounded PNG raster")
            pos += n + 12
            if kind == b"IEND":
                need(n == 0 and pos == len(b), "PNG exact end")
                ended = True
        need(dims and ended, "complete PNG")
        original_rel = Path(row["source"]).relative_to(S.resolve()).as_posix()
        need({"bytes": len(b), "sha256": sha(b)} == visual["images"][original_rel], "Root visually inspected image identity")
        pngs[name] = {"bytes": len(b), "sha256": sha(b), "dimensions": dims}
    need(len(pngs) == 4, "four selected image witnesses")
    natives = []
    for n, scenario, count, ids in (
        ("native-sequence-first", "sequence-follow-creep", 107, first),
        ("native-hold-final", "hold-follow-creep", 67, final), ("native-zoom-final", "zoom", 213, final),
    ):
        hits = [k for k, r in rows.items() if Path(r["source"]).resolve() == (S / n / "report.json").resolve()]
        need(len(hits) == 1, "retained native report")
        report = json.loads(data[hits[0]])
        r = obj(n + ".json")
        need(report["failed"] is False and r["exit_code"] == 0 and r["sources_unchanged"] is True
             and r["binaries_unchanged"] is True and report["metadata"]["physical_display_measured"] is False
             and report["metadata"]["binary_sha256"] == ids["binaries"]["deadpan-app"]["sha256"], "native terminal/binary/proof boundary")
        scenes = {x["name"]: x for x in report["scenarios"]}
        need(set(scenes) == {"kestrel-shortcuts", scenario} and len(scenes[scenario]["checks"]) == count, "native exact count")
        need(all(c["passed"] is True for x in scenes.values() for c in x["checks"])
             and not any(f["severity"] == "failure" for x in scenes.values() for f in x["findings"]), "native raw successes")
        k = scenes["kestrel-shortcuts"]["checks"]
        need(len(k) == 1 and k[0]["actual"]["reserved_bindings"] == 62
             and k[0]["actual"]["routing_cases"] == 21885504 and k[0]["actual"]["conflicts"] == [], "raw Kestrel audit")
        previous = next(x for x in prior["native_reports"] if x["name"] == n)
        warnings = [f["message"].split(": [{")[0].split(": [[")[0] for f in scenes[scenario]["findings"] if f["severity"] == "warning"]
        need(warnings == previous["warnings"] and scenes[scenario]["skipped"] == previous["skips"], "warnings/skips unaltered")
        natives.append({"name": n, "checks": count, "kestrel": 1, "warnings": warnings, "skips": previous["skips"]})
    raw_tests = {}
    for n, expected in (("core-plan-final", (2049, 0, 0)), ("app-units-final", (1304, 0, 11)),
                        ("cli-framing-final", (13, 0, 0)), ("worker-core-1", (2, 0, 0)),
                        ("worker-cli-2", (1, 0, 0)), ("plan-focused-1", (2, 0, 0))):
        text = scratch(n + ".stdout").decode()
        ss = re.findall(r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", text, re.M)
        total = tuple(sum(int(x[i]) for x in ss) for i in range(3))
        need(total == expected and obj(n + ".json")["exit_code"] == 0, "raw test result totals")
        raw_tests[n] = list(total)
    for n in ("app-units-final", "release-final", "clippy-final", "format-first", "media-final", "cli-framing-final"):
        r = obj(n + ".json")
        need(r["exit_code"] == 0 and r["source_before"] == r["source_after"] == {str(R / p): m for p, m in final["sources"].items()},
             "final14 raw terminal scope")
        for p, m in r["streams"].items():
            need(meta(p) == m, "raw terminal output bytes")
    media = archives["archives/final-sequence-project-and-export.tar.gz"]
    get = lambda n: json.loads(media[n])
    commands = get("commands.json")
    need(len(commands) == 9 and all(c["exit_code"] == 0 for c in commands), "all nine real media commands")
    need(get("dump.stdout") == get("portable-dump.stdout") and get("validate.stdout") == get("portable-validate.stdout")
         and get("validate.stdout")["valid"] is True, "exact authored copy and real Store validate")
    v = get("verification.json")
    need(v == get("verify.stdout") and v["passed"] is True and v["failures"] == []
         and v["range"] == [0, 80] and v["frame_rate"] == [30000, 1001], "actual emitted verification")
    need(len(v["pictures"]) == 80 and all(p["passed"] is True and p["flags"] == [] for p in v["pictures"])
         and [p["output_frame"] for p in v["pictures"]] == list(range(80)), "complete actual picture rows")
    observable = collections.Counter(p[k]["status"] for p in v["pictures"] for k in ("previous_index", "next_index") if p[k] is not None)
    need(observable == {"observable": 152, "unobservable": 6}, "all observability limits preserved")
    a = v["audio"]
    need(len(a) == 1 and a[0]["passed"] is True and a[0]["flags"] == [] and a[0]["window"] == a[0]["project_samples"] == [0, 128128]
         and a[0]["uncovered_samples"] == 0 and a[0]["measured_offset_samples"] == 0 and a[0]["offset_status"] == "verified_zero"
         and all(x["peak_lag"] == 0 and x["status"] == "verified_zero" for x in a[0]["segments"]), "full zero-offset audio")
    need(v["movie"] == prior["actual_movie_metadata"] and v["movie"]["audio"]["gaps"] == [], "actual AAC/video edit metadata")
    movie = {"bytes": len(media["sequence-follow-creep.mp4"]), "sha256": sha(media["sequence-follow-creep.mp4"])}
    need(movie == prior["movie"] == {"bytes": 117671, "sha256": "f58bac4dd4e4dec609af135aa0958578ebe3bcb618cd0e089c1d2f086dc45611"}, "actual MP4 bytes")
    pubs = [d for line in media["render.stdout"].splitlines() for d in dicts(json.loads(line)) if "movie_sha256" in d]
    need(pubs and all(d == pubs[0] for d in pubs) and pubs[0]["movie_sha256"] == movie["sha256"]
         and sha(media[Path(pubs[0]["report"]).name]) == pubs[0]["report_sha256"], "actual movie/render receipt binding")
    need(get("copy.stdout")["portable_copy"]["verified_history"] == {"replayed": 0, "revisions": 26, "verified_by_receipt": 26}, "portable full history receipt")
    original = archives["archives/actual-native-sequence-project.tar.gz"]
    histories = []
    for root, members, key in ((Path(aa[4]["root"]), original, "project.sqlite"),
                                (Path(aa[3]["root"]), media, "portable.deadpan/project.sqlite")):
        db = root / key
        wal = db.with_name(db.name + "-wal")
        need(not wal.exists() or wal.stat().st_size == 0, "empty WAL before immutable read")
        m = meta(db)
        need(m == {"bytes": len(members[key]), "sha256": sha(members[key])}, "actual original/portable database retained")
        with sqlite3.connect(db.as_uri() + "?mode=ro&immutable=1", uri=True) as con:
            need(con.execute("PRAGMA quick_check").fetchall() == [("ok",)], "immutable SQLite integrity")
            tables = {t: sorted(con.execute('SELECT * FROM "' + t + '"').fetchall(), key=repr)
                      for t in ("revisions", "revision_patches", "history", "history_receipt", "redo", "state", "transaction_steps")}
        need(meta(db) == m, "database unchanged")
        histories.append(tables)
    need(histories[0] == histories[1] and len(histories[0]["revisions"]) == 26 and len(histories[0]["history"]) == 15, "complete retained history equality/counts")
    result.update(status="no_findings", findings=[], inventory_sha256=INV, primary_files=135,
                  payloads=134, payload_bytes=8457096, encoding_records=131, encodings=dict(counts),
                  raw_payload_bytes=114876734, budget_bytes=LIMIT, archives=summaries, tar_members=68,
                  tar_member_bytes=4626097, authored_omissions=0, metadata_omissions=0, final_source_paths=14,
                  embedded_raw_audit_sha256=AUDIT, first_audit_stop_preserved=True, selected_pngs=pngs,
                  native=natives, raw_tests=raw_tests, actual_media_commands=9, movie=movie,
                  picture_rows=80, audio_window=[0, 128128], audio_offset=0, picture_neighbor_observability=dict(observable),
                  portable_revisions=26, portable_history_rows=15, all_selected_history_tables_equal=True,
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
    result["initial_audit_history_unchanged"] = history_before == tree(S / "audit-history")
    result["current_scoped_sources_unchanged"] = source_before == source_after
    if not all(result[k] for k in ("primary_tree_unchanged", "earlier_audit_files_unchanged", "initial_audit_history_unchanged", "current_scoped_sources_unchanged")):
        result["status"] = "findings"
        result["findings"].append({"message": "Primary, audit history or scoped sources changed during verification"})
    result["prior_audit_file_pins"] = old_before
    result["verifier_sha256"] = meta(Path(__file__))["sha256"]
    result["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (S / "package-verification.json").open("x") as f:
        json.dump(result, f, indent=2, sort_keys=True)
        f.write("\n")
    print(json.dumps({k: result.get(k) for k in ("status", "findings", "primary_files", "payloads", "payload_bytes",
                                              "inventory_sha256", "tar_members", "primary_tree_unchanged", "earlier_audit_files_unchanged")}, sort_keys=True))
sys.exit(0 if result["status"] == "no_findings" else 1)
