"""Bounded non-GUI evidence collection; root must explicitly authorize final run.

Run only after all required checks pass:
  python /tmp/deadpan-fixed-retime-owned-sound-2026-10-21/collect.py --confirmed-final

Root writes runtime-results.json before collection:
  {"passed":true,"verified_by":"root","runs":{
    "focused-core":{"log":"focused-core.log","passed":13},
    "focused-pcm":{"log":"focused-pcm.log","passed":6},
    "focused-store":{"log":"focused-store.log","passed":2},
    "broad":{"log":"broad.log","passed":ACTUAL},
    "encoded":{"log":"encoded.log","passed":1}},
   "binaries":{"encoded-cli":{"path":"...","sha256":"..."},
               "encoded-test":{"path":"...","sha256":"..."},
               "final-core":{"path":"...","sha256":"..."},
               "final-pcm":{"path":"...","sha256":"..."},
               "final-store":{"path":"...","sha256":"..."}}}

Use copied executable paths if a later Cargo graph replaced an executed file.
No hash from the earlier setter milestone establishes this run's identity.
"""
from pathlib import Path
import gzip
import hashlib
import json
import re
import sys

SCRATCH = Path('/tmp/deadpan-fixed-retime-owned-sound-2026-10-21')
OUTPUT = Path('tools/media-qualification/evidence/2026-10-21-fixed-retime-output-sound')
OLD_BINARY = '0ce77dde3a67df13925e3deefaff8b277c75872ce356430ef09571c236f089bf'
FIRST_BINARY = 'be72697af5825287937d4098dca2f78a7f28d5609f5e65f6470898d7e4de40cc'
MEDIA = {
    'original': ('native/deadpan-source/tests/fixtures/cfr-bframes.mp4',
                 '5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918'),
    'event': ('native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav',
              'b95b3debd2c83cf2590fddfaa98276c23c3b383b720fd96ac52c2c00c668318b'),
}

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def read(name):
    return json.loads((SCRATCH / name).read_text())

assert sys.argv[1:] == ['--confirmed-final'], 'Root authorization and final runtime results required'
runtime = read('runtime-results.json')
assert runtime['passed'] is True and runtime['verified_by'] == 'root'
for name in ['focused-core', 'focused-pcm', 'focused-store', 'broad', 'encoded']:
    run = runtime['runs'][name]
    text = (SCRATCH / run['log']).read_text()
    assert run['passed'] > 0, (name, 'missing actual pass count')
    summary = next(line for line in reversed(text.splitlines()) if 'Summary [' in line)
    assert re.search(rf"\b{run['passed']} tests? run: {run['passed']} passed\b", summary), (name, summary)
    assert ' failed' not in summary, (name, summary)
for name in ['encoded-cli', 'encoded-test', 'final-core', 'final-pcm', 'final-store']:
    identity = runtime['binaries'][name]
    assert re.fullmatch('[a-f0-9]{64}', identity['sha256']), name
    assert sha(identity['path']) == identity['sha256'], (name, 'executed binary overwritten; use retained copy')

assert sha(SCRATCH / 'old-core-test') == OLD_BINARY
assert sha(SCRATCH / 'first-core-test') == FIRST_BINARY
old = read('old-identities.json')
assert old['base_revision'] == '072f0d82' and old['sha256'] == OLD_BINARY
assert (old['tests'], old['positive_failed'], old['negative_passed']) == (7, 6, 1)
assert (SCRATCH / 'old-witness.exit').read_text().strip() == '100'
assert (SCRATCH / 'old-witness-compile-first.exit').read_text().strip() == '101'
assert read('first-core-binary.json')['sha256'] == FIRST_BINARY
assert '12 tests run: 10 passed, 2 failed' in (SCRATCH / 'focused-core-first.log').read_text()
store_first = (SCRATCH / 'focused-store-first.log').read_text()
assert '2 tests run: 0 passed, 2 failed' in store_first
assert store_first.count('source path must be absolute, bounded, and have no parent traversal') == 2
encoded_first = (SCRATCH / 'encoded-first.log').read_text()
assert '1 test run: 0 passed, 1 failed' in encoded_first
assert 'String("reject")' in encoded_first and '"hold_adjacent"' in encoded_first
first_encoded_ids = read('encoded-first-binaries.json')
assert first_encoded_ids['encoded_first_cli']['sha256'] == 'fe41efd48eba3d5a9b2f2e0de1ffd64f5e090a5942b72d8e4b1ae20c33d21826'
assert first_encoded_ids['encoded_first_test']['sha256'] == 'e0dd28d606aeaf25e531a41b6612db2b5fac7acc11d7383d995d0cb3893ea09b'
sources = read('source-identities.json')
for path, expected in sources.items():
    assert sha(path) == expected, ('source changed after verification', path)
for name, (path, expected) in MEDIA.items():
    assert sha(path) == expected, ('measured media identity', name)

encoded = read('encoded-result.json')
report = encoded['report']
assert report['passed'] is True and report['failures'] == []
assert report['summary']['pictures_checked'] == 11 and report['summary']['pictures_failed'] == 0
assert len(report['pictures']) == 11
assert report['movie']['video_frames'] == report['movie']['expected_video_frames'] == 11
assert report['movie']['audio']['presented_end'] == 17618
assert report['movie']['audio_edits']['presented_media_ticks'] == 17618
assert report['movie']['expected_audio_samples'] == 17618
assert report['movie']['picture_anomalies'] == [] and report['movie']['audio']['gaps'] == []
assert report['summary']['nonzero_offsets'] == 0
assert report['summary']['signal_windows'] == 2 and report['summary']['audio_windows_checked'] == 2
assert len(report['audio']) == 2
end = 0
for window in report['audio']:
    assert window['window'][0] == end
    end = window['window'][1]
    assert window['project_samples'] == window['window']
    assert window['passed'] is True and window['flags'] == [] and window['kind'] == 'signal'
    assert window['offset_status'] == 'verified_zero' and window['measured_offset_samples'] == 0
    assert window['blocks']['low_snr_blocks'] == 0
assert end == 17618
package = Path(encoded['package'])
oracle = package.parent / 'output-owned-event-oracle.json'
oracle_data = json.loads(oracle.read_text())
assert oracle_data['event_native_samples'] == [3100, 8005]
assert oracle_data['mix_support'] == [6443, 11782] and oracle_data['event_samples'] == 5339
assert oracle_data['retained_native_phase'] == '2480147/800'
assert oracle_data['source_step'] == '147/160'
assert oracle_data['phase_control_max_difference'] > 0.001
assert all(value < 0.000002 for value in oracle_data['maximum_event_errors_before_grow_shrink'])
movie = Path(encoded['movie'])
retained_original = package.parent / 'original.mp4'
assert sha(retained_original) == MEDIA['original'][1]

OUTPUT.mkdir(exist_ok=True)
def keep(source, name=None, compress=False):
    source = Path(source)
    content = source.read_bytes()
    name = name or source.name
    destination = OUTPUT / name
    if compress:
        content = gzip.compress(content, mtime=0)
        destination = OUTPUT / (name + '.gz')
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(content)

extensions = {'.log', '.json', '.sh', '.txt', '.diff', '.exit', '.py', '.sha256', '.patch'}
for path in sorted(SCRATCH.iterdir()):
    if path.is_file() and path.suffix in extensions:
        keep(path, compress=path.suffix in {'.log', '.diff', '.patch'}
             or (path.suffix == '.json' and any(word in path.stem for word in ['report', 'result']))
             or path.stat().st_size > 65536)
keep(SCRATCH / 'design.md', 'design.md', True)
# Preserve exact old source directory names, including pre-registration base.
for path in sorted((SCRATCH / 'old-witness').rglob('*')):
    if path.is_file():
        keep(path, str(path.relative_to(SCRATCH)), compress=path.suffix == '.rs')
for directory in ['pcm', 'store-export']:
    for path in sorted((SCRATCH / directory).rglob('*')):
        if path.is_file() and path.suffix in extensions | {'.rs'}:
            keep(path, str(path.relative_to(SCRATCH)), compress=path.suffix == '.rs' or path.stat().st_size > 65536)
for path in sources:
    if path.endswith('.rs'):
        keep(path, path.replace('/', '__'), True)
keep(oracle, 'output-owned-event-oracle.json')

provenance = {
    'scope': 'fixed Retime output-owned event; no DSP/schema/UI change or native interaction claim',
    'measured_sources': {name: {'path': path, 'sha256': expected, 'bytes': Path(path).stat().st_size}
                         for name, (path, expected) in MEDIA.items()},
    'movie': {'path': str(movie), 'sha256': sha(movie), 'bytes': movie.stat().st_size},
    'project': {'path': str(package), 'project_id': report['project_id'],
                'revision_id': report['revision_id'], 'document_sha256': report['document_sha256']},
    'policy': 'Reports, literal phase oracle and measured media identities retained; raw media, package and executables omitted.',
}
(OUTPUT / 'media-provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
summary = {'encoded': report['summary'], 'presented_samples': 17618, 'pictures': 11,
           'runtime': runtime, 'old_binary': OLD_BINARY, 'first_core_binary': FIRST_BINARY}
(OUTPUT / 'verification-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
manifest = {str(path.relative_to(OUTPUT)): {'bytes': path.stat().st_size, 'sha256': sha(path)}
            for path in sorted(OUTPUT.rglob('*')) if path.is_file() and path.name != 'inventory.json'}
total = sum(item['bytes'] for item in manifest.values())
assert total < 8 * 1024 * 1024, ('Unexpectedly large evidence collection', total)
(OUTPUT / 'inventory.json').write_text(json.dumps(manifest, indent=2) + '\n')
for name, item in manifest.items():
    assert sha(OUTPUT / name) == item['sha256'], name
print(f'{len(manifest)} evidence files, {total} bytes; inventory verified')
