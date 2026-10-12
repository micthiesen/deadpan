"""Retain this run's exact logs, identities and verified export."""
import gzip
import hashlib
import io
import json
import re
import shutil
import subprocess
import tarfile
from pathlib import Path

scratch = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/audio-qualification/evidence/2026-10-12-input-automation'
out.mkdir(parents=True, exist_ok=True)

def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()

def write(name, value):
    (out / name).write_text(json.dumps(value, indent=2) + '\n')

def compressed(name, source):
    (out / (name + '.log.gz')).write_bytes(gzip.compress(source.read_bytes(), mtime=0))

records = []
for name in ['baseline', 'clippy', 'export', 'final-focus', 'fmt', 'whitespace']:
    record = json.loads((scratch / (name + '.json')).read_text())
    assert record['exit'] == (101 if name == 'baseline' else 0), name
    assert record['log_sha256'] == digest(scratch / (name + '.log')), name
    records.append(record)
    compressed(name, scratch / (name + '.log'))
suite = Path('/tmp/deadpan-independent-gain-suites1.log')
suite_text = suite.read_text()
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', suite_text))) == 973
assert not re.search(r'test result: FAILED|; [1-9][0-9]* (?:failed|ignored);', suite_text)
records.insert(0, dict(name='suites', command=['cargo', 'test', '--locked', '-p', 'deadpan-audio', '-p', 'deadpan-plan'], cwd=str(repo), exit=0, log_sha256=digest(suite), note='Original harness exit 0; elapsed time not retained. Only production comments changed before the final focused run.'))
compressed('suites', suite)
for number in [1, 2, 3]:
    compressed('initial-focus-' + str(number), Path('/tmp/deadpan-independent-gain-focus' + str(number) + '.log'))
write('commands.json', records)

identity = json.loads((scratch / 'identity.json').read_text())
identity.pop('test_binary')
sources = list(identity['sources']) + [
    'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
    'crates/deadpan-audio/src/shared_eq.rs',
    'crates/deadpan-audio/src/authored_gain.rs',
    'crates/deadpan-audio/tests/audio_definition.rs',
    'crates/deadpan-audio/tests/audio_definition/sound_clocks.rs',
    'native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav',
    'crates/deadpan-cli/tests/preview_export.rs',
    'crates/deadpan-cli/tests/preview_export/recipes.rs',
]
identity['sources'] = {name: digest(repo / name) for name in sources}
identity['binaries'] = {}
for name in ['final-focus', 'export']:
    binaries = {}
    for line in (scratch / (name + '.log')).read_text().splitlines():
        if not line.startswith('{'):
            continue
        data = json.loads(line)
        binary = data.get('executable')
        if data.get('reason') == 'compiler-artifact' and binary:
            binaries[binary] = digest(Path(binary))
    assert binaries, name
    identity['binaries'][name] = binaries
write('identity.json', identity)
for name in ['baseline-source.json', 'baseline-binaries.json', 'run.py', 'baseline.py', 'retain.py', 'check-evidence.py', 'review.md']:
    shutil.copyfile(scratch / name, out / name)
base = identity['base_commit']
changed = [name for name in sources if name in json.loads((scratch / 'identity.json').read_text())['sources']]
(out / 'implementation.patch').write_bytes(subprocess.check_output(['git', 'diff', base, '--', *changed], cwd=repo))
write('environment.json', dict(machine='Apple M5 Max', physical_cores=18, memory_bytes=137438953472, architecture='arm64', macos='26.5.2 (25F84)', rust='rustc 1.97.1 (8bab26f4f 2026-07-14)', ffmpeg_prefix='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix', agents='primary agent only', live_gui=False, device_run=False))

export = json.loads((scratch / 'export-results.json').read_text())
assert export['report']['passed'] and not export['report']['failures']
write('export-results.json', export)
movie = Path(export['movie'])
shutil.copyfile(movie, out / 'shared-equalizer.mp4')
receipts = list(movie.parent.glob('deadpan-render-*.json'))
assert len(receipts) == 1
shutil.copyfile(receipts[0], out / 'render-receipt.json')
package = Path(export['package'])
wal = package / 'project.sqlite-wal'
assert not wal.exists() or wal.stat().st_size == 0
stream = io.BytesIO()
with tarfile.open(fileobj=stream, mode='w') as archive:
    for path in sorted(package.rglob('*')):
        if path.is_file() and not path.name.endswith(('-shm', '-wal', '.lock')):
            item = archive.gettarinfo(str(path), arcname='shared-equalizer.deadpan/' + str(path.relative_to(package)))
            item.uid = item.gid = item.mtime = 0
            item.uname = item.gname = ''
            with path.open('rb') as data:
                archive.addfile(item, data)
(out / 'shared-equalizer.deadpan.tar.gz').write_bytes(gzip.compress(stream.getvalue(), mtime=0))
write('results.json', dict(suite_passed=973, focused_passed=7, baseline_passed=4, baseline_expected_failures=3, export_tests=1, summary=export['report']['summary']))
write('manifest.json', {p.name: digest(p) for p in sorted(out.iterdir()) if p.is_file() and p.name != 'manifest.json'})
print('Retained', len(list(out.iterdir())), 'files in', out)
