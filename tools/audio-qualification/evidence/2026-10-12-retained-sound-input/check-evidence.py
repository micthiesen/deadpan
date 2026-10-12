#!/usr/bin/env python3
"""Check retained-input qualification evidence and optional current sources."""
import argparse
import gzip
import hashlib
import json
import re
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--source", action="store_true")
args = parser.parse_args()
here = Path(__file__).resolve().parent
repo = here.parents[3]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(name):
    return json.loads((here / name).read_text())


manifest = read("manifest.json")
assert set(manifest) == {
    path.name for path in here.iterdir()
    if path.is_file() and path.name != "manifest.json"
}
for name, expected in manifest.items():
    assert digest(here / name) == expected, name
commands = {row["name"]: row for row in read("commands.json")}
logs = {}
for name, row in commands.items():
    data = gzip.decompress((here / (name + ".log.gz")).read_bytes())
    assert hashlib.sha256(data).hexdigest() == row["log_sha256"], name
    logs[name] = data.decode()
for name in ("focused-final", "audio-playback-tests", "clippy", "fmt-check", "whitespace"):
    assert commands[name]["exit"] == 0, name
assert commands["focused-initial"]["exit"] != 0
assert "cannot find function `occurrence_raw_queries`" in logs["focused-initial"]
tests = logs["audio-playback-tests"]
assert "FAILED" not in tests
assert sum(map(int, re.findall(r"test result: ok\. (\d+) passed", tests))) == 692
assert not any(map(int, re.findall(r"failed; (\d+) ignored", tests)))
for name in (
    "retained_input_keeps_removed_history_and_old_extent_at_both_source_rates",
    "retained_input_rejects_foreign_ranges_and_cancellation_before_source_io",
    "retained_input_reads_nested_preserve_before_route_gaps_and_moves",
    "retained_projection_input_survives_fully_deleted_delivery",
):
    assert name + " ... ok" in tests, name
assert commands["audio-playback-tests"]["binaries"]
identity = read("identity.json")
if args.source:
    for name, expected in identity["sources"].items():
        assert digest(repo / name) == expected, name
print("Retained sound-input evidence verified" + (" against current source" if args.source else ""))
