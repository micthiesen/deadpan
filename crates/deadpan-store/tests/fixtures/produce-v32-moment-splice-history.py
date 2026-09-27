"""Produce genuine DB32/core26 nested Sequence InsertTime history.

Usage: python3 produce-v32-moment-splice-history.py OLD_CLI SCRATCH_DIRECTORY
The preserved binary migrates the unchanged DB31 fixture and authors all added
commands, snapshots, patches and history navigation. No wire content is rewritten.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import sqlite3
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("scratch", type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
binary_sha = "2dbf31b498bae2805cd417468e1b0fa542d02fd019232156adefdc68958ce547"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
environment = dict(os.environ, PATH=str(binary.parent) + os.pathsep + os.environ["PATH"])
base = Path(__file__).resolve().parent
input_fixture = base / "v31-sequence-insert-history.sql"
args.scratch.mkdir()
project = args.scratch / "moment-splice-history.deadpan"
project.mkdir()
(project / "Snapshots").mkdir()
with sqlite3.connect(project / "project.sqlite") as database:
    database.executescript(input_fixture.read_text())
log = []
log_path = base / "v32-moment-splice-history.commands.json"


def run(*arguments):
    invocation = [binary.name, *map(str, arguments)]
    result = subprocess.run(invocation, capture_output=True, text=True, env=environment)
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


doctor = run("doctor")
assert (doctor["document_schema"], doctor["database_schema"]) == (26, 32)
run("project", "migrate", project)
hold = dict(duration=2, video=dict(type="background"), audio=dict(type="silence"))
document = run("project", "dump", project, "--json")
command("core26-add-group", dict(command="insert", parent=document["root"], index=0,
        subtree=dict(root="core26-outer", nodes={
            "core26-outer": dict(label="Outer", kind=dict(type="sequence", children=["core26-inner"])),
            "core26-inner": dict(label="Inner", kind=dict(type="sequence", children=["core26-first", "core26-second"])),
            "core26-first": dict(label="First", kind=dict(type="hold", recipe=dict(hold, duration=4))),
            "core26-second": dict(label="Second", kind=dict(type="hold", recipe=dict(hold, duration=3))),
        })))
command("core26-nested-interior", dict(command="insert_time", at=2, hold=hold,
        id="core26-pause", identities=dict(nodes=["core26-left", "core26-right", "core26-owner"]),
        timing=dict(allocation="core26-nested-interior", ordinal=0)))
command("core26-nested-seam", dict(command="insert_time", at=6,
        hold=dict(hold, duration=3), id="core26-second-pause", identities=dict(nodes=[]),
        timing=dict(allocation="core26-nested-seam", ordinal=0)))
for verb in ("undo", "redo", "undo"):
    run("project", verb, project, "--expected",
        run("project", "dump", project, "--json")["revision_id"])
run("project", "validate", project)
final = run("project", "dump", project, "--json")
assert "core26-pause" in final["nodes"]
assert "core26-second-pause" not in final["nodes"]
assert final["audio_bindings"]["bindings"]["core23-detached-gap"]["lattice"]["reference"]["recipe"] == "repeat_gap"
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (32,)
    assert all(json.loads(row[0])["schema_version"] == 26
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=32;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v32-moment-splice-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history", "redo")}
source_manifest = Path("/tmp/deadpan-moment-paste-20260926/baseline.json")
report = dict(binary_sha256=binary_sha,
              source_manifest=str(source_manifest),
              source_manifest_sha256=hashlib.sha256(source_manifest.read_bytes()).hexdigest(),
              source_verified_checkpoint="/tmp/deadpan-group-navigation-20260926/checkpoint",
              input_fixture=input_fixture.name,
              input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=26, database_schema=32,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=log_path.name,
              producer_log_sha256=hashlib.sha256(log_path.read_bytes()).hexdigest(),
              doctor=doctor,
              method="Preserved core26/database32 CLI migrated the unchanged database31 fixture, inserted nested ordinary Sequences, authored an interior and a child-seam InsertTime, then undo/redo/undo left pending redo. Old gap overrides, bindings and abandoned history remain. No schema relabeling or snapshot/request/patch rewriting.")
(base / "v32-moment-splice-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
