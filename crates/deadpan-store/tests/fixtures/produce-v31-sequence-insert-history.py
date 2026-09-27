"""Produce genuine DB31/core25 root interior InsertTime history with the preserved CLI.

Usage: python3 produce-v31-sequence-insert-history.py OLD_CLI SCRATCH_DIRECTORY
The scratch directory must not exist. The old binary migrates the unmodified
DB30 fixture and authors every new command, snapshot, patch and navigation row.
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
binary_sha = "0d2b9ea0d1105c1ea66f4fc51d7e10f0e1c5d29a176223964f7c03896b3a0273"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
base = Path(__file__).resolve().parent
input_fixture = base / "v30-interior-insert-history.sql"
args.scratch.mkdir()
project = args.scratch / "sequence-insert-history.deadpan"
project.mkdir()
(project / "Snapshots").mkdir()
with sqlite3.connect(project / "project.sqlite") as database:
    database.executescript(input_fixture.read_text())
log = []
log_path = base / "v31-sequence-insert-history.commands.json"


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
assert (doctor["document_schema"], doctor["database_schema"]) == (25, 31)
run("project", "migrate", project)
hold = dict(duration=2, video=dict(type="background"), audio=dict(type="silence"))
# Core25 newly admitted this root Hold interior before a composite Repeat suffix.
# Retain all older composite seams, gap overrides, bindings and abandoned history.
command("core25-root-interior", dict(command="insert_time", at=2, hold=hold,
        id="core25-pause", identities=dict(nodes=["core25-left", "core25-right", "core25-owner"]),
        timing=dict(allocation="core25-root-interior", ordinal=0)))
command("core25-following-seam", dict(command="insert_time", at=0,
        hold=dict(hold, duration=3), id="core25-second-pause", identities=dict(nodes=[]),
        timing=dict(allocation="core25-following-seam", ordinal=0)))
for verb in ("undo", "redo", "undo"):
    navigate(verb)
run("project", "validate", project)
final = run("project", "dump", project, "--json")
assert "core25-pause" in final["nodes"]
assert "core24-pause" in final["nodes"]
assert "core25-second-pause" not in final["nodes"]
assert final["audio_bindings"]["bindings"]["core23-detached-gap"]["lattice"]["reference"]["recipe"] == "repeat_gap"
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (31,)
    assert all(json.loads(row[0])["schema_version"] == 25
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=31;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v31-sequence-insert-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history", "redo")}
report = dict(binary_sha256=binary_sha,
              input_fixture=input_fixture.name,
              input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=25, database_schema=31,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=log_path.name,
              producer_log_sha256=hashlib.sha256(log_path.read_bytes()).hexdigest(),
              doctor=doctor,
              method="Preserved core25/database31 CLI migrated the unmodified database30 root-seam/gap history, then authored an InsertTime inside the first root Hold before a composite Repeat suffix and a new root seam, with undo/redo/undo leaving pending redo. Earlier gap isolation/overrides and abandoned history remain intact. No schema relabeling or snapshot/request/patch rewriting.")
(base / "v31-sequence-insert-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
