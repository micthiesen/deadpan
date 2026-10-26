#!/usr/bin/env python3
"""Retain exact invocation, scoped inputs, binary identity, output and exit."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path('/Users/michael/Code/deadpan')
SCRATCH = Path(__file__).resolve().parent


def pins(paths):
    return {str(path): {'bytes': path.stat().st_size,
                       'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
            for path in paths}


parser = argparse.ArgumentParser()
parser.add_argument('label')
parser.add_argument('--binary', action='append', default=[])
parser.add_argument('command', nargs=argparse.REMAINDER)
args = parser.parse_args()
command = args.command
if command and command[0] == '--':
    command = command[1:]
if not command or not args.label.replace('-', '').isalnum():
    raise SystemExit('A simple unique label and command are required')
os.environ.setdefault('CARGO_PROFILE_DEV_OPT_LEVEL', '2')
os.environ.setdefault('DEADPAN_REQUIRE_PHYSICAL_MAC', '1')
receipt = SCRATCH / f'{args.label}.json'
if receipt.exists():
    raise SystemExit('Refusing to overwrite a run')
sources = [ROOT / path for path in json.loads((SCRATCH / 'source-files.json').read_text())]
binaries = [Path(path) for path in args.binary]
removed = json.loads((SCRATCH / 'removed-files.json').read_text())
assert all(not (ROOT / name).exists() for name in removed), 'removed source reappeared'
before = pins(sources)
record = {
    'started_at': datetime.now(timezone.utc).isoformat(),
    'cwd': str(ROOT), 'command': command,
    'environment': {key: os.environ[key] for key in (
        'DEADPAN_FFMPEG_PREFIX', 'CARGO_TARGET_DIR', 'DEADPAN_REQUIRE_PHYSICAL_MAC',
        'DEADPAN_PREVIEW_EXPORT_KEEP', 'DEADPAN_MEDIA_WORKER',
        'DEADPAN_UPDATE_COMMANDS_MD', 'DEADPAN_UPDATE_ROUTING_BASELINE',
        'CARGO_PROFILE_DEV_OPT_LEVEL', 'DEADPAN_ORT_PREFIX',
        'DEADPAN_ALIGN_REFERENCE_ROOT', 'DEADPAN_ALIGN_CHECKPOINT',
        'DEADPAN_ALIGN_STATISTICS', 'DEADPAN_ALIGN_PARITY_OUTPUT',
        'DEADPAN_ALIGN_OBSERVED_OUTPUT',
        'DEADPAN_ALIGN_TEST_WORKER', 'DEADPAN_ALIGN_MODEL_ROOT') if key in os.environ},
    'removed_paths_absent_before': removed,
    'runner_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    'source_before': before, 'binary_before': pins(binaries),
}
receipt.write_text(json.dumps(record, indent=2) + '\n')
start = time.monotonic()
with (SCRATCH / f'{args.label}.stdout').open('xb') as stdout:
    with (SCRATCH / f'{args.label}.stderr').open('xb') as stderr:
        process = subprocess.Popen(command, cwd=ROOT, stdout=stdout, stderr=stderr)
        record['pid'] = process.pid
        receipt.write_text(json.dumps(record, indent=2) + '\n')
        code = process.wait()
record.update({'exit_code': code, 'seconds': time.monotonic() - start,
               'finished_at': datetime.now(timezone.utc).isoformat(),
               'source_after': pins(sources), 'binary_after': pins(binaries)})
record['removed_paths_absent_after'] = [name for name in removed if not (ROOT / name).exists()]
assert record['removed_paths_absent_after'] == removed, 'removed source reappeared during run'
record['sources_unchanged'] = record['source_before'] == record['source_after']
record['binaries_unchanged'] = record['binary_before'] == record['binary_after']
record['streams'] = pins([SCRATCH / f'{args.label}.stdout', SCRATCH / f'{args.label}.stderr'])
receipt.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'label': args.label, 'exit_code': code, 'seconds': record['seconds'],
                  'sources_unchanged': record['sources_unchanged'],
                  'binaries_unchanged': record['binaries_unchanged']}), flush=True)
raise SystemExit(code)
