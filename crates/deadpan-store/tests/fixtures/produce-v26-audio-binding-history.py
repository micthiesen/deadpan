"""Produce DB26 history with the preserved pre-reanchor CLI, never version relabeling.

Usage: python3 produce-v26-audio-binding-history.py OLD_CLI SCRATCH_DIRECTORY
The scratch directory must not exist. Only SQL and provenance enter this directory.
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
binary_sha = "8694544804e5554d3d4bff5fa0ed57255672306205b80acee8437d9c62c205f5"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
base = Path(__file__).resolve().parent
args.scratch.mkdir()
project = args.scratch / "audio-binding-history.deadpan"
project.mkdir()
(project / "Snapshots").mkdir()
input_fixture = base / "v25-captured-audio-history.sql"
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


def snapshot():
    return run("project", "dump", project, "--json")


def command(revision, payload):
    document = snapshot()
    request = dict(protocol=1, project_id=document["project_id"],
                   expected_revision=document["revision_id"], new_revision=revision,
                   command=payload)
    path = args.scratch / (revision + ".json")
    path.write_text(json.dumps(request, indent=2) + "\n")
    return run("command", project, "--json", path)


def ratio(n, d=1):
    return dict(numerator=str(n), denominator=str(d))


doctor = run("doctor")
assert (doctor["document_schema"], doctor["database_schema"]) == (20, 26)
run("project", "migrate", project)
run("project", "redo", project, "--expected", snapshot()["revision_id"])
command("selected-placement", dict(command="set_source_audio_mapping", node="source",
        mapping=dict(type="selected_placement", start=ratio(-2, 3),
                     frames=ratio(30000, 1001),
                     selection=dict(start=ratio(2), end=ratio(20))), offset=-31))
# The old InsertTime capture rejects Repeat gap owners. Keep that authored
# Repeat in history and remove it by the same old command API before pausing.
command("remove-gap-repeat", dict(command="delete", node="repeat"))
document = snapshot()


def duration(node):
    kind = document["nodes"][node]["kind"]
    if kind["type"] == "sequence":
        return sum(duration(child) for child in kind["children"])
    if kind["type"] == "source":
        return kind["source"]["duration"]
    if kind["type"] == "hold":
        return kind["recipe"]["duration"]
    if kind["type"] == "retime":
        return kind["duration"]
    if kind["type"] == "repeat":
        count = sum(run["count"] for run in kind["iterations"]["runs"])
        return count * duration(kind["child"]) + (count - 1) * (kind.get("gap") or {}).get("duration", 0)
    raise AssertionError(kind)


children = document["nodes"][document["root"]]["kind"]["children"]
source_start = sum(duration(node) for node in children[:children.index("source")])
command("selected-pause", dict(command="insert_time", at=source_start + 5,
        hold=dict(duration=2, video=dict(type="background"), audio=dict(type="silence")),
        id="selected-pause-hold",
        identities=dict(nodes=[f"selected-split-{i}" for i in range(len(document["nodes"]) + 4)]),
        timing=dict(allocation="selected-pause", ordinal=0)))
command("pending-binding-rename", dict(command="rename", node=document["root"],
        label="Retained selected-audio redo"))
for verb in ("undo", "redo", "undo"):
    run("project", verb, project, "--expected", snapshot()["revision_id"])
run("project", "validate", project)
final = snapshot()
bindings = final["audio_bindings"]["bindings"]
assert any(binding.get("resume", {}).get("phase", {}).get("terms")
           for binding in bindings.values() if binding.get("resume"))
assert all("reanchors" not in binding for binding in bindings.values())
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (26,)
    assert all(json.loads(row[0])["schema_version"] == 20
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=26;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v26-audio-binding-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history")}
report = dict(binary_sha256=binary_sha,
              input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=20, database_schema=26,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=str(args.scratch / "commands.json"),
              producer_log_sha256=hashlib.sha256((args.scratch / "commands.json").read_bytes()).hexdigest(),
              doctor=doctor,
              method="Restored actual core19/db25 SQL history, migrated with the preserved core20/db26 CLI, authored SelectedPlacement and InsertTime with resume phase terms, then undo/redo/pending redo. Dumped SQL only after old binary validation.")
(base / "v26-audio-binding-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
