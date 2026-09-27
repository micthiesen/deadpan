"""Produce genuine DB29/core23 gap and InsertTime history with the preserved CLI.

Usage: python3 produce-v29-composite-insert-history.py OLD_CLI SCRATCH_DIRECTORY
The scratch directory must not exist. The old binary migrates the unmodified
DB28 fixture and authors every new command, snapshot, patch and navigation row.
No wire version or historical content is rewritten by this producer.
"""

from pathlib import Path
import argparse
import hashlib
import json
import sqlite3
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("scratch", type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
binary_sha = "7a02df4266800123c60440cb8f78450bb5b46a9c7e5608f21a7b14cfc29645fd"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
base = Path(__file__).resolve().parent
input_fixture = base / "v28-gap-binding-history.sql"
args.scratch.mkdir()
project = args.scratch / "composite-insert-history.deadpan"
project.mkdir()
(project / "Snapshots").mkdir()
with sqlite3.connect(project / "project.sqlite") as database:
    database.executescript(input_fixture.read_text())
log = []
log_path = base / "v29-composite-insert-history.commands.json"


def run(*arguments):
    invocation = [str(binary), *map(str, arguments)]
    result = subprocess.run(invocation, capture_output=True, text=True)
    record = dict(command=invocation, returncode=result.returncode,
                  stdout_sha256=hashlib.sha256(result.stdout.encode()).hexdigest(),
                  stderr=result.stderr)
    if arguments[0] == "doctor" or arguments[1] in ("validate", "migrate"):
        record["stdout"] = result.stdout
    log.append(record)
    log_path.write_text(json.dumps(log, indent=2) + "\n")
    if result.returncode:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)


def command(revision, payload):
    document = run("project", "dump", project, "--json")
    request = dict(protocol=1, project_id=document["project_id"],
                   expected_revision=document["revision_id"],
                   new_revision=revision, command=payload)
    path = args.scratch / (revision + ".json")
    path.write_text(json.dumps(request, indent=2) + "\n")
    return run("command", project, "--json", path)


def navigate(verb):
    run("project", verb, project, "--expected",
        run("project", "dump", project, "--json")["revision_id"])


doctor = run("doctor")
assert (doctor["document_schema"], doctor["database_schema"]) == (23, 29)
run("project", "migrate", project)
document = run("project", "dump", project, "--json")
repeat = "old-gap-owner"
first = dict(allocation="old-configured-gap", ordinal=0)
second = dict(allocation="old-gap-growth", ordinal=0)
last = dict(allocation="binding-gap-growth", ordinal=0)
hold = dict(duration=2, video=dict(type="background"), audio=dict(type="silence"))
# The Repeat is entirely before this seam. Every shifted suffix beat was
# admitted by core23: a Hold, Source, or transparent physical partition.
command("core23-ordinary-insert", dict(command="insert_time", at=18, hold=hold,
        id="core23-pause", identities=dict(nodes=[]),
        timing=dict(allocation="core23-ordinary-insert", ordinal=0)))
command("core23-isolate-gap", dict(command="isolate_gap", node=repeat,
        iteration=first, id="core23-detached-gap",
        timing=dict(allocation="core23-isolate-gap", ordinal=0)))
command("core23-gap-branch", dict(command="set_gap_override", node=repeat,
        iteration=second, subtree=dict(root="core23-gap-branch", overrides={},
            nodes={"core23-gap-branch": dict(label="Old authored branch",
                kind=dict(type="hold", recipe=hold))})))
command("core23-occurrence-gap", dict(command="edit_occurrence",
        instance=dict(node=repeat, repeats=[]), identities=dict(nodes=[], marks=[]),
        edit=dict(type="set_gap_override", iteration=last,
            subtree=dict(root="core23-dormant-gap", overrides={},
                nodes={"core23-dormant-gap": dict(label="Dormant last gap",
                    kind=dict(type="hold", recipe=hold))}))))
command("core23-clear-gap", dict(command="clear_gap_override", node=repeat,
        iteration=first))
navigate("undo")
command("core23-gap-rename", dict(command="rename", node="core23-detached-gap",
        label="Renamed detached gap"))
for verb in ("undo", "redo", "undo"):
    navigate(verb)
run("project", "validate", project)
final = run("project", "dump", project, "--json")
assert final["nodes"]["core23-detached-gap"]["label"] != "Renamed detached gap"
assert final["audio_bindings"]["bindings"]["core23-detached-gap"]["lattice"]["reference"]["recipe"] == "repeat_gap"
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (29,)
    assert all(json.loads(row[0])["schema_version"] == 23
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=29;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v29-composite-insert-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history", "redo")}
report = dict(binary_sha256=binary_sha,
              input_fixture=input_fixture.name,
              input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=23, database_schema=29,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=log_path.name,
              producer_log_sha256=hashlib.sha256(log_path.read_bytes()).hexdigest(),
              doctor=doctor,
              method="Preserved core23/database29 CLI migrated the unmodified database28 gap binding history, then authored ordinary InsertTime, IsolateGap, direct and occurrence SetGapOverride, ClearGapOverride and undo, Rename, undo/redo/undo with pending redo. No schema relabeling or snapshot/request/patch rewriting.")
(base / "v29-composite-insert-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
