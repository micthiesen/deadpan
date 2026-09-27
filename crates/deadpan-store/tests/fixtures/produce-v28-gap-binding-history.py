"""Produce retained gap-binding DB28 history with the preserved core22 CLI.

Usage: python3 produce-v28-gap-binding-history.py OLD_CLI SCRATCH_DIRECTORY
The scratch directory must not exist. The checked-in initial seed is made from
the v28 gap-branch fixture's final snapshot with core capture_unbound_audio_bindings;
see the provenance report. This script seeds only that first revision. Every
subsequent request, revision and patch is written by the old CLI.
"""

from pathlib import Path
import argparse
import copy
import hashlib
import json
import sqlite3
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("scratch", type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
binary_sha = "d572c2b6e19f0d31e04912bc375b2deac7536225c9755381a508270d9414a475"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
base = Path(__file__).resolve().parent
seed_path = base / "v28-gap-binding-seed.json"
seed = json.loads(seed_path.read_text())
assert seed["schema_version"] == 22
assert len(seed["audio_bindings"]["gap_bindings"]) == 1
args.scratch.mkdir()
project = args.scratch / "gap-binding-history.deadpan"
log = []


def run(*arguments):
    invocation = [str(binary), *map(str, arguments)]
    result = subprocess.run(invocation, capture_output=True, text=True)
    log.append(dict(command=invocation, returncode=result.returncode,
                    stdout=result.stdout, stderr=result.stderr))
    (args.scratch / "commands.json").write_text(json.dumps(log, indent=2) + "\n")
    if result.returncode:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)


doctor = run("doctor")
assert (doctor["document_schema"], doctor["database_schema"]) == (22, 28)
run("project", "create", project, "--fps", "30000/1001", "--size", "16x16")
created = run("project", "dump", project, "--json")
initial = copy.deepcopy(seed)
initial["project_id"] = created["project_id"]
initial["revision_id"] = created["revision_id"]
initial_json = json.dumps(initial, separators=(",", ":"))
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("select count(*) from revisions").fetchone() == (1,)
    assert database.execute("select count(*) from history").fetchone() == (0,)
    database.execute("update revisions set document=? where kind='initial'", (initial_json,))
run("project", "validate", project)
assert run("project", "dump", project, "--json") == initial


def command(revision, payload):
    document = run("project", "dump", project, "--json")
    request = dict(protocol=1, project_id=document["project_id"],
                   expected_revision=document["revision_id"],
                   new_revision=revision, command=payload)
    path = args.scratch / (revision + ".json")
    path.write_text(json.dumps(request, indent=2) + "\n")
    return run("command", project, "--json", path)


gap = initial["nodes"]["old-gap-owner"]["kind"]["gap"]
command("binding-gap-growth", dict(command="set_repeat", node="old-gap-owner",
                               plays=4, gap=gap))
command("old-gap-rename", dict(command="rename", node=initial["root"],
                               label="Retained core 22 gap binding"))
for verb in ("undo", "redo", "undo"):
    run("project", verb, project, "--expected",
        run("project", "dump", project, "--json")["revision_id"])
run("project", "validate", project)
final = run("project", "dump", project, "--json")
assert len(final["audio_bindings"]["gap_bindings"]) == 1
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (28,)
    assert all(json.loads(row[0])["schema_version"] == 22
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=28;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v28-gap-binding-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history")}
report = dict(binary_sha256=binary_sha,
              seed_sha256=hashlib.sha256(seed_path.read_bytes()).hexdigest(),
              seed_source_fixture="v28-gap-branch-history.sql",
              seed_source_fixture_sha256=hashlib.sha256((base / "v28-gap-branch-history.sql").read_bytes()).hexdigest(),
              seed_source_revision="4399518a-df71-484e-b6e9-39501c5193ca",
              seed_capture_timing={"allocation": "schema28-gap-capture", "ordinal": 0},
              initial_document_sha256=hashlib.sha256(initial_json.encode()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=22,
              database_schema=28, producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=str(args.scratch / "commands.json"),
              producer_log_sha256=hashlib.sha256((args.scratch / "commands.json").read_bytes()).hexdigest(),
              doctor=doctor,
              method="For the initial seed, migrate v28-gap-branch-history.sql to core23/db29, read its head revision, run capture_unbound_audio_bindings with timing (schema28-gap-capture,0), replace only audio_bindings in the serialized document, set schema_version=22, and serialize compact JSON. The preserved core22 CLI created a fresh DB28 package, validated that initial intent, authored SetRepeat and Rename, then undo/redo/undo. The producer script changed no history rows or later snapshots.")
(base / "v28-gap-binding-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
