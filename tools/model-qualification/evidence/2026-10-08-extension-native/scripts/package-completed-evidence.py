"""Retain completed native Extension evidence without reading live model outputs."""
from pathlib import Path
import ast
import gzip
import hashlib
import json
import re

ROOT = Path(__file__).resolve().parent
DEST = Path('/Users/michael/Code/deadpan/tools/model-qualification/evidence/2026-10-08-extension-native')
DEST.mkdir(parents=True, exist_ok=False)
copied = []
references = []


def sha(data):
    return hashlib.sha256(data).hexdigest()


def stable_bytes(path):
    before = path.stat()
    data = path.read_bytes()
    after = path.stat()
    assert (before.st_ino, before.st_size, before.st_mtime_ns) == (after.st_ino, after.st_size, after.st_mtime_ns), path
    assert len(data) == before.st_size, path
    return data


def review_text(data, name):
    text = data.decode('utf-8')
    forbidden = [r'-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----',
                 r'\bhf_[A-Za-z0-9]{24,}\b', r'\bgh[pousr]_[A-Za-z0-9]{24,}\b',
                 r'\bsk-[A-Za-z0-9]{24,}\b', r'(?i)authorization:\s*(?:bearer|basic)\s+\S+']
    assert not any(re.search(pattern, text) for pattern in forbidden), f'credential marker in {name}'
    return text


def write_new(relative, data):
    path = DEST / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('xb') as stream:
        stream.write(data)


def save_json(relative, value):
    data = (json.dumps(value, indent=2, allow_nan=False) + '\n').encode()
    review_text(data, relative)
    write_new(relative, data)


def retain(source, relative, compress=False):
    path = ROOT / source
    data = stable_bytes(path)
    review_text(data, source)
    if path.suffix == '.json':
        json.loads(data)
    if path.suffix == '.py':
        ast.parse(data, filename=source)
    write_new(relative, gzip.compress(data, compresslevel=9, mtime=0) if compress else data)
    copied.append({'retained': relative, 'source': str(path), 'source_bytes': len(data),
                   'source_sha256': sha(data), 'encoding': 'gzip' if compress else 'verbatim'})


logs = ['catalog-fuzz', 'doc-tests', 'final-clippy', 'final-clippy-2',
        'format', 'format-2', 'format-3', 'gain-pipe-check', 'python-tests',
        'retained-mode-test', 'retained-mode-test-2', 'runtime-clippy',
        'runtime-tests', 'ui-tests', 'bundle', 'bundle-2', 'bundle-verify',
        'bundle-verify-2', 'install-extension', 'replays', 'replays-2']
for name in logs:
    retain(name + '.log', 'logs/' + name + '.log.gz', True)
for name in ['verification-results.json', 'remaining-results.json', 'executed-binary.json', 'cases.json']:
    retain(name, name)
for name in ['qualify.py', 'oracle.py', 'verify-accepted.py', 'summarize-results.py',
             'verify.py', 'verify-remaining.py', 'verify-runtime-fix.py', 'package-completed-evidence.py']:
    retain(name, 'scripts/' + name)
retain('bundle-2-source.json', 'source/bundle-2-source.json.gz', True)
retain('bundle-2/Deadpan.app/Contents/Resources/build-provenance.json', 'bundle/build-provenance.json')

# This aggregate is still being appended. Retain only immutable completed-command
# records that precede the running matrix, never the partial aggregate itself.
runtime_records = json.loads((ROOT / 'runtime-fix-results.json').read_text())
expected = ['format-3', 'runtime-clippy', 'runtime-tests', 'gain-pipe-check',
            'bundle-2', 'bundle-verify-2', 'install-extension']
completed = [record for record in runtime_records if record['name'] in expected]
assert [record['name'] for record in completed] == expected
assert all(record['exit_code'] == 0 for record in completed)
save_json('completed-runtime-checks.json', {
    'scope': 'Completed pre-matrix commands only; later matrix/oracle/acceptance records are intentionally excluded.',
    'source': str(ROOT / 'runtime-fix-results.json'),
    'commands': completed,
})

replay_metadata = None
for batch in ['replays', 'replays-2']:
    retain(batch + '/summary.json', batch + '/summary.json')
    for path in sorted((ROOT / batch).glob('*/report.json')):
        raw = stable_bytes(path)
        review_text(raw, str(path))
        report = json.loads(raw)
        replay_metadata = report['metadata']
        reference = {'path': str(path), 'bytes': len(raw), 'sha256': sha(raw),
                     'retained': False, 'reason': 'Full per-frame replay trace; compact complete checks are retained.'}
        references.append(reference)
        excerpt = {key: value for key, value in report.items() if key != 'scenarios'}
        excerpt['scenarios'] = [{key: value for key, value in scenario.items() if key != 'steps'}
                                for scenario in report['scenarios']]
        excerpt['evidence_extraction'] = {
            'source': reference,
            'omitted': 'scenarios[*].steps only; metadata, every check, findings, skipped cases and timings retained.',
            'steps_omitted': {scenario['name']: len(scenario['steps']) for scenario in report['scenarios']},
        }
        data = (json.dumps(excerpt, indent=2, allow_nan=False) + '\n').encode()
        write_new(batch + '/' + path.parent.name + '.checks.json.gz', gzip.compress(data, compresslevel=9, mtime=0))

for name in ['bundle-source.json', 'bundle-source.patch', 'bundle-2-source.patch']:
    path = ROOT / name
    data = stable_bytes(path)
    references.append({'path': str(path), 'bytes': len(data), 'sha256': sha(data),
                       'retained': False, 'reason': 'Prior source inventory or source patch retained locally; final inventory is archived separately.'})

for batch in ['replays', 'replays-2']:
    summary = json.loads((ROOT / batch / 'summary.json').read_text())
    for name, binary in summary['binaries'].items():
        references.append({'path': binary['path'], 'bytes': Path(binary['path']).stat().st_size,
                           'sha256': binary['sha256'], 'sha256_source': batch + '/summary.json',
                           'retained': False, 'reason': 'Executed replay binary; identity from the original runner receipt.'})
binary = json.loads((ROOT / 'executed-binary.json').read_text())
references.append({**binary, 'bytes': Path(binary['path']).stat().st_size,
                   'sha256_source': 'executed-binary.json', 'retained': False,
                   'reason': 'Executed packaged CLI; identity from the generation runner receipt.'})

save_json('environment.json', {
    'source': str(ROOT / 'replays-2/model-packs/report.json'),
    **{key: replay_metadata[key] for key in ['hardware', 'os', 'rust', 'fixture', 'fixture_sha256',
                                             'power_thermal_state', 'physical_display_measured']},
    'packaged_build_metadata': 'bundle/build-provenance.json',
    'capture_note': 'Existing replay/build records; no new system or native probes were run to package evidence.',
})
save_json('inventory.json', {
    'scope': 'Completed automated checks, replays and bundle checks; real matrix/oracle/acceptance outputs pending separate collection.',
    'scratch_root': str(ROOT), 'copied': copied,
    'local_only': references,
    'intentionally_not_retained': [
        'results.json and measurement-summary.json: live incomplete real-model result aggregates',
        'real-matrix.log and runtime-fix.log: active process logs',
        '*f-from_*/: model matrices and generated media under active qualification',
        'oracle-results.json and accepted-verification/: pending final outputs owned by the root agent',
        'Complete app bundles, model weights, caches, replay media and private signing keys',
        'bakeoff-recipe.txt and warm-runtime-recipe.txt: unrelated next-slice research',
    ],
    'script_output_status': 'Copied scripts are reproducible source only; their real matrix, oracle and acceptance outputs are not claimed by this collection.',
    'review': 'Retained text and scripts reviewed for task relevance; credential marker scan passed. No private key or model/media file copied.',
})
print(json.dumps({'destination': str(DEST), 'files': len(list(DEST.rglob('*'))),
                  'bytes': sum(path.stat().st_size for path in DEST.rglob('*') if path.is_file())}))
