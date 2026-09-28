"""Author genuine DB35/core29 sound history using the preserved previous CLI.

Usage: python3 produce-v35-sound-route-history.py OLD_CLI NEW_SCRATCH_DIRECTORY
The actual old binary measures the source and authors every snapshot and patch.
"""
from fractions import Fraction
from pathlib import Path
import argparse
import copy
import hashlib
import json
import os
import shutil
import sqlite3
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("scratch", type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
binary_sha = "0bcffddfc0d8383841ecec308ee4bea93c8bae40b36fcdcba0ec5f31e772a922"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
base = Path(__file__).resolve().parent
repo = base.parents[3]
source = repo / "native/deadpan-source/tests/fixtures/offset-bframes.mp4"
environment = dict(os.environ, PATH=str(binary.parent) + os.pathsep + os.environ["PATH"])
args.scratch.mkdir()
project = args.scratch / "sound-route-history.deadpan"
log = []
log_path = base / "v35-sound-route-history.commands.json"


def run(*arguments):
    invocation = [binary.name, *map(str, arguments)]
    result = subprocess.run(invocation, capture_output=True, text=True,
                            env=environment, timeout=60)
    record = dict(command=invocation, returncode=result.returncode,
                  stdout_sha256=hashlib.sha256(result.stdout.encode()).hexdigest(),
                  stderr=result.stderr)
    if arguments[0] == "doctor" or arguments[1] in ("validate", "register-source"):
        record["stdout"] = result.stdout
    log.append(record)
    log_path.write_text(json.dumps(log, indent=2) + "\n")
    if result.returncode:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)


def dump():
    return run("project", "dump", project, "--json")


def command(revision, payload):
    document = dump()
    request = dict(protocol=1, project_id=document["project_id"],
                   expected_revision=document["revision_id"],
                   new_revision=revision, command=payload)
    path = args.scratch / (revision + ".json")
    path.write_text(json.dumps(request, indent=2) + "\n")
    return run("command", project, "--json", path)


def navigate(verb):
    return run("project", verb, project, "--expected", dump()["revision_id"])


def ratio(value):
    return dict(numerator=str(value.numerator), denominator=str(value.denominator))


doctor = run("doctor")
assert (doctor["document_schema"], doctor["database_schema"]) == (29, 35)
run("project", "create", project, "--fps", "30000/1001", "--size", "320x180")
initial = dump()
original = run("project", "retain-original", project, source)["retained_original"]["record"]
registration = dict(protocol=1, registration=dict(
    expected_revision=initial["revision_id"], new_revision="core29-import",
    original=original["object"]["content"], new_asset_id="camera", label="Measured camera",
    insertion=dict(parent=initial["root"], index=0, node="clip", label="Original", purpose="primary")),
    streams=dict(type="video_and_audio", audio_stream=1))
request_path = args.scratch / "registration.json"
request_path.write_text(json.dumps(registration, indent=2) + "\n")
run("project", "register-source", project, "--request-json", request_path)
command("core29-repeat", dict(command="wrap_repeat", node="clip", id="repeat", plays=2, gap=None))
document = dump()
span = copy.deepcopy(document["assets"]["camera"]["audio"])
span["end"]["ticks"] = (span["start"]["ticks"] + span["end"]["ticks"]) // 2
clock = span["start"]["time_base"]
frames = (Fraction(span["end"]["ticks"] - span["start"]["ticks"])
          * Fraction(clock["numerator"], clock["denominator"]) * Fraction(30000, 1001))
event = dict(owner=document["root"], label="Impact", source=dict(asset="camera", span=span),
             mapping=dict(type="duration", frames=ratio(frames)), offset=137,
             gain_millidecibels=-3000, start_edge="automatic", end_edge="hard", overflow="reject")
command("core29-add-impact", dict(command="set_sound", id="impact", event=event))
abandoned = dict(event, label="Abandoned impact", offset=941, gain_millidecibels=-6000)
command("core29-abandoned-impact", dict(command="set_sound", id="impact", event=abandoned))
navigate("undo")
changed = dict(event, label="Retained impact", offset=277, gain_millidecibels=-9000)
command("core29-update-impact", dict(command="set_sound", id="impact", event=changed))
bed = dict(event, label="Bed", offset=13, gain_millidecibels=-12000, end_edge="automatic")
command("core29-add-bed", dict(command="set_sound", id="bed", event=bed))
command("core29-label", dict(command="rename", node="repeat", label="Two plays with overlays"))
command("core29-delete-impact", dict(command="delete_sound", id="impact"))
for verb in ("undo", "redo", "undo"):
    navigate(verb)
run("project", "validate", project)
final = dump()
assert final["sounds"] == dict(impact=changed, bed=bed)
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (35,)
    assert all(json.loads(row[0])["schema_version"] == 29
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=35;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v35-sound-route-history.sql").write_text(sql)
    counts = {table: database.execute(f"select count(*) from {table}").fetchone()[0]
              for table in ("revisions", "history", "redo", "source_qualifications", "original_media")}
reopened = args.scratch / "reopened-sql.deadpan"
reopened.mkdir()
(reopened / "Snapshots").mkdir()
shutil.copytree(project / "Media", reopened / "Media")
with sqlite3.connect(reopened / "project.sqlite") as database:
    database.executescript(sql)
run("project", "validate", reopened)
assert run("project", "dump", reopened, "--json") == final
report = dict(binary_sha256=binary_sha,
              source_base_commit="d6e931c8c57f5684302ca9bc903d43ea84fed1a8",
              source_fixture=str(source.relative_to(repo)),
              source_fixture_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
              original=original, sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              initial_revision=initial["revision_id"], root=initial["root"],
              final_revision=final["revision_id"], core_schema=29, database_schema=35,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=log_path.name,
              producer_log_sha256=hashlib.sha256(log_path.read_bytes()).hexdigest(),
              reconstructed_sql_validated_by_old_binary=True, doctor=doctor,
              method="The preserved clean core29/database35 CLI created an explicit NTSC project, retained and measured the real MP4, wrapped a Repeat, and authored two sounds, a discarded sound branch, parameter updates, rename, deletion and undo/redo/undo. SQL was reconstructed with the unchanged managed Original and validated by the same old CLI. No schema relabeling or snapshot/request/patch rewriting.")
(base / "v35-sound-route-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
