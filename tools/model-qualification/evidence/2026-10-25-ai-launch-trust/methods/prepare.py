#!/usr/bin/env python3
"""Scratch setup only: clone the installed approved pack; never run models."""
import hashlib
import json
from pathlib import Path
import shutil
import stat
import subprocess
import time

ROOT = Path(__file__).resolve().parent
SOURCE = Path.home() / 'Library/Application Support/Deadpan/models/ltx-2.3-q4-extension/1'
DEST = ROOT / 'home/Library/Application Support/Deadpan/Models/ltx-2.3-q4-extension/1'
FIXTURE = Path('/Users/michael/Code/deadpan/native/deadpan-source/tests/fixtures/cfr-bframes.mp4')
started = time.monotonic()
receipt = json.loads((SOURCE / 'receipt.json').read_text())
assert receipt['pack_id'] == 'ltx-2.3-q4-extension' and receipt['pack_version'] == '1'
assert len(receipt['files']) == 31 and len(receipt['accepted_licenses']) == 2
DEST.parent.mkdir(parents=True, exist_ok=True)
if not DEST.exists():
    subprocess.run(['cp', '-cR', str(SOURCE), str(DEST)], check=True)
assert (DEST / 'receipt.json').read_bytes() == (SOURCE / 'receipt.json').read_bytes()
verified = []
for entry in receipt['files']:
    relative = Path(entry['name'])
    assert not relative.is_absolute() and '..' not in relative.parts
    src, dst = SOURCE / relative, DEST / relative
    before, cloned = src.stat(), dst.lstat()
    assert stat.S_ISREG(cloned.st_mode) and before.st_size == cloned.st_size == entry['bytes']
    assert (before.st_dev, before.st_ino) != (cloned.st_dev, cloned.st_ino), 'must not hard-link owner weights'
    with dst.open('rb') as file:
        digest = hashlib.file_digest(file, 'sha256').hexdigest()
    assert digest == entry['sha256'], str(relative)
    verified.append({'name': str(relative), 'bytes': cloned.st_size, 'sha256': digest,
                     'clone_has_distinct_inode': True})
fixture = ROOT / 'cfr-bframes.mp4'
if not fixture.exists():
    subprocess.run(['cp', '-c', str(FIXTURE), str(fixture)], check=True)
with fixture.open('rb') as file:
    fixture_hash = hashlib.file_digest(file, 'sha256').hexdigest()
report = {'source_pack': str(SOURCE), 'cloned_pack': str(DEST),
          'pack_id': receipt['pack_id'], 'pack_version': receipt['pack_version'],
          'accepted_licenses_reused': receipt['accepted_licenses'],
          'bytes': sum(x['bytes'] for x in verified), 'files': verified,
          'fixture': str(fixture), 'fixture_sha256': fixture_hash,
          'seconds': time.monotonic() - started, 'inference_or_product_commands_run': False}
(ROOT / 'setup.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k: report[k] for k in ['pack_id', 'pack_version', 'bytes', 'seconds',
                                        'cloned_pack', 'fixture', 'fixture_sha256']}, indent=2))
