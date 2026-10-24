#!/usr/bin/env python3
"""Select terminal evidence for the reviewed collector; no publication."""
import hashlib
import json
from pathlib import Path
import subprocess

scratch = Path(__file__).resolve().parent
root = Path('/Users/michael/Code/deadpan')


def identity(path):
    data = path.read_bytes()
    return {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


host = {'hardware': subprocess.check_output(['sysctl', '-n', 'hw.model'], text=True).strip(),
        'memory_bytes': int(subprocess.check_output(['sysctl', '-n', 'hw.memsize'], text=True)),
        'cores': int(subprocess.check_output(['sysctl', '-n', 'hw.ncpu'], text=True)),
        'os': subprocess.check_output(['sysctl', '-n', 'kern.osproductversion'], text=True).strip(),
        'hypervisor': int(subprocess.check_output(['sysctl', '-n', 'kern.hv_vmm_present'], text=True))}
with (scratch / 'host.json').open('x') as output:
    json.dump(host, output, indent=2)
    output.write('\n')

files = []
excluded = {'collect-plan.json', 'collect.stdout', 'collect.stderr', 'collect-receipt.json'}
for source in sorted(scratch.iterdir()):
    if source.is_file() and source.name not in excluded and source.suffix in {'.stdout', '.stderr', '.json', '.md', '.txt', '.py'}:
        files.append({'source': str(source), 'path': 'scratch/' + source.name})
for run in ['native-faces-first', 'native-targets-final', 'native-faces-final', 'native-targets-second', 'native-zoom-final']:
    receipt = json.loads((scratch / (run + '.json')).read_text())
    assert receipt['exit_code'] == 0 and receipt['sources_unchanged'] and receipt['binaries_unchanged']
    files.append({'source': str(scratch / run / 'report.json'), 'path': 'native/' + run + '.json'})
for run, name, target in [
    ('native-faces-first', 'faces-078.png', 'before-overlapping-badges.png'),
    ('native-faces-final', 'faces-079.png', 'camera-proposals-page-two.png'),
    ('native-faces-final', 'faces-121.png', 'committed-following-creep.png')]:
    files.append({'source': str(scratch / run / name), 'path': 'screenshots/' + target, 'png': True})
inputs = {name: identity(root / name) for name in [
    'Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml', '.cargo/config.toml',
    'crates/deadpan-app/src/navigation/registry/baseline-routing.txt', 'docs/COMMANDS.md',
    'native/deadpan-source/tests/fixtures/cfr-bframes.mp4']}
metadata = 'crates/deadpan-app/src/preview/harness/targets.rs'
plan = {'schema_version': 1, 'repository': str(root),
        'destination': 'tools/media-qualification/evidence/2026-10-24-camera-face-picker',
        'raw_byte_limit': 256 * 1024 * 1024, 'gzip_threshold_bytes': 65536,
        'inputs': inputs, 'post_run_sources': {metadata: identity(root / metadata)},
        'files': files, 'archives': [
            {'name': 'first-source', 'root': str(scratch / 'source-first')},
            {'name': 'final-camera-project-and-export', 'root': str(scratch / 'media-final')}],
        'limitations': [
            'Ten qualified sources plus one post-run diagnostic-string-only amendment; no all-tree compilation attestation.',
            'Positive face observations are scripted; real counter-fixture Vision no-face runs do not establish people/detector accuracy.',
            'Offscreen Metal UI with simulated audio, not physical input/display, OS IME, VoiceOver speech, listening or latency qualification.',
            'Export references the canonical shared picture/DSP implementation; no independent DSP oracle.',
            'Raw reports keep all warnings/skips, including the stale targets skip corrected only after the runs.',
            'Collector output and later package-verification receipts are retained separately, not recursively inside this immutable package.']}
with (scratch / 'collect-plan.json').open('x') as output:
    json.dump(plan, output, indent=2)
    output.write('\n')
print(json.dumps({'files': len(files), 'archives': len(plan['archives']),
                  'collector': identity(scratch / 'collect.py'), 'plan': identity(scratch / 'collect-plan.json')}))
