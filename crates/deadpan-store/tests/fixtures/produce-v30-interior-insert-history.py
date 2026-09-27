"""Produce genuine DB30/core24 composite seam InsertTime history with the preserved CLI.

Usage: python3 produce-v30-interior-insert-history.py OLD_CLI SCRATCH_DIRECTORY
The scratch directory must not exist. The old binary migrates the unmodified
DB29 fixture and authors every new command, snapshot, patch and navigation row.
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
binary_sha = "ba8c2c9dd860d0ae8540df6aaa3759d5f0b61e9bbadc87798fbbe2cabd35522a"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
base = Path(__file__).resolve().parent
input_fixture = base / "v29-composite-insert-history.sql"
args.scratch.mkdir()
project = args.scratch / "interior-insert-history.deadpan"
project.mkdir()
(project / "Snapshots").mkdir()
with sqlite3.connect(project / "project.sqlite") as database:
    database.executescript(input_fixture.read_text())
log = []
log_path = base / "v30-interior-insert-history.commands.json"


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
assert (doctor["document_schema"], doctor["database_schema"]) == (24, 30)
run("project", "migrate", project)
hold = dict(duration=5, video=dict(type="background"), audio=dict(type="silence"))
# First root seam precedes the composite Repeat and all authored gap branches.
# The later seam is after the new Hold and before that same Repeat.
command("core24-composite-seam", dict(command="insert_time", at=0, hold=hold,
        id="core24-pause", identities=dict(nodes=[]),
        timing=dict(allocation="core24-composite-seam", ordinal=0)))
command("core24-second-seam", dict(command="insert_time", at=5,
        hold=dict(hold, duration=2), id="core24-second-pause", identities=dict(nodes=[]),
        timing=dict(allocation="core24-second-seam", ordinal=0)))
for verb in ("undo", "redo", "undo"):
    navigate(verb)
run("project", "validate", project)
final = run("project", "dump", project, "--json")
assert "core24-pause" in final["nodes"]
assert "core24-second-pause" not in final["nodes"]
assert final["audio_bindings"]["bindings"]["core23-detached-gap"]["lattice"]["reference"]["recipe"] == "repeat_gap"
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (30,)
    assert all(json.loads(row[0])["schema_version"] == 24
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=30;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v30-interior-insert-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history", "redo")}
report = dict(binary_sha256=binary_sha,
              input_fixture=input_fixture.name,
              input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=24, database_schema=30,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=log_path.name,
              producer_log_sha256=hashlib.sha256(log_path.read_bytes()).hexdigest(),
              doctor=doctor,
              method="Preserved core24/database30 CLI migrated the unmodified database29 gap/InsertTime history, then authored two InsertTime commands at root seams before its composite Repeat suffix, with undo/redo/undo leaving pending redo. Earlier gap isolation/overrides and abandoned history remain intact. No schema relabeling or snapshot/request/patch rewriting.")
(base / "v30-interior-insert-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
