"""Produce genuine DB34/core28 Retime history with the preserved old CLI.

Usage: python3 produce-v34-sound-history.py OLD_CLI SCRATCH_DIRECTORY
The old binary migrates the unchanged DB33 fixture and authors all added wire.
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
binary_sha = "f052a7d72019802e17210ce8a0e04dedcf281b7ff376d83c6cefc79048c289be"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
environment = dict(os.environ, PATH=str(binary.parent) + os.pathsep + os.environ["PATH"])
base = Path(__file__).resolve().parent
input_fixture = base / "v33-retime-history.sql"
args.scratch.mkdir()
project = args.scratch / "sound-history.deadpan"
project.mkdir()
(project / "Snapshots").mkdir()
with sqlite3.connect(project / "project.sqlite") as database:
    database.executescript(input_fixture.read_text())
log = []
log_path = base / "v34-sound-history.commands.json"


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


def navigate(verb):
    revision = run("project", "dump", project, "--json")["revision_id"]
    return run("project", verb, project, "--expected", revision)


doctor = run("doctor")
assert (doctor["document_schema"], doctor["database_schema"]) == (28, 34)
run("project", "migrate", project)
command("core28-wrap-retime", dict(command="wrap_retime", node="core26-second",
                                  id="core28-retime", duration=8, pitch="preserve"))
command("core28-abandoned-retime", dict(command="set_retime", node="core28-retime",
                                       duration=12, pitch="follow_speed"))
navigate("undo")
command("core28-set-retime", dict(command="set_retime", node="core28-retime",
                                 duration=6, pitch="follow_speed"))
command("core28-occurrence-retime", dict(
    command="edit_occurrence", instance=dict(node="core28-retime", repeats=[]),
    edit=dict(type="set_retime", duration=7, pitch="preserve"), identities=dict(nodes=[], marks=[])))
for verb in ("undo", "redo", "undo"):
    navigate(verb)
run("project", "validate", project)
final = run("project", "dump", project, "--json")
assert final["nodes"]["core28-retime"]["kind"]["duration"] == 6
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (34,)
    assert all(json.loads(row[0])["schema_version"] == 28
               for row in database.execute("select document from revisions"))
    assert database.execute("select count(*) from history where revision_id='core28-abandoned-retime'").fetchone() == (1,)
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=34;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v34-sound-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history", "redo")}
reopened = args.scratch / "reopened-sql.deadpan"
reopened.mkdir()
(reopened / "Snapshots").mkdir()
with sqlite3.connect(reopened / "project.sqlite") as database:
    database.executescript(sql)
run("project", "validate", reopened)
assert run("project", "dump", reopened, "--json") == final
source_manifest = Path("/tmp/deadpan-sound-events-20260927/baseline.json")
report = dict(binary_sha256=binary_sha,
              source_manifest=str(source_manifest),
              source_manifest_sha256=hashlib.sha256(source_manifest.read_bytes()).hexdigest(),
              source_base_commit="34e816a64b628de1731c2bf161669dc8fd483d40",
              input_fixture=input_fixture.name,
              input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=28, database_schema=34,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=log_path.name,
              producer_log_sha256=hashlib.sha256(log_path.read_bytes()).hexdigest(),
              reconstructed_sql_validated_by_old_binary=True,
              doctor=doctor,
              method="Preserved core28/database34 CLI migrated the unchanged database33 fixture, authored WrapRetime, SetRetime and occurrence SetRetime, abandoned one changed duration, then undo/redo/undo left pending redo. Earlier source splices, gap overrides, bindings and abandoned history remain. The emitted SQL was reconstructed and validated by the same old CLI. No schema relabeling or snapshot/request/patch rewriting.")
(base / "v34-sound-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
