"""Produce DB27 reanchor history using the preserved core21/database27 CLI.

Usage: python3 produce-v27-audio-reanchor-history.py OLD_CLI SCRATCH_DIRECTORY
The scratch directory must not exist. Only SQL and provenance enter this directory.
The CLI lacks an initial-document import. Seed one initial document before any
history, validate with that CLI, and create every subsequent revision through it.
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
binary_sha = "2c3a9501d1d412797ea076f6c8f56c5ad529b1cbd4bba0fdb200e6a364be4600"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
base = Path(__file__).resolve().parent
args.scratch.mkdir()
input_fixture = base / "v26-audio-binding-history.sql"
source_project = args.scratch / "source.deadpan"
source_project.mkdir()
(source_project / "Snapshots").mkdir()
with sqlite3.connect(source_project / "project.sqlite") as database:
    database.executescript(input_fixture.read_text())
project = args.scratch / "audio-reanchor-history.deadpan"
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


doctor = run("doctor")
assert (doctor["document_schema"], doctor["database_schema"]) == (21, 27)
run("project", "migrate", source_project)
source = run("project", "dump", source_project, "--json")
run("project", "create", project, "--fps", "30000/1001", "--size", "16x16")
empty = snapshot()
initial = copy.deepcopy(source)
initial["project_id"] = empty["project_id"]
initial["revision_id"] = empty["revision_id"]
for binding in initial["audio_bindings"]["bindings"].values():
    binding["reanchors"] = [dict(placement=copy.deepcopy(binding["lattice"]))]
initial_json = json.dumps(initial, separators=(",", ":"))
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("select count(*) from revisions").fetchone() == (1,)
    assert database.execute("select count(*) from history").fetchone() == (0,)
    database.execute("update revisions set document=? where kind='initial'", (initial_json,))
run("project", "validate", project)
assert snapshot() == initial
# The seeded snapshot is explicit initial fixture intent. No authored revision,
# request, forward patch or inverse patch is edited by the producer.
command("split-reanchor", dict(command="split", node="selected-pause-hold", at=1,
        identities=dict(nodes=["reanchor-left", "reanchor-right", "reanchor-copy"])))
document = snapshot()
command("append-reanchor", dict(command="insert_time", at=1,
        hold=dict(duration=2, video=dict(type="background"), audio=dict(type="silence")),
        id="reanchor-pause",
        identities=dict(nodes=[f"reanchor-split-{i}" for i in range(len(document["nodes"]) + 4)]),
        timing=dict(allocation="append-reanchor", ordinal=0)))
# Old documents can add configured gaps after their last capture. Replaying
# these ordinary edits must not invent a gap binding under the modern schema.
gap = dict(duration=3, video=dict(type="background"), audio=dict(type="silence"))
command("old-configured-gap", dict(command="wrap_repeat", node="reanchor-pause",
        id="old-gap-owner", plays=1, gap=gap))
command("old-gap-growth", dict(command="set_repeat", node="old-gap-owner", plays=3, gap=gap))
command("pending-reanchor-rename", dict(command="rename", node=initial["root"],
        label="Retained reanchor redo"))
for verb in ("undo", "redo", "undo"):
    run("project", verb, project, "--expected", snapshot()["revision_id"])
run("project", "validate", project)
final = snapshot()
bindings = final["audio_bindings"]["bindings"]
assert any(binding.get("resume", {}).get("phase", {}).get("terms")
           for binding in bindings.values() if binding.get("resume"))
assert any(len(binding.get("reanchors", [])) > 1 for binding in bindings.values())
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (27,)
    assert all(json.loads(row[0])["schema_version"] == 21
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=27;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v27-audio-reanchor-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history")}
report = dict(binary_sha256=binary_sha,
              input_fixture_sha256=hashlib.sha256(input_fixture.read_bytes()).hexdigest(),
              initial_document_sha256=hashlib.sha256(initial_json.encode()).hexdigest(),
              sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              final_revision=final["revision_id"], core_schema=21, database_schema=27,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=str(args.scratch / "commands.json"),
              producer_log_sha256=hashlib.sha256((args.scratch / "commands.json").read_bytes()).hexdigest(),
              doctor=doctor,
              method="Actual core21/db27 CLI migrated v26 seed and created a new package. Before any history, seeded its sole initial snapshot with selected audio, legacy phase terms and explicit reanchor steps; old CLI validated it. All Split, InsertTime, post-capture configured-gap WrapRepeat/SetRepeat, rename, undo/redo and pending-redo chronology was then produced and validated by that binary. No schema relabeling or history patch rewriting.")
(base / "v27-audio-reanchor-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
