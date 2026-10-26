#!/usr/bin/env python3
"""Retain a research checkpoint without claiming product qualification."""
import gzip
import hashlib
import json
from pathlib import Path
import re
import shutil

ROOT = Path('/Users/michael/Code/deadpan')
SCRATCH = Path(__file__).resolve().parent
DEST = ROOT / 'tools/media-qualification/evidence/2026-10-26-general-split-research'
assert not DEST.exists(), 'Never overwrite an evidence collection'
DEST.mkdir(parents=True)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def compressed(source, target):
    data = source.read_bytes()
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(gzip.compress(data, mtime=0))
    return {'original_bytes': len(data), 'original_sha256': sha(data)}


def copied(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)


expansions = {}
runs = []
for source in sorted(SCRATCH.glob('*.json')):
    record = json.loads(source.read_text())
    if not isinstance(record, dict) or 'source_before' not in record or 'command' not in record:
        continue
    assert 'exit_code' in record, ('running command', source)
    name = source.stem
    rows = re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored',
                      source.with_suffix('.stdout').read_text())
    runs.append({'name': name, 'exit_code': record['exit_code'],
                 'sources_unchanged': record['sources_unchanged'],
                 'seconds': record['seconds'], 'suites': len(rows),
                 'passed': sum(int(row[1]) for row in rows),
                 'failed': sum(int(row[2]) for row in rows),
                 'ignored': sum(int(row[3]) for row in rows)})
    for suffix in ['.json', '.stdout', '.stderr']:
        source = SCRATCH / (name + suffix)
        target = DEST / 'runs' / (source.name + '.gz')
        expansions[str(target.relative_to(DEST))] = compressed(source, target)

for name in ['retain-research.py', 'run.py', 'source-files.json', 'removed-files.json', 'candidate-tracked.patch',
             'candidate-new-files.txt', 'trace-plan-work.patch', 'trace-shared-input.patch',
             'trace-input-comparison.json', 'current-core-tests.json',
             'stages-precache-fifteenth.json', 'stages-trace-inputs-sixteenth.json']:
    source = SCRATCH / name
    assert source.is_file(), name
    copied(source, DEST / 'provenance' / name)

for name in (SCRATCH / 'candidate-new-files.txt').read_text().splitlines():
    assert name.startswith(('crates/deadpan-core/', 'crates/deadpan-plan/', 'crates/deadpan-audio/'))
    copied(ROOT / name, DEST / 'candidate' / name)

for source in sorted((SCRATCH / 'trace-inputs').iterdir()):
    target = DEST / 'trace-inputs' / (source.name + '.gz')
    expansions[str(target.relative_to(DEST))] = compressed(source, target)

strict = json.loads((SCRATCH / 'strict-postcache-seventeenth.json').read_text())
assert all(sha(Path(path).read_bytes()) == pin['sha256']
           for path, pin in strict['source_before'].items())
summary = {
    'kind': 'research_checkpoint_not_product_qualification',
    'completion_credit': 'none',
    'delivery_gates': {'closed': 0, 'total': 7, 'status': 'all remain open'},
    'base_revision': 'c99d04048f9f3cf4ba9554d573e72af9348a35ce',
    'candidate': 'Apply provenance/candidate-tracked.patch to an isolated copy of the base, then overlay candidate/. Production changes remain uncommitted.',
    'final_candidate_matches_all_strict17_source_pins': True,
    'source_files': len(strict['source_before']),
    'runs': runs,
    'expansions': expansions,
    'known_failures': [
        'Current clean seven-case PCM run: two pass and five fail; three preparation-work failures and two exact-PCM failure tests.',
        'Source/Sequence first Split differs at authored stereo sample 5977: expected [0.94367766,-0.99999905], actual [0.9436776,-0.99999905].',
        'Exact root EQ input shows the right-fragment sound excluded beginning at Split sample 8008; the input tile ends at 8192.',
        'Per-Source sound processing scopes retain no Split alias certificate for delivered continuity at ancestor/root temporal processing.',
    ],
    'limitations': [
        'core-sound-regression-fourth executed the earlier Core test binary from core-development-third; it is not current-source verification despite unchanged source/binary guards during that direct execution.',
        'No native scoped Split, emitted-media, Store, app, release, latency or full-goal closure qualification is claimed.',
        'Only the final clean candidate source patch/new files are retained. Older failed iterations retain raw logs/source pins, not every complete intermediate source body.',
        'audio-trace-inputs-sixteenth changed only the cfg(test) module path annotation during the run. The ordinary dependency excludes that test module; raw pins identify the exact change. The clean seventeenth run independently reproduces the PCM failure.',
        'Test executables are omitted from this published collection. Locally retained executable paths and SHA-256 pins are in provenance; source snapshots and build/test commands are retained.',
    ],
}
(DEST / 'SUMMARY.json').write_text(json.dumps(summary, indent=2) + '\n')
(DEST / 'README.md').write_text('''# General Split sound research checkpoint

This is an incomplete research checkpoint, not product qualification. See
SUMMARY.json for exact passing and failing runs and important limitations.

Run receipts retain invocation, environment, source pins, streams, exit and
elapsed time. Compressed files use deterministic gzip; SUMMARY.json records
their expanded hashes and sizes. The final candidate patch plus nine new source
files reconstruct the clean candidate from the named Git base. Use a separate
checkout/copy and Cargo target for a source comparison. Never reset this shared
checkout to reproduce an older state.

The trace comparison proves a missing delivered sound contribution in a root
temporal input, rather than acceptable floating-point variation. Do not relax
PCM equality or widen processing-history scope to conceal that failure. Delivery
continuity needs its own checked authority, preserving current source admission,
per-route support and silence permissions. All seven delivery gates stay open.
''')
payloads = {}
for path in sorted(DEST.rglob('*')):
    if path.is_file():
        data = path.read_bytes()
        payloads[str(path.relative_to(DEST))] = {'bytes': len(data), 'sha256': sha(data)}
(DEST / 'INVENTORY.json').write_text(json.dumps(payloads, indent=2) + '\n')
inventory_sha = sha((DEST / 'INVENTORY.json').read_bytes())
(DEST / 'INVENTORY.sha256').write_text(inventory_sha + '  INVENTORY.json\n')
print(json.dumps({'path': str(DEST), 'payloads': len(payloads), 'runs': len(runs),
                  'stored_payload_bytes': sum(pin['bytes'] for pin in payloads.values()),
                  'inventory_sha256': inventory_sha}))
