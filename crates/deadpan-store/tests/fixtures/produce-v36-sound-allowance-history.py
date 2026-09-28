"""Author genuine DB36/core30 sound history using the preserved previous CLI.

Usage: python3 produce-v36-sound-allowance-history.py OLD_CLI NEW_SCRATCH_DIRECTORY
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
binary_sha = "dcf9b01910439a3b9f5f8d762e69c60d609f3daee7d0a87f9ffa84d7f93daa7d"
assert hashlib.sha256(binary.read_bytes()).hexdigest() == binary_sha
base = Path(__file__).resolve().parent
repo = base.parents[3]
source = repo / "native/deadpan-source/tests/fixtures/offset-bframes.mp4"
environment = dict(os.environ, PATH=str(binary.parent) + os.pathsep + os.environ["PATH"],
                   DEADPAN_FFMPEG_PREFIX="/tmp/deadpan-ui-ffmpeg/prefix")
args.scratch.mkdir()
project = args.scratch / "sound-allowance-history.deadpan"
log = []
log_path = base / "v36-sound-allowance-history.commands.json"


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
assert (doctor["document_schema"], doctor["database_schema"]) == (30, 36)
run("project", "create", project, "--fps", "30000/1001", "--size", "320x180")
initial = dump()
original = run("project", "retain-original", project, source)["retained_original"]["record"]
registration = dict(protocol=1, registration=dict(
    expected_revision=initial["revision_id"], new_revision="core30-import",
    original=original["object"]["content"], new_asset_id="camera", label="Measured camera",
    insertion=dict(parent=initial["root"], index=0, node="clip", label="Original", purpose="primary")),
    streams=dict(type="video_and_audio", audio_stream=1))
request_path = args.scratch / "registration.json"
request_path.write_text(json.dumps(registration, indent=2) + "\n")
run("project", "register-source", project, "--request-json", request_path)
document = dump()
span = copy.deepcopy(document["assets"]["camera"]["audio"])
span["end"]["ticks"] = (span["start"]["ticks"] + span["end"]["ticks"]) // 2
clock = span["start"]["time_base"]
frames = (Fraction(span["end"]["ticks"] - span["start"]["ticks"])
          * Fraction(clock["numerator"], clock["denominator"]) * Fraction(30000, 1001))
event = dict(owner=document["root"], label="Impact", source=dict(asset="camera", span=span),
             mapping=dict(type="duration", frames=ratio(frames)), offset=137,
             gain_millidecibels=-3000, start_edge="automatic", end_edge="hard", overflow="reject")
command("core30-add-impact", dict(command="set_sound", id="impact", event=event))
bed = dict(event, label="Bed", offset=13, gain_millidecibels=-12000, end_edge="automatic")
command("core30-add-bed", dict(command="set_sound", id="bed", event=bed))


def pause(revision, at, frames):
    command(revision, dict(command="insert_time", at=at,
            hold=dict(duration=frames, video=dict(type="background"), audio=dict(type="silence")),
            id=revision + "-hold", identities=dict(nodes=[]),
            timing=dict(allocation=revision, ordinal=0)))


pause("core30-first-pause", 0, 1)
pause("core30-second-pause", 1, 2)
changed = dict(event, label="Retained impact", gain_millidecibels=-9000)
command("core30-update-impact", dict(command="set_sound", id="impact", event=changed))
abandoned = dict(event, label="Abandoned replacement", offset=941, gain_millidecibels=-6000)
command("core30-abandoned-replacement", dict(command="replace_sound", id="impact", event=abandoned))
assert "impact" not in dump().get("sound_routes", {})
navigate("undo")
command("core30-split", dict(command="split", node="clip", at=1,
        identities=dict(nodes=["split-" + str(i) for i in range(16)])))
command("core30-delete-pause", dict(command="delete", node="core30-second-pause-hold"))
replacement = dict(bed, label="Replaced bed", offset=71)
command("core30-replace-bed", dict(command="replace_sound", id="bed", event=replacement))
for verb in ("undo", "redo", "undo"):
    navigate(verb)
run("project", "validate", project)
final = dump()
assert final["sounds"] == dict(impact=changed, bed=bed)
assert set(final["sound_routes"]) == {"impact", "bed"}
assert all(len(route["edits"]) == 3 for route in final["sound_routes"].values())
with sqlite3.connect(project / "project.sqlite") as database:
    assert database.execute("pragma user_version").fetchone() == (36,)
    assert all(json.loads(row[0])["schema_version"] == 30
               for row in database.execute("select document from revisions"))
    appid = database.execute("pragma application_id").fetchone()[0]
    sql = (f"PRAGMA application_id={appid};\nPRAGMA user_version=36;\n"
           + "\n".join(database.iterdump()) + "\n")
    (base / "v36-sound-allowance-history.sql").write_text(sql)
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
              source_base_commit="357ae533b399ccf51e6dc3196ba2e0bba720ef9f",
              binary_provenance="Preserved target/debug/deadpan-cli from the preceding workspace gate, copied before allowance implementation. The subsequent 357ae533 app-only commit did not change its CLI/core dependencies; this task did not rebuild the legacy executable.",
              source_fixture=str(source.relative_to(repo)),
              source_fixture_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
              original=original, sql_sha256=hashlib.sha256(sql.encode()).hexdigest(), counts=counts,
              initial_revision=initial["revision_id"], root=initial["root"],
              final_revision=final["revision_id"], core_schema=30, database_schema=36,
              producer_script=Path(__file__).name,
              producer_script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              producer_log=log_path.name,
              producer_log_sha256=hashlib.sha256(log_path.read_bytes()).hexdigest(),
              reconstructed_sql_validated_by_old_binary=True, doctor=doctor,
              method="The preserved core30/database36 CLI created an explicit NTSC project, retained and measured the real MP4, authored two sounds, InsertTime routes, a parameter update, an abandoned ReplaceSound branch, non-root Split, ordinary Sequence Delete and a ReplaceSound undo/redo/undo. SQL was reconstructed with the unchanged managed Original and validated by the same old CLI. No schema relabeling or snapshot/request/patch rewriting.")
(base / "v36-sound-allowance-history.provenance.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
