"""Produce genuine DB33/core27 SpliceSource history.

Usage: python3 produce-v33-retime-history.py OLD_CLI SCRATCH_DIRECTORY
The preserved binary migrates the unchanged DB32 fixture and authors every
added request, snapshot, patch and navigation revision. No wire is rewritten.
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
binary_sha = "9f8642c3d8e7884461e20f72917e3668151a510c85f34e07212d6e2893ec0995"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
environment = dict(os.environ, PATH=str(binary.parent) + os.pathsep + os.environ["PATH"])
base = Path(__file__).resolve().parent
input_fixture = base / "v32-moment-splice-history.sql"
args.scratch.mkdir()
project = args.scratch / "retime-history.deadpan"
project.mkdir()
(project / "Snapshots").mkdir()
with sqlite3.connect(project / "project.sqlite") as database:
    database.executescript(input_fixture.read_text())
log = []
log_path = base / "v33-retime-history.commands.json"


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


def splice(revision, parent, index, identity):
    document = run("project", "dump", project, "--json")
    source = document["nodes"]["source"]["kind"]["source"]
    command(revision, dict(command="splice_source", parent=parent, index=index,
                          source=source, id=identity, label="Pasted selected audio",
                          timing=dict(allocation=revision, ordinal=0)))


doctor = run("doctor")
assert (doctor["document_schema"], doctor["database_schema"]) == (27, 33)
run("project", "migrate", project)
document = run("project", "dump", project, "--json")
splice("core27-root-splice", document["root"], 1, "core27-root-paste")
splice("core27-abandoned-splice", "core26-inner", 0, "core27-abandoned-paste")
navigate("undo")
splice("core27-nested-splice", "core26-inner", 1, "core27-nested-paste")
for verb in ("undo", "redo", "undo"):
    navigate(verb)
run("project", "validate", project)
final = run("project", "dump", project, "--json")
assert "core27-root-paste" in final["nodes"]
assert "core27-nested-paste" not in final["nodes"]
assert "core27-abandoned-paste" not in final["nodes"]
assert final["audio_bindings"]["bindings"]["core23-detached-gap"]["lattice"]["reference"]["recipe"] == "repeat_gap"
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (33,)
    assert all(json.loads(row[0])["schema_version"] == 27
               for row in database.execute("select document from revisions"))
    assert database.execute("select count(*) from history where revision_id='core27-abandoned-splice'").fetchone() == (1,)
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=33;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v33-retime-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history", "redo")}
# Independently reconstruct the checked-in SQL and prove this old binary opens it.
reopened = args.scratch / "reopened-sql.deadpan"
reopened.mkdir()
(reopened / "Snapshots").mkdir()
with sqlite3.connect(reopened / "project.sqlite") as database:
    database.executescript(sql)
run("project", "validate", reopened)
assert run("project", "dump", reopened, "--json") == final
source_manifest = Path("/tmp/deadpan-retime-20260927/baseline.json")
report = dict(binary_sha256=binary_sha,
              source_manifest=str(source_manifest),
              source_manifest_sha256=hashlib.sha256(source_manifest.read_bytes()).hexdigest(),
              source_verified_checkpoint="/tmp/deadpan-original-audition-20260927/checkpoint",
              input_fixture=input_fixture.name,
              input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=27, database_schema=33,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=log_path.name,
              producer_log_sha256=hashlib.sha256(log_path.read_bytes()).hexdigest(),
              reconstructed_sql_validated_by_old_binary=True,
              doctor=doctor,
              method="Preserved core27/database33 CLI migrated the unchanged database32 fixture, authored root and nested SpliceSource with exact selected audio, abandoned one nested paste, then undo/redo/undo left pending redo. Old gap overrides, bindings and earlier abandoned history remain. The emitted SQL was reconstructed and independently validated by the same old CLI. No schema relabeling or snapshot/request/patch rewriting.")
(base / "v33-retime-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
