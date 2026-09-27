"""Produce DB28 history with the preserved core22/database28 CLI.

Usage: python3 produce-v28-gap-branch-history.py OLD_CLI SCRATCH_DIRECTORY
The scratch directory must not exist. The prior v27 fixture is migrated by the
old binary; the added edit and undo are made by that binary too. No stored
snapshot, request or patch is rewritten by this script.
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
binary_sha = "d572c2b6e19f0d31e04912bc375b2deac7536225c9755381a508270d9414a475"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
base = Path(__file__).resolve().parent
input_fixture = base / "v27-audio-reanchor-history.sql"
args.scratch.mkdir()
project = args.scratch / "gap-branch-history.deadpan"
project.mkdir()
(project / "Snapshots").mkdir()
with sqlite3.connect(project / "project.sqlite") as database:
    database.executescript(input_fixture.read_text())
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
run("project", "migrate", project)
document = run("project", "dump", project, "--json")
request = dict(protocol=1, project_id=document["project_id"],
               expected_revision=document["revision_id"],
               new_revision="core22-gap-branch-rename",
               command=dict(command="rename", node=document["root"],
                            label="Core 22 authored rename"))
request_path = args.scratch / "rename.json"
request_path.write_text(json.dumps(request, indent=2) + "\n")
run("command", project, "--json", request_path)
document = run("project", "dump", project, "--json")
run("project", "undo", project, "--expected", document["revision_id"])
run("project", "validate", project)
final = run("project", "dump", project, "--json")
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (28,)
    assert all(json.loads(row[0])["schema_version"] == 22
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=28;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v28-gap-branch-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history")}
report = dict(binary_sha256=binary_sha,
              input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=22,
              database_schema=28, producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=str(args.scratch / "commands.json"),
              producer_log_sha256=hashlib.sha256((args.scratch / "commands.json").read_bytes()).hexdigest(),
              doctor=doctor,
              method="Actual core22/db28 CLI migrated the unmodified v27 history, then authored a rename and undo with pending redo. The prior fixture's gap owner has no authored gap binding; no later history or patch was rewritten.")
(base / "v28-gap-branch-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
