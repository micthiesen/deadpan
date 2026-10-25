"""Remove one explicit rebuildable incremental cache with runtime-pin readback."""
from pathlib import Path
from datetime import datetime, timezone
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('target', choices=['task', 'root'])
parser.add_argument('--execute', action='store_true', required=True)
args = parser.parse_args()
s = Path(__file__).resolve().parent
target = Path('/tmp/deadpan-follow-creep-2026-10-24/target' if args.target == 'task' else '/Users/michael/Code/deadpan/target').resolve(strict=True)
cache = target / 'debug/incremental'
assert cache.is_dir() and not cache.is_symlink() and cache.resolve() == cache
receipt = s / f'clear-incremental-{args.target}.json'
assert not receipt.exists()
pins = json.loads((s / 'final-identities.json').read_text())['binaries']

def check_pins():
    for row in pins.values():
        path = Path(row['executed'])
        assert path.stat().st_size == row['bytes']
        assert hashlib.sha256(path.read_bytes()).hexdigest() == row['sha256']

def available():
    stats = os.statvfs(target)
    return stats.f_bavail * stats.f_frsize

check_pins()
usage = subprocess.run(['du', '-sk', str(cache)], capture_output=True, text=True, check=True).stdout
record = {'started_at': datetime.now(timezone.utc).isoformat(), 'cache': str(cache),
          'available_before': available(), 'allocated_reference_du': usage,
          'scope': 'Only rebuildable debug/incremental. No sources, top-level executables, deps, bundles, fixtures, evidence or other target directories.',
          'allocation_note': 'APFS shared or cloned allocation makes du unsuitable as a physical reclamation claim.',
          'state': 'running'}
receipt.write_text(json.dumps(record, indent=2) + '\n')
start = time.monotonic()
shutil.rmtree(cache)
check_pins()
record.update(state='complete', seconds=time.monotonic()-start, available_after=available(),
              cache_absent=not cache.exists(), frozen_executables_unchanged=True,
              finished_at=datetime.now(timezone.utc).isoformat())
receipt.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record))
