#!/usr/bin/env python3
"""Independent byte/provenance audit for a published Deadpan evidence collection.

The verifier reads only the published payload and retained scratch receipts. It
does not import or execute collection helpers and does not perform product tests.
"""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import sqlite3
import stat
import sys

CAP = 256 * 1024 * 1024
REPO = Path("/Users/michael/Code/deadpan")
KESTREL = Path("/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift")
BASE = "cced3e3519e111514e049c8b3e9d96ce3c4acd7d"
METADATA = {"plan.json", "collection.json", "files.json"}
NEW_PLACEMENT_GUARD = (
    "Scoped contents refuse fresh and pre-scope placement captures and disable direct slice buttons, "
    "with or without a Time range"
)


class AuditError(Exception):
    pass


def need(ok, message):
    if not ok:
        raise AuditError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_regular(path, cap=CAP):
    path = Path(path)
    st = path.lstat()
    need(stat.S_ISREG(st.st_mode), f"not a regular non-symlink file: {path}")
    need(st.st_size <= cap, f"file exceeds {cap}-byte cap: {path}")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_NONBLOCK", 0))
    try:
        before = os.fstat(fd)
        need(stat.S_ISREG(before.st_mode), f"not a regular file: {path}")
        need(before.st_size <= cap, f"file exceeds {cap}-byte cap: {path}")
        pieces, total = [], 0
        while True:
            block = os.read(fd, min(1024 * 1024, cap + 1 - total))
            if not block:
                break
            pieces.append(block)
            total += len(block)
            need(total <= cap, f"bounded read exceeded cap: {path}")
        after = os.fstat(fd)
        need((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) ==
             (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns),
             f"file changed during read: {path}")
        return b"".join(pieces)
    finally:
        os.close(fd)


def json_bytes(data, label):
    try:
        return json.loads(data)
    except Exception as exc:
        raise AuditError(f"invalid JSON {label}: {exc}") from exc


def path_inside(path, root):
    try:
        Path(path).resolve(strict=True).relative_to(Path(root).resolve(strict=True))
        return True
    except (ValueError, FileNotFoundError, OSError):
        return False


def has_symlink_component(path):
    """Check the lexical absolute path, without following any component."""
    absolute = Path(os.path.abspath(path))
    current = Path(absolute.anchor)
    for component in absolute.parts[1:]:
        current = current / component
        try:
            if stat.S_ISLNK(current.lstat().st_mode):
                return True
        except FileNotFoundError:
            return False
    return False


def source_pins_relative_to_repo(pins, label):
    need(isinstance(pins, dict), f"source pins are not an object: {label}")
    normalized = {}
    for name, pin in pins.items():
        path = Path(name)
        need(path.is_absolute() and path.is_relative_to(REPO),
             f"receipt source path is outside repository: {label}/{name}")
        relative = path.relative_to(REPO).as_posix()
        need(relative not in normalized, f"duplicate normalized source path: {label}/{relative}")
        normalized[relative] = pin
    return normalized


def canonical_digest(value):
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return digest(encoded)


def project_history(package):
    database = Path(package) / "project.sqlite"
    uri = database.as_uri() + "?mode=ro"
    with sqlite3.connect(uri, uri=True) as db:
        queries = {
            "revisions": "SELECT id,parent_id,kind FROM revisions ORDER BY id",
            "history": "SELECT id,parent_id,revision_id,request,edit FROM history ORDER BY id",
            "redo": "SELECT position,history_id FROM redo ORDER BY position",
            "state": "SELECT head_revision,cursor,workflow FROM state ORDER BY singleton",
        }
        values = {name: [list(row) for row in db.execute(query).fetchall()]
                  for name, query in queries.items()}
    return {
        "counts": {name: len(rows) for name, rows in values.items()},
        "sha256": canonical_digest(values),
        "state": values["state"],
        "revision_kinds": {kind: sum(row[2] == kind for row in values["revisions"])
                           for kind in ("initial", "edit", "undo", "redo")},
    }


def package_snapshot(package):
    package = Path(package)
    need(stat.S_ISDIR(package.lstat().st_mode), f"source project is not a real directory: {package}")
    persistent, directories, excluded = [], [], []

    def walk(directory):
        with os.scandir(directory) as entries:
            ordered = sorted(entries, key=lambda entry: entry.name)
        for entry in ordered:
            path = Path(entry.path)
            rel = path.relative_to(package).as_posix()
            mode = entry.stat(follow_symlinks=False).st_mode
            need(not stat.S_ISLNK(mode), f"source project contains symlink: {rel}")
            if stat.S_ISDIR(mode):
                directories.append(rel)
                walk(path)
            elif stat.S_ISREG(mode):
                raw = read_regular(path, CAP)
                row = {"relative_path": rel, "bytes": len(raw), "sha256": digest(raw)}
                if rel == "project.sqlite-shm":
                    row["reason"] = "transient SQLite shared-memory bytes; no persistence assertion"
                    excluded.append(row)
                else:
                    persistent.append(row)
            else:
                raise AuditError(f"source project contains special file: {rel}")

    walk(package)
    persistent.sort(key=lambda row: row["relative_path"])
    directories.sort()
    return {"persistent_files": persistent, "directories": directories,
            "excluded_transient_files": excluded,
            "persistent_inventory_sha256": canonical_digest({"directories": directories, "files": persistent})}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--collection", required=True, type=Path)
    parser.add_argument("--scratch", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--require-media", action="store_true",
                         help="fail unless a retained, successful final-source media proof is complete")
    parser.add_argument("--require-release", action="store_true",
                         help="fail unless the exact-source release build and scoped native replay are verified")
    args = parser.parse_args()
    need(not has_symlink_component(args.collection), "collection path contains a symlink component")
    root = args.collection.resolve(strict=True)
    scratch = args.scratch.resolve(strict=True)
    out = args.output
    need(not out.exists() and not out.is_symlink(), f"output must be fresh: {out}")
    repo_collection = REPO / "tools/media-qualification/evidence/2026-10-25-scoped-framing-ranges"
    in_scratch = path_inside(root, scratch)
    exact_repo_collection = root == repo_collection.resolve(strict=True)
    need(in_scratch or exact_repo_collection,
         "collection must be inside scratch or the exact approved repository evidence directory")
    need(out.parent.is_dir() and path_inside(out.parent, scratch), "output parent must be an existing directory inside scratch")
    checks = {}
    result = {"ok": False, "scope": "published evidence integrity and provenance only"}
    try:
        need(root.is_dir(), "collection root missing or not a directory")
        plan_raw = read_regular(root / "plan.json", 8_000_000)
        collection_raw = read_regular(root / "collection.json", 8_000_000)
        inventory_raw = read_regular(root / "files.json", 8_000_000)
        plan = json_bytes(plan_raw, "plan.json")
        collection = json_bytes(collection_raw, "collection.json")
        inventory = json_bytes(inventory_raw, "files.json")
        need(plan.get("schema_version") == 1 and collection.get("schema_version") == 1
             and inventory.get("schema_version") == 1, "unsupported evidence schema")
        need(plan.get("base") == BASE and collection.get("base") == BASE, "base revision differs")
        omissions = plan.get("omissions")
        need(isinstance(omissions, list) and omissions == collection.get("omissions"),
             "plan and collection omission ledgers differ")
        need(plan.get("exclusions") and collection.get("exclusions"), "scope exclusions are absent")
        plan_hash = digest(plan_raw)
        collector_hash = plan.get("collector", {}).get("sha256")
        need(len(collector_hash or "") == 64, "collector hash is missing")
        collector_alias = "metadata/collect-evidence.py.txt"
        need(collection.get("plan_sha256") == plan_hash, "collection plan digest differs")
        need(collection.get("collector_sha256") == collector_hash, "collector digest differs")
        need(collection.get("logical_inputs") == len(plan.get("inputs", [])),
             "collection logical input count differs from plan")
        need(collection.get("payload_objects") == len(plan.get("objects", [])),
             "collection payload count differs from plan")
        checks["plan_sha256"] = plan_hash
        checks["collector_sha256"] = collector_hash

        inputs = plan.get("inputs", [])
        objects = plan.get("objects", [])
        need(inputs and objects, "plan has no inputs or payload objects")
        input_by_name = {}
        for row in inputs:
            logical = row.get("logical")
            need(isinstance(logical, str) and logical and logical not in input_by_name,
                 f"invalid or duplicate logical input: {logical}")
            need(type(row.get("bytes")) is int and row["bytes"] >= 0, f"bad byte count: {logical}")
            need(re.fullmatch(r"[0-9a-f]{64}", row.get("sha256", "")) is not None,
                 f"bad input SHA-256: {logical}")
            need(not logical.endswith(".rs"), f"bare Rust source snapshot is prohibited: {logical}")
            if logical.startswith("sources/"):
                need(not re.search(r"\\.rs$", logical) or logical.endswith(".rs.txt"),
                     f"Rust source snapshot lacks .txt suffix: {logical}")
            input_by_name[logical] = row

        need(collector_alias in input_by_name
             and input_by_name[collector_alias]["sha256"] == collector_hash,
             "retained collector bytes do not match the approved collector hash")
        expected_omitted = {
            "media/media-first/baseline-decoded.yuv",
            "media/media-first/final-decoded.yuv",
        }
        need({row.get("logical") for row in omissions} == expected_omitted,
             "omission ledger does not name exactly the two declared optional decoded references")
        for row in omissions:
            logical = row.get("logical")
            source = Path(row.get("source", ""))
            need(logical not in input_by_name and path_inside(source, scratch)
                 and row.get("reason") ==
                 "Optional raw decoded/I420 reference omitted for the bounded collection; identity pinned, no retained-byte claim.",
                 f"omitted optional media input has an invalid scope or claim: {logical}")
            raw = read_regular(source, CAP)
            need(type(row.get("bytes")) is int and len(raw) == row["bytes"]
                 and row.get("sha256") == digest(raw),
                 f"omitted optional media input identity does not match its live scratch bytes: {logical}")
        checks["explicit_optional_omissions"] = sorted(expected_omitted)

        alias_to_object = {}
        object_paths = set()
        for obj in objects:
            rel = obj.get("path")
            need(isinstance(rel, str) and not Path(rel).is_absolute() and ".." not in Path(rel).parts,
                 f"unsafe payload path: {rel}")
            need(rel not in object_paths, f"duplicate payload path: {rel}")
            object_paths.add(rel)
            aliases = obj.get("aliases")
            need(isinstance(aliases, list) and aliases, f"payload lacks aliases: {rel}")
            for alias in aliases:
                need(alias in input_by_name, f"payload has unknown alias: {alias}")
                need(alias not in alias_to_object, f"logical alias appears twice: {alias}")
                alias_to_object[alias] = obj
            need(obj.get("encoding") in ("identity", "gzip"), f"unsupported encoding for {rel}")
        need(set(alias_to_object) == set(input_by_name), "payload aliases do not cover exact input inventory")
        checks["logical_inputs"] = len(inputs)
        checks["payload_objects"] = len(objects)

        inv_rows = inventory.get("files")
        need(isinstance(inv_rows, list), "inventory files is not a list")
        inv = {}
        for row in inv_rows:
            rel = row.get("path")
            need(isinstance(rel, str) and rel not in inv, f"bad/duplicate inventory path: {rel}")
            relpath = Path(rel)
            need(not relpath.is_absolute() and ".." not in relpath.parts and "." not in relpath.parts,
                 f"unsafe inventory path: {rel}")
            inv[rel] = row
        expected_paths = set(inv) | {"files.json"}
        actual_paths, actual_dirs = set(), set()
        for base, dirs, files in os.walk(root, followlinks=False):
            parent = Path(base)
            for name in list(dirs):
                p = parent / name
                need(not p.is_symlink(), f"symlink directory: {p}")
                actual_dirs.add(p.relative_to(root).as_posix())
            for name in files:
                p = parent / name
                need(not p.is_symlink(), f"symlink file: {p}")
                actual_paths.add(p.relative_to(root).as_posix())
        need(actual_paths == expected_paths,
             f"published tree mismatch: missing={sorted(expected_paths-actual_paths)[:5]} "
             f"extra={sorted(actual_paths-expected_paths)[:5]}")
        expected_dirs = set()
        for name in expected_paths:
            expected_dirs.update(p.as_posix() for p in Path(name).parents if p.as_posix() != ".")
        need(actual_dirs == expected_dirs, "published directory tree differs from inventory")
        need(METADATA <= actual_paths, "required metadata file absent")
        need(inventory.get("inventory_not_self_listed") is True, "inventory self-listing marker differs")
        need(inventory.get("file_count_including_inventory") == len(actual_paths),
             "inventory total file count differs")
        need(inventory.get("cap_bytes_each") == CAP, "published per-file cap differs")

        bytes_by_path = {}
        sums = {"stored_bytes": 0, "raw_bytes": 0, "decoded_bytes": 0}
        max_seen = 0
        for rel, row in inv.items():
            blob = read_regular(root / rel, CAP)
            need(len(blob) == row.get("bytes"), f"stored byte count differs: {rel}")
            need(digest(blob) == row.get("sha256"), f"stored SHA-256 differs: {rel}")
            encoding = row.get("encoding")
            if rel in METADATA or encoding == "identity":
                decoded = blob
            elif encoding == "gzip":
                stream = gzip.GzipFile(fileobj=io.BytesIO(blob))
                chunks, total = [], 0
                while True:
                    chunk = stream.read(min(1024 * 1024, CAP + 1 - total))
                    if not chunk:
                        break
                    chunks.append(chunk)
                    total += len(chunk)
                    need(total <= CAP, f"decoded file exceeds cap: {rel}")
                decoded = b"".join(chunks)
                need(row.get("input_sha256") == Path(rel).name.split(".")[0],
                     f"gzip input digest/path mismatch: {rel}")
            else:
                raise AuditError(f"unknown inventory encoding: {rel}/{encoding}")
            need(len(decoded) == row.get("decoded_bytes"), f"decoded byte count differs: {rel}")
            need(len(decoded) == row.get("raw_bytes"), f"raw byte count differs: {rel}")
            need(len(blob) <= CAP and len(decoded) <= CAP, f"file exceeds cap: {rel}")
            max_seen = max(max_seen, len(blob), len(decoded))
            sums["stored_bytes"] += len(blob)
            sums["decoded_bytes"] += len(decoded)
            sums["raw_bytes"] += len(decoded)
            bytes_by_path[rel] = decoded
        sums["stored_bytes"] += len(inventory_raw)
        sums["decoded_bytes"] += len(inventory_raw)
        sums["raw_bytes"] += len(inventory_raw)
        reported = inventory.get("totals_including_inventory", {})
        for key, value in sums.items():
            inventory_key = "bytes" if key == "stored_bytes" else key
            need(reported.get(inventory_key) == value, f"inventory {key} total differs")
        checks["published_files_including_inventory"] = len(actual_paths)
        checks["byte_totals"] = sums
        checks["largest_stored_or_decoded_file"] = max_seen

        # Independently reconstitute every logical alias from published bytes.
        for logical, obj in alias_to_object.items():
            rel = obj["path"]
            need(rel in bytes_by_path, f"payload object is absent from inventory: {rel}")
            raw = bytes_by_path[rel]
            row = input_by_name[logical]
            need(len(raw) == row["bytes"] and digest(raw) == row["sha256"],
                 f"logical alias content differs: {logical}")
            need(row["bytes"] == obj.get("raw_bytes", row["bytes"]),
                 f"object raw byte count differs for alias: {logical}")
        checks["payload_alias_bytes_verified"] = len(alias_to_object)

        # Source snapshots and phase identities are compared against the exact
        # inputs that the collector published, then the final set against live files.
        phases = plan.get("source_phases", {})
        need(phases and phases == collection.get("source_phases"), "source phase manifests differ")
        phase_files = {}
        for phase, info in phases.items():
            logical = info.get("identity_file")
            need(logical in input_by_name, f"source identity alias missing: {phase}")
            wrapper = json_bytes(bytes_by_path[alias_to_object[logical]["path"]], logical)
            pins = wrapper.get("files", {})
            need(wrapper.get("base") == BASE and len(pins) == info.get("inputs"),
                 f"source identity wrapper differs: {phase}")
            for name, pin in pins.items():
                need(not Path(name).is_absolute() and ".." not in Path(name).parts,
                     f"unsafe phase path: {phase}/{name}")
                alias = f"sources/{phase}/{name}.txt"
                need(alias in input_by_name, f"published source snapshot missing: {alias}")
                raw = bytes_by_path[alias_to_object[alias]["path"]]
                need(len(raw) == pin.get("bytes") and digest(raw) == pin.get("sha256"),
                     f"source phase bytes mismatch: {phase}/{name}")
            phase_files[phase] = pins
        source_list_name = "metadata/source-files.json"
        need(source_list_name in input_by_name, "source-files.json snapshot missing")
        current_names = json_bytes(bytes_by_path[alias_to_object[source_list_name]["path"]], source_list_name)
        need(isinstance(current_names, list) and current_names, "source-files list is empty")
        need(len(current_names) == 33, "final source inventory is not the expected 33 files")
        candidate_phases = [phase for phase, pins in phase_files.items() if set(pins) == set(current_names)]
        need(candidate_phases, "no source phase exactly covers source-files.json")
        # Select a full 33-file phase that matches the live source tree. Prefer
        # the latest successful Cargo build receipt among those exact matches.
        assoc_logical = "metadata/run-source-associations.json"
        need(assoc_logical in input_by_name, "run/source association ledger missing")
        associations = json_bytes(bytes_by_path[alias_to_object[assoc_logical]["path"]], assoc_logical)
        meta_assoc_logical = "metadata/meta-run-associations.json"
        need(meta_assoc_logical in input_by_name, "publication/audit association ledger missing")
        meta_associations = json_bytes(bytes_by_path[alias_to_object[meta_assoc_logical]["path"]],
                                       meta_assoc_logical)
        need(isinstance(meta_associations, dict)
             and plan.get("meta_run_receipts") == meta_associations,
             "plan and retained publication/audit association ledgers differ")
        need(plan.get("qualification_run_receipts") == sorted(associations),
             "plan qualification receipt names differ from the exact source association ledger")
        live_matching = []
        for phase in candidate_phases:
            pins = phase_files[phase]
            matches_live = True
            for name, pin in pins.items():
                live = read_regular(REPO / name, CAP)
                if len(live) != pin["bytes"] or digest(live) != pin["sha256"]:
                    matches_live = False
                    break
            if matches_live:
                live_matching.append(phase)
        need(live_matching, "no full source phase matches current live files byte-for-byte")

        # Validate exact receipt/stream bytes directly in scratch and in the
        # published aliases. No collector logic is imported.
        receipt_names = set()
        receipts = {}
        for path in scratch.glob("*.json"):
            raw = read_regular(path, CAP)
            row = json_bytes(raw, path.name)
            if isinstance(row, dict) and "command" in row and "source_before" in row and "runner_sha256" in row:
                receipt_names.add(path.name)
                receipts[path.name] = row
        qualification_names = receipt_names & set(associations)
        meta_receipt_names = receipt_names - set(associations)
        need(qualification_names == set(associations),
             "run/source ledger omits a qualification runner receipt")
        planned_meta_names = set(meta_associations)
        post_plan_publication_names = {
            name for name in meta_receipt_names
            if re.fullmatch(r"evidence-(?:dry-run|collect)-final[0-9]+\.json", name)}
        allowed_meta = planned_meta_names | post_plan_publication_names
        allowed_meta.update(name for name in meta_receipt_names
                            if re.fullmatch(r"byte-audit-[a-z0-9-]+\.json", name))
        need(meta_receipt_names <= allowed_meta,
             f"unclassified Root runner receipts: {sorted(meta_receipt_names-allowed_meta)}")
        # Only the named evidence-publication and byte-audit commands may sit
        # outside the qualification association ledger. Their exact commands,
        # outcomes and completed streams are checked separately below.
        qualification_receipts = {name: receipts[name] for name in qualification_names}
        build_phase_candidates = []
        for receipt_name, record in qualification_receipts.items():
            argv = record.get("command", [])
            if (argv[:2] != ["cargo", "build"] or record.get("exit_code") != 0
                    or record.get("sources_unchanged") is not True
                    or record.get("source_before") != record.get("source_after")):
                continue
            normalized_build_sources = source_pins_relative_to_repo(
                record.get("source_before", {}), f"{receipt_name}/source_before")
            refs = associations[receipt_name].get("source_phases", {}).get("source_before", [])
            for phase in refs:
                if phase in live_matching and normalized_build_sources == phase_files[phase]:
                    build_phase_candidates.append((record.get("finished_at", ""), phase))
        need(build_phase_candidates, "no successful Cargo build receipt covers a live-matching source phase")
        final_phase = max(build_phase_candidates)[1]
        final_pins = phase_files[final_phase]
        checks["source_phases"] = {name: len(pins) for name, pins in phase_files.items()}
        checks["final_source_phase"] = final_phase
        checks["final_live_source_files_verified"] = len(final_pins)
        runner_path = scratch / "run.py"
        runner_pin = digest(read_regular(runner_path, CAP))
        runner_alias = "metadata/run.py.txt"
        need(runner_alias in input_by_name and input_by_name[runner_alias]["sha256"] == runner_pin,
             "published root runner bytes differ from actual runner receipts")
        def receipt_option(command, option, label):
            need(command.count(option) == 1 and command.index(option) + 1 < len(command),
                 f"meta receipt lacks a unique {option}: {label}")
            return command[command.index(option) + 1]

        def validate_meta_stable_receipt(name, row):
            need(row.get("runner_sha256") == runner_pin and row.get("cwd") == str(REPO),
                 f"meta receipt runner/cwd differs: {name}")
            for field in ("source_before", "source_after"):
                pins = row.get(field)
                need(isinstance(pins, dict), f"meta receipt lacks {field}: {name}")
                normalized = source_pins_relative_to_repo(pins, f"{name}/{field}")
                need(any(normalized == phase_files[phase] for phase in phase_files),
                     f"meta receipt {field} is not an exact retained source phase: {name}")
            need(row.get("source_before") == row.get("source_after")
                 and row.get("sources_unchanged") is True
                 and row.get("binaries_unchanged") is True
                 and row.get("binary_before", {}) == row.get("binary_after", {}),
                 f"meta receipt source/binary stability is incomplete or contradictory: {name}")
            need(type(row.get("exit_code")) is int and type(row.get("pid")) is int and row["pid"] > 0
                 and isinstance(row.get("finished_at"), str) and row["finished_at"]
                 and isinstance(row.get("seconds"), (int, float)) and row["seconds"] >= 0,
                 f"meta receipt has no complete terminal outcome: {name}")
            actual_streams = {}
            for stream in ("stdout", "stderr"):
                stream_path = scratch / Path(name).with_suffix(f".{stream}")
                raw = read_regular(stream_path, CAP)
                pin = {"bytes": len(raw), "sha256": digest(raw)}
                need(row.get("streams", {}).get(str(stream_path.resolve())) == pin,
                     f"meta receipt stream pin differs: {name}/{stream}")
                actual_streams[stream] = pin
            return actual_streams

        meta_outcomes = {}
        audit_script = Path(__file__).resolve(strict=True)
        repo_collection = REPO / "tools/media-qualification/evidence/2026-10-25-scoped-framing-ranges"
        current_audit_receipt = None

        def validate_planned_meta(name, classification, row, actual_streams):
            need(name in meta_associations, f"meta receipt is not in the frozen plan ledger: {name}")
            phase_association = {}
            for field in ("source_before", "source_after"):
                if field not in row:
                    continue
                normalized = source_pins_relative_to_repo(row[field], f"{name}/{field}")
                matches = [phase for phase, pins in phase_files.items() if pins == normalized]
                need(matches, f"meta receipt source phase is absent: {name}/{field}")
                phase_association[field] = matches
            complete = all(k in row for k in ("exit_code", "finished_at", "source_after", "streams", "pid", "seconds"))
            expected_note = ("Actual complete terminal receipt; exit code is retained, including failures."
                             if complete else
                             "Incomplete actual runner record; no exit status, duration, or successful native execution is inferred.")
            expected = {
                "classification": classification,
                "complete_terminal_receipt": complete,
                "exit_code": row.get("exit_code"),
                "pid_recorded": row.get("pid"),
                "source_phases": phase_association,
                "actual_streams": actual_streams,
                "note": expected_note,
            }
            need(meta_associations[name] == expected,
                 f"frozen meta association contradicts the actual receipt/streams: {name}")
            receipt_alias = f"metadata/{name}"
            need(receipt_alias in input_by_name
                 and bytes_by_path[alias_to_object[receipt_alias]["path"]]
                    == read_regular(scratch / name, CAP),
                 f"published meta receipt bytes differ from the actual Root receipt: {name}")
            for stream in ("stdout", "stderr"):
                stream_alias = f"metadata/{Path(name).stem}.{stream}"
                stream_path = scratch / Path(name).with_suffix(f".{stream}")
                need(stream_alias in input_by_name
                     and bytes_by_path[alias_to_object[stream_alias]["path"]]
                        == read_regular(stream_path, CAP),
                     f"published meta receipt stream differs from scratch: {name}/{stream}")

        for name in sorted(meta_receipt_names):
            row = receipts[name]
            command = row.get("command")
            need(isinstance(command, list) and command and all(isinstance(x, str) for x in command),
                 f"meta receipt command is malformed: {name}")
            need(row.get("runner_sha256") == runner_pin and row.get("cwd") == str(REPO),
                 f"meta receipt runner/cwd differs: {name}")
            planned_meta = meta_associations.get(name)
            is_dry_run = name.startswith("evidence-dry-run")
            is_collect = name.startswith("evidence-collect")
            if is_dry_run or is_collect:
                need(len(command) >= 2 and Path(command[1]).resolve(strict=True)
                     == (scratch / "evidence/collect-evidence.py").resolve(strict=True),
                     f"evidence publication receipt runs an unexpected command: {name}")
                need(Path(command[0]).name in {"python", "python3"},
                     f"evidence publication receipt does not use Python: {name}")
                plan_arg = Path(receipt_option(command, "--plan", name)).resolve(strict=True)
                need(plan_arg == (scratch / "evidence/plan.json").resolve(strict=True),
                     f"evidence publication receipt uses a different plan: {name}")
                approved_plan_hash = receipt_option(command, "--approved-plan-sha256", name)
                approved_script_hash = receipt_option(command, "--approved-script-sha256", name)
                need(approved_script_hash == collector_hash,
                     f"evidence publication receipt collector approval differs: {name}")
                historical_plan = scratch / "superseded-first/tree/plan.json"
                historical_plan_hash = digest(read_regular(historical_plan, CAP))
                report_path = Path(receipt_option(command, "--report", name)).resolve()
                suffix = (name.removeprefix("evidence-dry-run").removesuffix(".json") if is_dry_run
                          else name.removeprefix("evidence-collect").removesuffix(".json"))
                if is_dry_run:
                    report_name = "dry-run" + suffix + ".json"
                    need(command.count("--dry-run") == 1 and command.count("--output") == 0
                         and report_path == (scratch / "evidence" / report_name).resolve(),
                         "dry-run receipt command/report differs")
                    expected_mode, expected_verified = "dry-run", False
                else:
                    output_path = Path(receipt_option(command, "--output", name)).resolve(strict=True)
                    report_name = "collection-run" + suffix + ".json"
                    need(command.count("--dry-run") == 0 and output_path == repo_collection.resolve(strict=True)
                         and report_path == (scratch / "evidence" / report_name).resolve(),
                         "collection receipt command/output/report differs")
                    expected_mode, expected_verified = "collected", True
                actual_streams = validate_meta_stable_receipt(name, row)
                classification = "publication_dry_run" if is_dry_run else "publication_collect"
                if name in planned_meta_names:
                    need(planned_meta.get("classification") == classification,
                         f"publication receipt's frozen class differs: {name}")
                    validate_planned_meta(name, classification, row, actual_streams)
                else:
                    need(name in post_plan_publication_names,
                         f"unplanned publication receipt is not one of the final pair: {name}")
                if row["exit_code"] != 0:
                    failure_bytes = read_regular(scratch / Path(name).with_suffix(".stderr"), CAP)
                    if is_dry_run:
                        need(name in planned_meta_names and not report_path.exists()
                             and approved_plan_hash != plan_hash
                             and b"plan approval hash differs" in failure_bytes,
                             f"publication failure is not the retained plan-hash preflight case: {name}")
                        failure_class = "failed_preflight_plan_hash"
                    else:
                        output_path = Path(receipt_option(command, "--output", name)).resolve()
                        need(name == "evidence-collect-final3.json"
                             and output_path == repo_collection.resolve()
                             and not report_path.exists()
                             and b"refusing existing output" in failure_bytes,
                             f"publication failure is not the retained existing-output refusal: {name}")
                        failure_class = "failed_preflight_existing_output"
                    meta_outcomes[name] = {"class": failure_class,
                                           "exit_code": row["exit_code"], "streams": row["streams"],
                                           "approved_plan_sha256": approved_plan_hash}
                    continue
                need(report_path.is_file() and not report_path.is_symlink(),
                     f"successful publication report is missing or unsafe: {name}")
                report = json_bytes(read_regular(report_path, 8_000_000), str(report_path))
                need(report.get("mode") == expected_mode
                     and report.get("published_tree_verified") is expected_verified
                     and report.get("plan_sha256") == approved_plan_hash
                     and report.get("collector_sha256") == collector_hash,
                     f"publication report does not match the exact approved plan and collector: {name}")
                if approved_plan_hash == plan_hash:
                    need(report.get("inventory_sha256") == digest(inventory_raw)
                         and report.get("files_including_metadata") == inventory.get("file_count_including_inventory")
                         and report.get("logical_inputs") == len(inputs)
                         and report.get("payload_objects") == len(objects)
                         and report.get("omission_count") == len(omissions)
                         and report.get("verified_inputs_and_guards") == len(inputs) + len(plan.get("runtime_guards", []))
                         and report.get("totals_including_metadata") == inventory.get("totals_including_inventory"),
                         f"current-plan publication report contradicts the retained collection: {name}")
                else:
                    # Older successful publications are retained as execution
                    # evidence; their exact command approval is bound to their
                    # preserved report, while current-plan equivalence is checked
                    # above for the final publication.
                    need(report.get("files_including_metadata", 0) > 0
                         and report.get("logical_inputs", 0) > 0
                         and report.get("payload_objects", 0) > 0
                         and report.get("totals_including_metadata", {}).get("decoded_bytes", CAP + 1) <= CAP,
                         f"historical publication report has invalid bounded totals: {name}")
                if name in planned_meta_names:
                    report_alias = f"publication-reports/{report_path.name}"
                    need(report_alias in input_by_name
                         and bytes_by_path[alias_to_object[report_alias]["path"]]
                            == read_regular(report_path, CAP),
                         f"published evidence report differs from actual output: {name}")
                meta_outcomes[name] = {"class": expected_mode, "exit_code": row["exit_code"],
                                       "streams": row["streams"]}
                need(row["exit_code"] == 0, f"evidence publication command failed: {name}")
                continue

            # Byte-audit receipts are a separate meta-proof class. A runner
            # writes its terminal fields only after this verifier exits, so the
            # one invocation whose --output equals this run's requested output
            # must be incomplete and is reported as in progress.
            script_positions = [i for i, value in enumerate(command)
                                if Path(value).name == "verify_evidence.py"]
            if name == "byte-audit-second.json":
                expected_bad_command = [
                    "--binary", str(scratch / "bin-release-eighteenth/deadpan-app"),
                    "--binary", str(scratch / "bin-release-eighteenth/deadpan-cli"),
                    "--binary", str(scratch / "media-helper"),
                    "--binary", "/opt/homebrew/Cellar/ffmpeg/9.0.2_1/bin/ffmpeg",
                    "--", "python3", str(audit_script),
                    "--collection", str(repo_collection), "--scratch", str(scratch),
                    "--output", str(scratch / "audit/published-result-second.json"),
                    "--require-media", "--require-release",
                ]
                need(command == expected_bad_command,
                     "preserved byte-audit-second malformed command differs from the recorded invocation")
                need("exit_code" not in row and "pid" not in row and "streams" not in row
                     and not any(k in row for k in ("finished_at", "source_after", "seconds")),
                     "byte-audit-second unexpectedly has terminal child fields")
                for stream in ("stdout", "stderr"):
                    stream_path = scratch / Path(name).with_suffix(f".{stream}")
                    need(read_regular(stream_path, CAP) == b"",
                         f"prelaunch-failed runner unexpectedly emitted child {stream}: {name}")
                actual_streams = {
                    stream: {"bytes": 0, "sha256": digest(b"")}
                    for stream in ("stdout", "stderr")
                }
                validate_planned_meta(name, "audit_runner_prelaunch_failure", row, actual_streams)
                meta_outcomes[name] = {"class": "runner_prelaunch_failure", "exit_code": None,
                                       "detail": "runner command begins with --binary after the label; no child PID or terminal receipt"}
                continue

            need(len(script_positions) == 1 and Path(command[script_positions[0]]).resolve(strict=True) == audit_script,
                 f"byte-audit receipt does not invoke this verifier exactly once: {name}")
            need(Path(command[0]).name in {"python", "python3"},
                 f"byte-audit receipt does not use Python: {name}")
            collection_arg = Path(receipt_option(command, "--collection", name)).resolve(strict=True)
            scratch_arg = Path(receipt_option(command, "--scratch", name)).resolve(strict=True)
            output_arg = Path(receipt_option(command, "--output", name)).resolve()
            need(collection_arg == repo_collection.resolve(strict=True) and scratch_arg == scratch
                 and output_arg.parent == (scratch / "audit").resolve(strict=True)
                 and command.count("--require-media") == 1 and command.count("--require-release") == 1,
                 f"byte-audit receipt scope/required gates differ: {name}")
            if output_arg == args.output.resolve():
                need(current_audit_receipt is None and row.get("pid")
                     and not any(k in row for k in ("exit_code", "finished_at", "source_after", "streams")),
                     "current byte-audit runner receipt is not the single pending terminal receipt")
                current_audit_receipt = name
                meta_outcomes[name] = {"class": "current_audit_pending_terminal_receipt",
                                       "detail": "Root runner writes exit status and final streams after verifier exit"}
                continue
            actual_streams = validate_meta_stable_receipt(name, row)
            validate_planned_meta(name, "audit_attempt", row, actual_streams)
            if name == "byte-audit-first.json":
                need(row["exit_code"] == 1 and not output_arg.exists()
                     and b"collection is outside scratch root" in
                     read_regular(scratch / Path(name).with_suffix(".stderr"), CAP),
                     "first failed byte-audit receipt/result differs from its retained preflight failure")
                status = "failed_preflight_no_result"
            else:
                need(output_arg.is_file() and not output_arg.is_symlink(),
                     f"completed byte-audit output is missing or unsafe: {name}")
                audit_result = json_bytes(read_regular(output_arg, 8_000_000), str(output_arg))
                need(type(audit_result.get("ok")) is bool
                     and (row["exit_code"] == 0) is audit_result["ok"],
                     f"byte-audit exit contradicts its retained result: {name}")
                result_alias = f"publication-audit/{output_arg.name}"
                need(result_alias in input_by_name
                     and bytes_by_path[alias_to_object[result_alias]["path"]]
                        == read_regular(output_arg, CAP),
                     f"published prior audit result differs from its actual output: {name}")
                status = "passed" if audit_result["ok"] else "failed"
                meta_outcomes[name] = {"class": status, "exit_code": row["exit_code"],
                                       "error": audit_result.get("error"), "streams": actual_streams}
                continue
            meta_outcomes[name] = {"class": status, "exit_code": row["exit_code"],
                                   "streams": actual_streams}
        checks["publication_and_audit_receipts"] = meta_outcomes
        receipts = qualification_receipts
        for name, row in receipts.items():
            assoc = associations[name]
            need(row.get("runner_sha256") == runner_pin, f"runner pin differs: {name}")
            complete = all(k in row for k in ("exit_code", "finished_at", "source_after", "streams", "pid", "seconds"))
            need(assoc.get("complete_terminal_receipt") is complete, f"receipt completion differs: {name}")
            if complete:
                need(type(row["exit_code"]) is int and type(row["pid"]) is int and row["pid"] > 0
                     and isinstance(row["finished_at"], str) and row["finished_at"]
                     and isinstance(row["seconds"], (int, float)) and row["seconds"] >= 0,
                     f"invalid terminal receipt fields: {name}")
                need(assoc.get("exit_code") == row["exit_code"] and assoc.get("pid_recorded") == row["pid"],
                     f"receipt outcome/pid differs: {name}")
                need(row.get("sources_unchanged") is (row["source_before"] == row["source_after"]),
                     f"source stability flag contradicts receipt pins: {name}")
                need(row.get("binaries_unchanged") is (row.get("binary_before", {}) == row.get("binary_after", {})),
                     f"binary stability flag contradicts receipt pins: {name}")
            else:
                need(assoc.get("exit_code") is None, f"incomplete receipt invents exit status: {name}")
            for field in ("source_before", "source_after"):
                if field not in row:
                    continue
                normalized_source_pins = source_pins_relative_to_repo(row[field], f"{name}/{field}")
                phase_matches = [phase for phase, pins in phase_files.items() if pins == normalized_source_pins]
                need(phase_matches and assoc.get("source_phases", {}).get(field) == phase_matches,
                     f"source phase association differs from actual receipt: {name}/{field}")
            for stream in ("stdout", "stderr"):
                stream_path = scratch / Path(name).with_suffix(f".{stream}")
                raw = read_regular(stream_path, CAP)
                published_alias = f"metadata/{Path(name).stem}.{stream}"
                need(published_alias in input_by_name, f"published stream missing: {published_alias}")
                published = bytes_by_path[alias_to_object[published_alias]["path"]]
                need(raw == published, f"scratch and published stream bytes differ: {name}/{stream}")
                pin = {"bytes": len(raw), "sha256": digest(raw)}
                need(assoc.get("actual_streams", {}).get(stream) == pin, f"stream receipt differs: {name}/{stream}")
                if complete:
                    resolved_streams = {str(Path(k).resolve()): v for k, v in row.get("streams", {}).items()}
                    need(resolved_streams.get(str(stream_path.resolve())) == pin,
                         f"terminal stream pin differs: {name}/{stream}")
            for field in ("binary_before", "binary_after"):
                for path, pin in row.get(field, {}).items():
                    guard = next((g for g in plan.get("runtime_guards", [])
                                  if Path(g["source"]).resolve() == Path(path).resolve()
                                  and g.get("bytes") == pin.get("bytes")
                                  and g.get("sha256") == pin.get("sha256")), None)
                    need(guard is not None, f"receipt binary lacks exact published guard: {name}/{field}/{path}")
        checks["actual_runner_receipts"] = len(receipts)
        checks["complete_runner_receipts"] = sum(1 for a in associations.values()
                                                  if a.get("complete_terminal_receipt") is True)
        checks["incomplete_runner_receipts"] = sum(1 for a in associations.values()
                                                    if a.get("complete_terminal_receipt") is not True)
        checks["runner_receipt_outcomes"] = {
            name: {"complete": assoc.get("complete_terminal_receipt"),
                   "exit_code": assoc.get("exit_code"),
                   "seconds": receipts[name].get("seconds"),
                   "started_at": receipts[name].get("started_at"),
                   "finished_at": receipts[name].get("finished_at"),
                   "sources_unchanged": receipts[name].get("sources_unchanged"),
                   "binaries_unchanged": receipts[name].get("binaries_unchanged"),
                   "source_phases": assoc.get("source_phases", {})}
            for name, assoc in associations.items()}

        # Confirm every listed live runtime guard against current bytes.
        guards = plan.get("runtime_guards", [])
        need(guards and guards == collection.get("runtime_guards"), "runtime guard manifests differ")
        guard_paths = set()
        audit_script_guard = None
        for guard in guards:
            source = Path(guard.get("source", ""))
            need(source.is_absolute() and str(source) not in guard_paths, f"invalid/duplicate guard: {source}")
            guard_paths.add(str(source))
            if source.resolve() == Path(__file__).resolve():
                audit_alias = "verification/verify_evidence.py.txt"
                need(guard.get("kind") == "retained_script" and audit_alias in input_by_name
                     and input_by_name[audit_alias].get("bytes") == guard.get("bytes")
                     and input_by_name[audit_alias].get("sha256") == guard.get("sha256"),
                     "audit script guard does not match its published retained alias")
                current_audit_bytes = read_regular(source, CAP)
                audit_script_guard = {
                    "published_sha256": guard["sha256"],
                    "current_sha256": digest(current_audit_bytes),
                    "matches_live": len(current_audit_bytes) == guard["bytes"]
                    and digest(current_audit_bytes) == guard["sha256"],
                    "scope_note": "The verifier may be revised after collection; its own guard is checked against published bytes and the live delta is reported for Root's post-freeze rebuild.",
                }
            else:
                raw = read_regular(source, CAP)
                need(len(raw) == guard.get("bytes") and digest(raw) == guard.get("sha256"),
                     f"runtime guard differs from live bytes: {source}")
            need(guard.get("kind") in {"final_source", "retained_script", "executed_binary",
                                        "copied_runtime_binary", "native_fixture", "external_tool"},
                 f"unknown runtime guard kind: {source}")
            if guard.get("kind") == "native_fixture":
                need(source == REPO / "native/deadpan-source/tests/fixtures/cfr-bframes.mp4",
                     f"unexpected native fixture guard: {source}")
            if guard.get("kind") == "external_tool":
                need(source == Path("/opt/homebrew/Cellar/ffmpeg/9.0.2_1/bin/ffmpeg")
                     and guard.get("sha256")
                        == "768609fb3f463ff9272bebac61478515f19e43c5343b0c60dde2e6230c40013e",
                     f"unexpected or insufficiently pinned external tool guard: {source}")
        checks["runtime_guard_files_verified"] = len(guards)
        checks["audit_script_guard"] = audit_script_guard

        # Binary build/run links are hash-backed and source-phase-scoped. The
        # verifier deliberately reports associations, not compiler dependency proof.
        manifests = {}
        for name, row in input_by_name.items():
            if re.fullmatch(r"metadata/bin-(debug|release)-[a-z0-9_-]+-binaries\.json", name):
                data = json_bytes(bytes_by_path[alias_to_object[name]["path"]], name)
                need(isinstance(data.get("sources"), dict) and isinstance(data.get("pins"), dict),
                     f"binary manifest lacks source-to-copy mapping: {name}")
                phase_name = name.removeprefix("metadata/bin-").removesuffix("-binaries.json")
                profile, phase = phase_name.split("-", 1)
                entries = {}
                for dest, pin in data["pins"].items():
                    dest_path = Path(dest).resolve()
                    src = data["sources"].get(Path(dest).name)
                    # Manifests from this runner may key sources by executable name.
                    if src is None:
                        src = data["sources"].get(Path(dest).name.removeprefix("deadpan-"))
                    need(src is not None, f"binary manifest source absent: {name}/{dest}")
                    need(isinstance(src, str) and Path(src).is_absolute(),
                         f"binary manifest source path is invalid: {name}/{dest}")
                    source_path = Path(src).resolve()
                    raw = read_regular(dest_path, CAP)
                    need(len(raw) == pin.get("bytes") and digest(raw) == pin.get("sha256"),
                         f"copied runtime binary differs: {dest}")
                    source_matches = False
                    if source_path.exists():
                        source_raw = read_regular(source_path, CAP)
                        source_matches = (len(source_raw) == pin.get("bytes")
                                          and digest(source_raw) == pin.get("sha256"))
                    build_matches = []
                    expected_rel = Path(profile) / Path(dest).name
                    for receipt_name, record in receipts.items():
                        argv = record.get("command", [])
                        if argv[:2] != ["cargo", "build"]:
                            continue
                        cargo_profile = "release" if "--release" in argv else "debug"
                        target_dir = record.get("environment", {}).get("CARGO_TARGET_DIR")
                        if cargo_profile != profile or not target_dir:
                            continue
                        if (Path(target_dir) / expected_rel).resolve() != source_path:
                            continue
                        source_refs = associations[receipt_name].get("source_phases", {}).get("source_before", [])
                        normalized_source = source_pins_relative_to_repo(
                            record.get("source_before", {}), f"{receipt_name}/source_before")
                        exact_phases = [p for p in source_refs
                                        if p in phase_files and normalized_source == phase_files[p]]
                        build_matches.append({"receipt": receipt_name, "phases": exact_phases,
                                              "exit_code": record.get("exit_code")})
                    entries[str(dest_path)] = {"pin": pin, "source": str(source_path),
                                               "source_matches_current_bytes": source_matches,
                                               "build_receipts": build_matches}
                manifests[name] = {"profile": profile, "phase": phase, "entries": entries}
        for name, manifest in manifests.items():
            need(manifest["phase"] in phase_files,
                 f"binary manifest names an absent source phase: {name}")
        checks["binary_manifests"] = {
            name: {"profile": row["profile"], "phase": row["phase"],
                  "source_file_count": len(phase_files[row["phase"]])}
            for name, row in manifests.items()}

        build_receipts = {}
        for manifest_name, manifest in manifests.items():
            for destination, entry in manifest["entries"].items():
                for association in entry["build_receipts"]:
                    if association["exit_code"] == 0 and manifest["phase"] in association["phases"]:
                        build_receipts.setdefault(association["receipt"], set()).add(manifest["phase"])
        checks["binary_build_receipts"] = {
            "associations": {name: sorted(phases) for name, phases in build_receipts.items()},
            "basis": "successful source-phase-matched build receipts plus manifest paths; build receipts do not hash Cargo output artifacts",
        }

        # Every native report must be tied to a completed --ui-check receipt and
        # agrees with the report's own pass/fail/check/skip counts.
        reports = {}
        report_raw_by_logical = {}
        for logical, obj in alias_to_object.items():
            if logical.startswith("native/") and logical.endswith("/report.json"):
                reports[logical] = json_bytes(bytes_by_path[obj["path"]], logical)
                report_raw_by_logical[logical] = bytes_by_path[obj["path"]]
        report_receipt_links = {}
        for name, record in receipts.items():
            command = record.get("command", [])
            if "--ui-check" not in command:
                continue
            if not assoc_complete(associations, name):
                continue
            output_arg = command.index("--output") + 1 if command.count("--output") == 1 else -1
            need(output_arg > 0 and output_arg < len(command), f"native output arg invalid: {name}")
            outdir = Path(command[output_arg])
            if not outdir.is_absolute():
                outdir = Path(record["cwd"]) / outdir
            report_path = outdir / "report.json"
            report_alias = "native/" + report_path.relative_to(scratch).as_posix()
            need(report_alias in reports, f"native report absent from published evidence: {report_alias}")
            report_receipt_links[report_alias] = name
            report = reports[report_alias]
            actual_report = json_bytes(read_regular(report_path, CAP), str(report_path))
            need(actual_report == report, f"published native report differs from scratch: {report_alias}")
            need(command.count("--kestrel-source") == 1, f"Kestrel source arg missing: {name}")
            kestrel_path = Path(command[command.index("--kestrel-source") + 1]).resolve(strict=True)
            need(kestrel_path == KESTREL.resolve(strict=True), f"Kestrel source differs: {name}")
            normalized_binary_before = {str(Path(k).resolve()): v
                                        for k, v in record.get("binary_before", {}).items()}
            need(report.get("metadata", {}).get("binary_sha256")
                 == normalized_binary_before.get(str(Path(command[0]).resolve()), {}).get("sha256"),
                 f"native report binary hash differs from executed app pin: {report_alias}")
            need(report.get("metadata", {}).get("git_revision") == BASE,
                 f"native report base revision differs: {report_alias}")
            failed = False
            scenario_counts = {}
            for scenario in report.get("scenarios", []):
                checks_list = scenario.get("checks", [])
                skipped = scenario.get("skipped", [])
                need(isinstance(checks_list, list) and isinstance(skipped, list),
                     f"native checks/skips malformed: {report_alias}/{scenario.get('name')}")
                findings = scenario.get("findings", [])
                need(isinstance(findings, list), f"native findings malformed: {report_alias}/{scenario.get('name')}")
                skipped = [value for value in skipped if isinstance(value, str) and value.strip()]
                need(len(skipped) == len(scenario.get("skipped", [])),
                     f"native skipped warning is empty or malformed: {report_alias}/{scenario.get('name')}")
                scenario_fail = (scenario.get("failed") is True
                                 or any(c.get("passed") is not True for c in checks_list)
                                 or any(f.get("severity") == "failure" for f in findings))
                need(scenario.get("failed") is scenario_fail,
                     f"native scenario outcome contradicts checks/findings: {report_alias}/{scenario.get('name')}")
                failed |= scenario_fail
                scenario_counts[scenario.get("name")] = {
                    "checks": len(checks_list), "skipped": len(skipped),
                    "skip_warnings": skipped, "failed": scenario_fail,
                    "all_checks_passed": bool(checks_list) and all(c.get("passed") is True for c in checks_list),
                    "check_names": [c.get("name") for c in checks_list],
                }
            need(report.get("failed") is failed, f"native report summary contradicts scenarios: {report_alias}")
            need((record.get("exit_code") != 0) == report.get("failed"),
                 f"native process exit contradicts actual report: {report_alias}")
            report_receipt_links[report_alias] = name
            report["_audit_counts"] = scenario_counts
        need(set(reports) == set(report_receipt_links), "native report is unlinked or a native receipt lacks a report")
        passed_scoped = []
        for logical, report in reports.items():
            for scenario_name, info in report["_audit_counts"].items():
                receipt_name = report_receipt_links[logical]
                phases = associations[receipt_name].get("source_phases", {}).get("source_before", [])
                if (scenario_name == "scoped-follow-creep" and not info["failed"]
                        and info["all_checks_passed"] and final_phase in phases
                        and NEW_PLACEMENT_GUARD in info["check_names"]):
                    passed_scoped.append((logical, info))
        need(passed_scoped, "no all-passing scoped native replay on final source phase retains the new placement guard")
        best = max(passed_scoped, key=lambda pair: pair[1]["checks"])
        final_native_pass_hashes = {digest(report_raw_by_logical[logical]) for logical, _ in passed_scoped}
        kestrel_rows = [r for logical, r in reports.items()
                        if final_phase in associations[report_receipt_links[logical]]
                        .get("source_phases", {}).get("source_before", [])
                        for s in r.get("scenarios", [])
                        if s.get("name") == "kestrel-shortcuts" and s.get("failed") is False
                        and s.get("checks") and all(c.get("passed") is True for c in s["checks"])]
        need(kestrel_rows, "no passing Kestrel shortcut scenario is tied to the final source phase")
        checks["native_reports"] = {
            "reports": len(reports),
            "outcomes": {
                "failed": sum(1 for r in reports.values() if r.get("failed") is True),
                "passed": sum(1 for r in reports.values() if r.get("failed") is False),
            },
            "passing_scoped_check_count": best[1]["checks"],
            "passing_scoped_explicit_skips": best[1]["skipped"],
            "passing_scoped_skip_warnings": best[1]["skip_warnings"],
            "required_final_source_placement_guard": NEW_PLACEMENT_GUARD,
            "passing_kestrel_scenarios": len(kestrel_rows),
            "scenario_outcomes": {
                report_name: report.get("_audit_counts", {}) for report_name, report in reports.items()},
        }

        # Connect the final native invocation to its copied executable and, if
        # that source came from Cargo's target tree, to the successful build
        # receipt for the exact source phase. Older failed reports may use
        # inherited binaries; their executed hashes remain independently pinned.
        native_binary_provenance = {}
        for report_alias, receipt_name in report_receipt_links.items():
            record = receipts[receipt_name]
            command = record["command"]
            executable = str(Path(command[0]).resolve())
            normalized_binary_before = {str(Path(k).resolve()): v
                                        for k, v in record.get("binary_before", {}).items()}
            executed_pin = normalized_binary_before.get(executable)
            need(executed_pin is not None, f"native executable has no before-run pin: {receipt_name}")
            matches = []
            for manifest_name, manifest in manifests.items():
                entry = manifest["entries"].get(executable)
                if entry and entry["pin"] == executed_pin:
                    matches.append((manifest_name, manifest, entry))
            need(matches, f"native executable is absent from copied-binary manifests: {receipt_name}")
            if len(matches) > 1:
                need(all(m[2]["pin"] == matches[0][2]["pin"] for m in matches),
                     f"native executable manifests conflict: {receipt_name}")
            manifest_name, manifest, entry = matches[0]
            # The actual report's binary hash was already matched to the runner
            # receipt above; now require a successful build association for
            # binaries copied from this task's Cargo target tree.
            if Path(entry["source"]).parent.name == manifest["profile"] \
                    and Path(entry["source"]).parent.parent.name == "target":
                successful = [a for a in entry["build_receipts"] if a["exit_code"] == 0
                              and manifest["phase"] in a["phases"]]
                if manifest["phase"] == final_phase:
                    need(successful, f"final-source native binary lacks its exact successful build phase: {receipt_name}")
                    need(entry["source_matches_current_bytes"],
                         f"final-source native copy differs from its current Cargo target binary: {receipt_name}")
                native_binary_provenance[report_alias] = {
                    "manifest": manifest_name, "build_receipts": [x["receipt"] for x in successful],
                    "source_phase": manifest["phase"],
                    "source_file_count": len(phase_files[manifest["phase"]]),
                    "source_matches_current_bytes": entry["source_matches_current_bytes"],
                    "note": ("final copied bytes match the current exact-phase Cargo target output"
                             if successful and manifest["phase"] == final_phase else
                             "historical copied bytes are pinned; no final-build-byte equivalence is asserted"),
                }
            else:
                native_binary_provenance[report_alias] = {
                    "manifest": manifest_name, "build_receipts": [],
                    "source_phase": manifest["phase"],
                    "source_file_count": len(phase_files[manifest["phase"]]),
                    "source_matches_current_bytes": entry["source_matches_current_bytes"],
                    "note": "copied executable bytes are directly hash-linked; build source is inherited outside this task's Cargo target tree",
                }
        checks["native_binary_provenance"] = native_binary_provenance

        # This narrow release claim needs a real release build receipt for the
        # final source phase, its matching copied app bytes, and a passing
        # release-profile native replay of the same scoped scenario.
        release_candidates = []
        for report_alias, receipt_name in report_receipt_links.items():
            report = reports[report_alias]
            if report.get("metadata", {}).get("build_profile") != "release":
                continue
            run_record = receipts[receipt_name]
            scoped = report.get("_audit_counts", {}).get("scoped-follow-creep", {})
            source_refs = associations[receipt_name].get("source_phases", {}).get("source_before", [])
            if (run_record.get("exit_code") != 0 or report.get("failed") is not False
                    or final_phase not in source_refs or not scoped
                    or scoped.get("failed") is not False or not scoped.get("all_checks_passed")
                    or NEW_PLACEMENT_GUARD not in scoped.get("check_names", [])):
                continue
            normalized = {str(Path(k).resolve()): v for k, v in run_record.get("binary_before", {}).items()}
            executable = str(Path(run_record["command"][0]).resolve())
            executed_pin = normalized.get(executable)
            if not executed_pin or report.get("metadata", {}).get("binary_sha256") != executed_pin.get("sha256"):
                continue
            for manifest_name, manifest in manifests.items():
                if manifest.get("profile") != "release" or manifest.get("phase") != final_phase:
                    continue
                entry = manifest["entries"].get(executable)
                if not entry or entry.get("pin") != executed_pin or not entry.get("source_matches_current_bytes"):
                    continue
                build_matches = [row for row in entry.get("build_receipts", [])
                                 if row.get("exit_code") == 0 and final_phase in row.get("phases", [])]
                valid_builds = []
                for association in build_matches:
                    build = receipts[association["receipt"]]
                    argv = build.get("command", [])
                    if (argv[:2] == ["cargo", "build"] and "--release" in argv
                            and source_pins_relative_to_repo(build.get("source_before", {}),
                                                             f"{association['receipt']}/source_before") == final_pins
                            and source_pins_relative_to_repo(build.get("source_after", {}),
                                                             f"{association['receipt']}/source_after") == final_pins
                            and build.get("sources_unchanged") is True):
                        valid_builds.append(association["receipt"])
                kestrel_in_same_report = any(s.get("name") == "kestrel-shortcuts"
                                             and s.get("failed") is False and s.get("checks")
                                             and all(c.get("passed") is True for c in s["checks"])
                                             for s in report.get("scenarios", []))
                if valid_builds and kestrel_in_same_report:
                    release_candidates.append({
                        "status": "release_build_and_scoped_replay_verified_within_scope",
                        "build_receipts": valid_builds,
                        "manifest": manifest_name,
                        "native_report": report_alias,
                        "native_receipt": receipt_name,
                        "source_phase": final_phase,
                        "source_file_count": len(final_pins),
                        "binary_sha256": executed_pin["sha256"],
                        "full_product_release_qualified": False,
                        "scope_note": "No packaging, signing, clean-Mac, or full-product release claim.",
                    })
        if args.require_release:
            need(release_candidates, "required final-source release build and passing release-profile scoped replay proof is absent")
        release_status = release_candidates[0] if release_candidates else {
            "status": "pending", "claimed": False, "full_product_release_qualified": False}

        # Optional media qualification: require the actual driver receipt and
        # independently reconcile its retained package inventories, database
        # history, helper/report inputs, verification reports and movie bytes.
        media_candidates = [(name, obj) for name, obj in alias_to_object.items()
                            if Path(name).name == "result.json"]
        media = {"status": "pending"}
        if media_candidates:
            qualifying = []
            for logical, obj in media_candidates:
                candidate = json_bytes(bytes_by_path[obj["path"]], logical)
                if candidate.get("frames_checked") is not None:
                    qualifying.append((logical, candidate))
            need(len(qualifying) == 1, "ambiguous retained media result documents")
            logical, proof = qualifying[0]
            media_obj = next(obj for candidate_name, obj in media_candidates if candidate_name == logical)
            input_row = input_by_name[logical]
            result_source = Path(input_row.get("source", "")).resolve(strict=True)
            need(path_inside(result_source, scratch), "media result source is outside scratch")
            actual_result = read_regular(result_source, CAP)
            need(actual_result == bytes_by_path[media_obj["path"]], "published media result differs from retained scratch result")
            need(proof.get("frames_checked") == 240 and proof.get("canonical_stereo_sample_frames") == 384384,
                 "media result does not prove 240 pictures and 384384 canonical audio sample frames")
            need(proof.get("live_frames") == 216 and proof.get("captured_gap_frames") == 24
                 and proof.get("owner_range") == [164, 200]
                 and proof.get("authored_window") == [170, 188]
                 and proof.get("expected_reference_pixel_change_range") == [170, 200],
                 "media result scoped range, live frame, or captured-gap fields differ")
            need(proof.get("authored_limited_and_limiter_gain_bits_equal") is True
                 and type(proof.get("shared_reference_pixels_changed_frames")) is int
                 and proof["shared_reference_pixels_changed_frames"] == 30
                 and type(proof.get("unchanged_frames_with_codec_pixel_residual")) is int
                 and proof["unchanged_frames_with_codec_pixel_residual"] >= 0,
                 "media result canonical PCM or pixel-comparison fields are invalid")
            evidence_scope = proof.get("evidence_scope")
            need(isinstance(evidence_scope, dict)
                 and isinstance(evidence_scope.get("native_exclusions"), list)
                 and all(isinstance(x, str) and x for x in evidence_scope["native_exclusions"])
                 and all(isinstance(evidence_scope.get(k), str) and evidence_scope[k]
                         for k in ("oracle", "canonical", "pixels", "history", "not_claimed"))
                 and "independent DSP or renderer" in evidence_scope["not_claimed"],
                 "media evidence scope or qualification exclusions are missing")

            def retained_media_file(basename):
                rows = [(alias, obj) for alias, obj in alias_to_object.items()
                        if Path(alias).name == basename]
                need(len(rows) == 1, f"media proof must retain exactly one {basename}")
                alias, object_row = rows[0]
                source = Path(input_by_name[alias].get("source", "")).resolve(strict=True)
                need(path_inside(source, scratch) and path_inside(source, result_source.parent),
                     f"media proof file is outside its recorded output directory: {alias}")
                raw = read_regular(source, CAP)
                published = bytes_by_path[object_row["path"]]
                need(raw == published, f"published media proof differs from its scratch file: {alias}")
                return alias, source, raw

            before_alias, before_path, before_raw = retained_media_file("source-package-before.json")
            after_alias, after_path, after_raw = retained_media_file("source-package-after.json")
            comparison_alias, comparison_path, comparison_raw = retained_media_file("source-package-comparison.json")
            before_inventory = json_bytes(before_raw, before_alias)
            after_inventory = json_bytes(after_raw, after_alias)
            comparison = json_bytes(comparison_raw, comparison_alias)

            def validate_package_inventory(inventory, phase, label):
                need(inventory.get("complete") is True and inventory.get("phase") == phase,
                     f"source package inventory is incomplete or mislabeled: {label}")
                need(inventory.get("exclusion_rule") ==
                     "only root project.sqlite-shm; bytes observed separately; no persistence claim",
                     f"source package exclusion rule differs: {label}")
                files = inventory.get("persistent_files")
                directories = inventory.get("directories")
                transient = inventory.get("excluded_transient_files")
                need(isinstance(files, list) and isinstance(directories, list) and isinstance(transient, list),
                     f"source package inventory arrays are missing: {label}")
                names = []
                for row in files:
                    rel = row.get("relative_path")
                    need(isinstance(rel, str) and not Path(rel).is_absolute()
                         and ".." not in Path(rel).parts and "." not in Path(rel).parts
                         and type(row.get("bytes")) is int and row["bytes"] >= 0
                         and re.fullmatch(r"[0-9a-f]{64}", row.get("sha256", "")) is not None,
                         f"invalid persistent source file pin: {label}/{rel}")
                    names.append(rel)
                need(names == sorted(set(names)), f"persistent source file inventory is unsorted or duplicated: {label}")
                need(directories == sorted(set(directories)),
                     f"source package directory inventory is unsorted or duplicated: {label}")
                for rel in directories:
                    need(isinstance(rel, str) and not Path(rel).is_absolute()
                         and ".." not in Path(rel).parts and "." not in Path(rel).parts,
                         f"invalid source package directory: {label}/{rel}")
                need(len(transient) <= 1 and all(row.get("relative_path") == "project.sqlite-shm"
                                                and type(row.get("bytes")) is int and row["bytes"] >= 0
                                                and re.fullmatch(r"[0-9a-f]{64}", row.get("sha256", ""))
                                                and row.get("reason") ==
                                                "transient SQLite shared-memory bytes; no persistence assertion"
                                                for row in transient),
                     f"source package transient-file inventory is malformed: {label}")
                calculated = canonical_digest({"directories": directories, "files": files})
                need(inventory.get("persistent_inventory_sha256") == calculated,
                     f"source package inventory digest differs: {label}")

            validate_package_inventory(before_inventory, "before any source read/copy", "before")
            validate_package_inventory(after_inventory, "after final source read/copy", "after")
            source_project = Path(before_inventory.get("source_project", "")).resolve(strict=True)
            need(source_project == Path(after_inventory.get("source_project", "")).resolve(strict=True)
                 and path_inside(source_project, scratch), "source project path differs or is outside scratch")
            need(source_project == Path(proof.get("source_project", "")).resolve(strict=True),
                 "media result source project path differs from package inventories")
            before_files = before_inventory["persistent_files"]
            after_files = after_inventory["persistent_files"]
            before_map = {row["relative_path"]: row for row in before_files}
            after_map = {row["relative_path"]: row for row in after_files}
            expected_changed = [{"before": before_map[path], "after": after_map[path]}
                                for path in sorted(set(before_map) & set(after_map))
                                if before_map[path] != after_map[path]]
            expected_comparison = {
                "persistent_path_bytes_and_sha256_equal": before_map == after_map,
                "directories_equal": before_inventory["directories"] == after_inventory["directories"],
                "added_persistent_paths": sorted(set(after_map) - set(before_map)),
                "removed_persistent_paths": sorted(set(before_map) - set(after_map)),
                "changed_persistent_files": expected_changed,
                "before_persistent_inventory_sha256": before_inventory["persistent_inventory_sha256"],
                "after_persistent_inventory_sha256": after_inventory["persistent_inventory_sha256"],
                "excluded_transient_before": before_inventory["excluded_transient_files"],
                "excluded_transient_after": after_inventory["excluded_transient_files"],
                "transient_exclusion_claim": "reported path/size/hash observations only; equality/persistence not asserted",
            }
            need(comparison == expected_comparison,
                 "retained source-package comparison does not recompute from its before/after inventories")
            need(comparison == proof.get("source_package_path_bytes_and_hash_preservation"),
                 "media result source-package fields differ from retained comparison file")
            need(comparison["persistent_path_bytes_and_sha256_equal"] is True
                 and comparison["directories_equal"] is True
                 and not comparison["added_persistent_paths"] and not comparison["removed_persistent_paths"]
                 and not comparison["changed_persistent_files"],
                 "source package persistent files or directories changed")
            current_snapshot = package_snapshot(source_project)
            need(current_snapshot["persistent_files"] == after_files
                 and current_snapshot["directories"] == after_inventory["directories"],
                 "live source package no longer matches the retained after-inventory")
            need(current_snapshot["persistent_inventory_sha256"] == after_inventory["persistent_inventory_sha256"],
                 "live source package inventory hash differs from retained after-inventory")

            history = proof.get("source_history_unchanged_and_portable_equal")
            need(isinstance(history, dict)
                 and isinstance(history.get("counts"), dict)
                 and all(type(history["counts"].get(k)) is int and history["counts"][k] > 0
                         for k in ("revisions", "history", "state"))
                 and type(history["counts"].get("redo")) is int and history["counts"]["redo"] >= 0
                 and re.fullmatch(r"[0-9a-f]{64}", history.get("sha256", ""))
                 and isinstance(history.get("state"), list)
                 and isinstance(history.get("revision_kinds"), dict)
                 and all(type(history["revision_kinds"].get(k)) is int and history["revision_kinds"][k] > 0
                         for k in ("initial", "edit", "undo", "redo")),
                 "portable history proof is missing required retained fields")
            live_history = project_history(source_project)
            need(history == live_history,
                 "media result history fields do not match the independently read source SQLite history")
            report_sha = proof.get("native_report_sha256")
            need(re.fullmatch(r"[0-9a-f]{64}", report_sha or "") is not None
                 and report_sha in final_native_pass_hashes,
                 "media result is not tied to a retained final-source passing native scoped report")
            for key in ("native_movie_sha256", "fixed_control_movie_sha256"):
                need(re.fullmatch(r"[0-9a-f]{64}", proof.get(key, "")) is not None,
                     f"media result lacks a valid {key}")
            pins = proof.get("pins")
            need(isinstance(pins, dict) and pins, "media result input pins are missing")
            normalized_media_pins = {str(Path(pin_path).resolve()): expected
                                     for pin_path, expected in pins.items()}
            need(len(normalized_media_pins) == len(pins)
                 and all(isinstance(pin_path, str) and Path(pin_path).is_absolute()
                         and re.fullmatch(r"[0-9a-f]{64}", expected or "") is not None
                         for pin_path, expected in pins.items()),
                 "media result input pin map is malformed or has duplicate paths")
            for pin_path, expected in pins.items():
                live = read_regular(Path(pin_path), CAP)
                need(isinstance(expected, str) and digest(live) == expected,
                     f"media run input changed since retained result: {pin_path}")
            approved_ffmpeg = Path("/opt/homebrew/Cellar/ffmpeg/9.0.2_1/bin/ffmpeg")
            need(str(approved_ffmpeg) in normalized_media_pins
                 and normalized_media_pins[str(approved_ffmpeg)]
                    == "768609fb3f463ff9272bebac61478515f19e43c5343b0c60dde2e6230c40013e",
                 "media proof does not pin the exact approved FFmpeg runtime")
            ffmpeg_guards = [g for g in plan.get("runtime_guards", [])
                             if Path(g.get("source", "")).as_posix() == approved_ffmpeg.as_posix()]
            need(len(ffmpeg_guards) == 1 and ffmpeg_guards[0].get("kind") == "executed_binary"
                 and ffmpeg_guards[0].get("sha256")
                    == "768609fb3f463ff9272bebac61478515f19e43c5343b0c60dde2e6230c40013e",
                 "external FFmpeg runtime guard is absent, ambiguous, or broader than its exact pin")

            def media_option(command, option):
                need(command.count(option) == 1 and command.index(option) + 1 < len(command),
                     f"media command lacks a unique {option} argument")
                value = Path(command[command.index(option) + 1])
                if not value.is_absolute():
                    value = Path(receipt["cwd"]) / value
                return value.resolve(strict=True)

            media_script = Path(input_by_name["metadata/media.py.txt"]["source"]).resolve(strict=True)
            media_receipts = []
            for name, candidate in receipts.items():
                command = candidate.get("command", [])
                script_args = []
                for item in command:
                    try:
                        if Path(item).name == "media.py":
                            script_args.append(Path(item))
                    except (TypeError, ValueError):
                        continue
                if len(script_args) == 1:
                    script_arg = script_args[0]
                    if not script_arg.is_absolute():
                        script_arg = Path(candidate["cwd"]) / script_arg
                    if script_arg.resolve() == media_script:
                        media_receipts.append((name, candidate))
            matching_media_receipts = []
            for name, candidate in media_receipts:
                command = candidate.get("command", [])
                if command.count("--output") != 1 or command.index("--output") + 1 >= len(command):
                    continue
                candidate_output = Path(command[command.index("--output") + 1])
                if not candidate_output.is_absolute():
                    candidate_output = Path(candidate["cwd"]) / candidate_output
                if candidate_output.resolve() == result_source.parent:
                    matching_media_receipts.append((name, candidate))
            need(len(matching_media_receipts) == 1,
                 "media result lacks exactly one root-runner receipt for its output directory")
            media_receipt_name, receipt = matching_media_receipts[0]
            need(assoc_complete(associations, media_receipt_name) and receipt.get("exit_code") == 0,
                 "actual media.py runner receipt is incomplete or unsuccessful")
            need(receipt.get("sources_unchanged") is True
                 and receipt.get("source_before") == receipt.get("source_after"),
                 "actual media.py run did not retain stable source bytes")
            need(final_phase in associations[media_receipt_name].get("source_phases", {}).get("source_before", []),
                 "media.py ran against a different source phase than the final native proof")
            command = receipt["command"]
            output_dir = media_option(command, "--output")
            need(output_dir == result_source.parent, "media result is not in the actual media command output directory")
            project_arg = media_option(command, "--project")
            need(project_arg == source_project, "media command project differs from inventoried source project")
            report_arg = media_option(command, "--native-report")
            need(digest(read_regular(report_arg, CAP)) == report_sha,
                 "media command native report does not match retained final native proof")
            bin_dir = media_option(command, "--bin")
            helper_arg = media_option(command, "--helper")
            ffmpeg_arg = media_option(command, "--ffmpeg")
            need(helper_arg.name == "media-helper", "media helper argument is unexpected")
            need(ffmpeg_arg == approved_ffmpeg,
                 "media command did not use the exact pinned external FFmpeg binary")
            required_pins = {str((bin_dir / "deadpan-app").resolve(strict=True)),
                             str((bin_dir / "deadpan-cli").resolve(strict=True)),
                             str(helper_arg), str(ffmpeg_arg), str(media_script)}
            need(required_pins <= set(normalized_media_pins),
                 "media result input pins omit its actual binaries, helper, FFmpeg or driver")
            for basename in ("native-report.json", "helper-build.json", "helper-result.json",
                             "native-ranged.mp4", "captured-fixed-control.mp4",
                             "native-ranged-verification.json", "captured-fixed-control-verification.json",
                             "fixed-reference-cross-verification.json"):
                retained_media_file(basename)
            _, retained_report_path, retained_report_raw = retained_media_file("native-report.json")
            need(retained_report_raw == read_regular(report_arg, CAP),
                 "retained media native report differs from command input")
            helper_build_alias, helper_build_path, helper_build_raw = retained_media_file("helper-build.json")
            helper_build = json_bytes(helper_build_raw, helper_build_alias)
            need(helper_build == json_bytes(read_regular(helper_arg.with_suffix(".build.json"), CAP), "helper build receipt"),
                 "retained helper build receipt differs from the actual helper receipt")
            normalized_helper_inputs = {str(Path(k).resolve()): v for k, v in helper_build.get("inputs", {}).items()}
            need(helper_build.get("binary_sha256") == normalized_media_pins[str(helper_arg)],
                 "helper build receipt does not pin the executed media helper")
            helper_exec_alias, _, helper_exec_raw = retained_media_file("helper-executables.json")
            helper_exec = json_bytes(helper_exec_raw, helper_exec_alias)
            need(set(helper_exec) == {"deadpan-app", "deadpan-cli"},
                 "helper executable provenance does not identify exactly the app and CLI")
            for executable_name, executable in helper_exec.items():
                build_input = Path(executable.get("build_input", "")).resolve(strict=True)
                executed_copy = Path(executable.get("executed_copy", "")).resolve(strict=True)
                expected_copy = (bin_dir / executable_name).resolve(strict=True)
                need(executed_copy == expected_copy and executable_name == Path(executed_copy).name,
                     f"helper executable copy path/name does not match the actual media command: {executable_name}")
                input_pin = normalized_helper_inputs.get(str(build_input))
                build_input_raw = read_regular(build_input, CAP)
                copy_pin = normalized_media_pins.get(str(executed_copy))
                need(isinstance(input_pin, str) and input_pin == executable.get("sha256")
                     and input_pin == digest(build_input_raw) and input_pin == copy_pin,
                     f"helper executable build input is not hash-linked to its executed copy: {executable_name}")
            helper_result_alias, _, helper_result_raw = retained_media_file("helper-result.json")
            helper_result = json_bytes(helper_result_raw, helper_result_alias)
            need(helper_result.get("read_only") is True and helper_result.get("canonical_pcm_bits_equal") is True,
                 "retained media helper result lacks successful read-only PCM proof")

            def check_movie_verification(basename, expected_revision):
                alias, _, raw = retained_media_file(basename)
                doc = json_bytes(raw, alias)
                need(doc.get("passed") is True and doc.get("failures") == []
                     and doc.get("revision_id") == expected_revision and doc.get("range") == [0, 240]
                     and doc.get("frame_rate") == [30000, 1001]
                     and doc.get("canvas") == [320, 180] and doc.get("raster") == [320, 180],
                     f"retained media export verification failed: {basename}")
                movie = doc.get("movie", {})
                need(movie.get("video_frames") == movie.get("expected_video_frames") == 240
                     and movie.get("expected_audio_samples") == 384384
                     and movie.get("audio", {}).get("presented_end") == 384384
                     and movie.get("audio", {}).get("gaps") == [],
                     f"retained movie verification clocks differ: {basename}")
                summary = doc.get("summary", {})
                pictures = doc.get("pictures", [])
                need(summary.get("pictures_checked") == 240 and summary.get("pictures_failed") == 0
                     and len(pictures) == 240
                     and all(row.get("passed") is True and row.get("output_frame") == i
                             and row.get("project_frame") == i for i, row in enumerate(pictures)),
                     f"retained movie per-picture proof is incomplete: {basename}")
                audio = doc.get("audio", [])
                need(len(audio) == 1 and audio[0].get("window") == [0, 384384]
                     and audio[0].get("project_samples") == [0, 384384]
                     and audio[0].get("passed") is True and audio[0].get("uncovered_samples") == 0
                     and audio[0].get("offset_status") == "verified_zero"
                     and audio[0].get("measured_offset_samples") == 0,
                     f"retained movie audio proof is incomplete: {basename}")

            check_movie_verification("native-ranged-verification.json", proof.get("native_revision"))
            check_movie_verification("captured-fixed-control-verification.json", proof.get("control_undo_revision"))
            cross_alias, _, cross_raw = retained_media_file("fixed-reference-cross-verification.json")
            cross = json_bytes(cross_raw, cross_alias)
            need(cross.get("passed") is True and cross.get("failures") == []
                 and cross.get("summary", {}).get("pictures_checked") == 210,
                 "retained fixed-reference cross-verification proof is incomplete")
            ranged_movie_alias, _, ranged_movie_raw = retained_media_file("native-ranged.mp4")
            control_movie_alias, _, control_movie_raw = retained_media_file("captured-fixed-control.mp4")
            need(digest(ranged_movie_raw) == proof.get("native_movie_sha256")
                 and digest(control_movie_raw) == proof.get("fixed_control_movie_sha256"),
                 "retained media movie bytes differ from result hashes")

            media = {"status": "verified_from_retained_result", "logical": logical,
                     "frames_checked": 240, "canonical_stereo_sample_frames": 384384,
                     "portable_history": "asserted_by_pinned_media_driver_exit0",
                     "source_history_independently_verified": True,
                     "native_report_sha256": report_sha,
                     "source_package_preserved": True, "source_project_rechecked": True,
                     "media_receipt": media_receipt_name, "retained_input_pins": len(pins),
                     "retained_movie_hashes_verified": True}
        if args.require_media:
            need(media.get("status") == "verified_from_retained_result",
                 "required retained media proof is absent")
        checks["media_qualification"] = media
        checks["release_qualification"] = release_status

        result.update({"ok": True, "collection": str(root), "checks": checks})
    except Exception as exc:
        result.update({"ok": False, "error": str(exc), "checks": checks})
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps(result, sort_keys=True))
    return 0 if result.get("ok") else 1


def assoc_complete(associations, name):
    return associations.get(name, {}).get("complete_terminal_receipt") is True


if __name__ == "__main__":
    sys.exit(main())
