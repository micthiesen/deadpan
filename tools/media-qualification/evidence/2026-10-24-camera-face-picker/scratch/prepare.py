#!/usr/bin/env python3
"""Freeze private source snapshots and sibling executables before the replay."""
import hashlib
import argparse
import json
from pathlib import Path
import shutil

SCRATCH = Path(__file__).resolve().parent
ROOT = Path('/Users/michael/Code/deadpan')
BUILD = Path('/tmp/deadpan-follow-creep-2026-10-24/target/release')


def identity(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    return {'bytes': path.stat().st_size, 'sha256': digest.hexdigest()}


parser = argparse.ArgumentParser()
parser.add_argument('--label', default='first', choices=['first', 'final'])
args = parser.parse_args()
binary_root = SCRATCH / f'bin-{args.label}'
source_root = SCRATCH / f'source-{args.label}'
binary_root.mkdir()
source_root.mkdir()
rows = {'sources': {}, 'binaries': {}}
for name in json.loads((SCRATCH / 'source-files.json').read_text()):
    source = ROOT / name
    destination = source_root / (name + '.txt')
    destination.parent.mkdir(parents=True, exist_ok=True)
    before = identity(source)
    shutil.copy2(source, destination)
    assert before == identity(source) == identity(destination)
    rows['sources'][name] = before
for name in ['deadpan-app', 'deadpan-cli', 'deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track']:
    source = BUILD / name
    destination = binary_root / name
    before = identity(source)
    shutil.copy2(source, destination)
    assert before == identity(source) == identity(destination)
    rows['binaries'][name] = {'executed': str(destination), **before}
with (SCRATCH / f'{args.label}-identities.json').open('x') as output:
    json.dump(rows, output, indent=2)
    output.write('\n')
print(json.dumps({'source_paths': len(rows['sources']), 'binaries': rows['binaries']}))
