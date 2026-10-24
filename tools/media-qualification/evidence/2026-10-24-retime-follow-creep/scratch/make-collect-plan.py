#!/usr/bin/env python3
"""Freeze bounded Retime evidence after qualification and audits terminate."""
import hashlib
import json
from pathlib import Path

s = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
prior = Path('/tmp/deadpan-repeat-follow-creep-2026-10-24')


def pin(path):
    return {'bytes': path.stat().st_size,
            'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


for name, sha in [
    ('collect.py', '320e79dbf00568d68d7657e3c31d968402772533071529e7c9c494f9aacc6f0e'),
    ('bundle-helpers.py', '2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13'),
]:
    data = (prior / name).read_bytes()
    assert hashlib.sha256(data).hexdigest() == sha
    if (s / name).exists():
        assert (s / name).read_bytes() == data
    else:
        (s / name).write_bytes(data)

inputs = [
    'Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml', '.cargo/config.toml',
    'crates/deadpan-app/src/navigation/registry/baseline-routing.txt',
    'docs/COMMANDS.md', 'docs/TARGETS.md', 'docs/FRAMING.md',
    'native/deadpan-source/tests/fixtures/cfr-bframes.mp4',
]
excluded = {
    'collect-plan.json', 'collect.stdout', 'collect.stderr',
    'package-verification.json', 'package-verification.stdout',
    'package-verification.stderr', 'verify-retime-package.py',
    'publish-verification.py', 'next-retime-map.txt', 'next-retime-map.md',
}
files = [
    {'source': str(p), 'path': 'scratch/' + p.name + ('.txt' if p.suffix == '.rs' else '')}
    for p in sorted(s.iterdir()) if p.is_file() and p.name not in excluded
]
for d in sorted(s.iterdir()):
    if d.is_dir() and d.name.startswith('native-'):
        for name in ['report.json', 'report.html']:
            p = d / name
            if p.exists():
                files.append({'source': str(p), 'path': d.name + '/' + name})
for name in json.loads((s / 'visual-review.json').read_text())['images']:
    files.append({'source': str(s / name),
                  'path': 'images/' + name.replace('/', '-'), 'png': True})

archive_roots = [
    ('first-source', 'source-first'),
    ('worker-core-source', 'worker-core-1-source'),
    ('worker-cli-source', 'worker-cli-1-source'),
    ('native-worker-source', 'native-source'),
    ('plan-focused-source', 'plan-focused-1-source'),
    ('final-native-retime-project-and-export', 'media-final'),
    ('actual-native-retime-project',
     'native-retime-first/projects/retime-follow-creep/Documents/Deadpan/cfr-bframes.deadpan'),
    ('audit-history', 'audit-history'),
]
plan = {
    'schema_version': 1, 'repository': str(repo),
    'destination': 'tools/media-qualification/evidence/2026-10-24-retime-follow-creep',
    'raw_byte_limit': 256 * 1024 * 1024, 'gzip_threshold_bytes': 65536,
    'inputs': {name: pin(repo / name) for name in inputs},
    'post_run_sources': {}, 'files': files,
    'archives': [{'name': name, 'root': str(s / path)} for name, path in archive_roots],
    'limitations': [
        'Seventeen scoped final source paths and five sibling executable identities, not an attestation of every repository file.',
        'One release candidate. First/final source and all five binary bytes are identical; native-retime-first is final and is not counted twice.',
        'Focused Core receipts retain a concurrent global Plan integration-test draft change. All eight owned sources and nine named numeric/Plan/CLI basis sources are stable. CLI15 and Plan3 bind all17 current source files; final broad checks and native/media bind current17.',
        'Three focused Plan tests cover ten bounded cases,78 owner frames and40 neighbor frames, with all287 Core/Plan basis sources stable. This is not qualification of every nested Retime or gap structure.',
        'The first offline raw audit stopped on an auditor type assertion: empty global-missing lists were compared to numeric zero. Its exact script/receipt/streams are retained before the narrow correction and passing retry. No product or fixture failed and no product run was repeated.',
        'Native Retime uses production keys and manually authored/corrected points; no document construction, mocked detector or tracker is counted as native authoring.',
        'Offscreen Metal and scripted input do not establish physical display/input, OS IME delivery, VoiceOver speech, listening or latency.',
        'Export comparison uses the committed shared picture/DSP reference. Neighbor observability limits, warnings and existing opt-in ignores remain explicit.',
        'App unit executable identity was observed during execution and again after terminal tests; byte identity is not an archived executable or a pre-run attestation.',
        'Actual native and portable projects, complete immutable history and every-picture/full-audio verification retain explicit encoder-delay metadata.',
        'Collector output and later package-verification receipts remain separate to avoid recursive publication.',
    ],
}
raw = sum(Path(x['source']).stat().st_size for x in files)
raw += sum((repo / name).stat().st_size for name in inputs)
assert raw < 220 * 1024 * 1024, raw
with (s / 'collect-plan.json').open('x') as f:
    json.dump(plan, f, indent=2)
    f.write('\n')
print(json.dumps({'files': len(files), 'listed_raw_bytes': raw,
                  'collector_sha256': pin(s / 'collect.py')['sha256'],
                  'plan_sha256': pin(s / 'collect-plan.json')['sha256']}))
