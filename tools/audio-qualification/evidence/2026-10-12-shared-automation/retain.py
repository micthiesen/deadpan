"""Retain complete command output and measured preview/export evidence."""
from pathlib import Path
import gzip
import hashlib
import io
import json
import re
import shutil
import subprocess
import tarfile

scratch = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/audio-qualification/evidence/2026-10-12-shared-automation'
out.mkdir(parents=True, exist_ok=True)

def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()

def write(name, value):
    (out / name).write_text(json.dumps(value, indent=2) + '\n')

def binaries(name):
    found = {}
    for line in (scratch / (name + '.log')).read_text().splitlines():
        if line.startswith('{'):
            data = json.loads(line)
            executable = data.get('executable')
            if data.get('reason') == 'compiler-artifact' and executable:
                found[executable] = digest(Path(executable))
    assert found, name
    return found

records = []
for receipt in sorted(scratch.glob('*.json')):
    data = json.loads(receipt.read_text())
    if not isinstance(data, dict) or 'log_sha256' not in data:
        continue
    log = scratch / (data['name'] + '.log')
    assert digest(log) == data['log_sha256'], log
    records.append(data)
    (out / (log.name + '.gz')).write_bytes(gzip.compress(log.read_bytes(), mtime=0))
write('commands.json', records)
for number in range(1, 6):
    path = Path('/tmp/deadpan-shared-components-focus' + str(number) + '.log')
    (out / ('initial-focus-' + str(number) + '.log.gz')).write_bytes(gzip.compress(path.read_bytes(), mtime=0))

base = json.loads((scratch / 'baseline-source.json').read_text())['revision']
changed = subprocess.check_output(['git', 'diff', '--name-only', base, '--', 'crates'], cwd=repo, text=True).splitlines()
changed.append('crates/deadpan-audio/src/shared_components.rs')
sources = changed + ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
    'crates/deadpan-audio/tests/audio_definition.rs',
    'crates/deadpan-audio/tests/audio_definition/sound_clocks.rs',
    'crates/deadpan-core/src/audio_reference.rs',
    'crates/deadpan-core/src/sound_clock/edit.rs',
    'native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav']
write('identity.json', dict(base_commit=base, sources={name: digest(repo / name) for name in sources}, binaries={name: binaries(name) for name in ['focus-final', 'suites-final2', 'export-source']}))
write('baseline-binaries.json', binaries('baseline-final'))
patch = subprocess.check_output(['git', 'diff', base, '--', *changed], cwd=repo)
if subprocess.run(['git', 'ls-files', '--error-unmatch', changed[-1]], cwd=repo, capture_output=True).returncode:
    patch += subprocess.run(['git', 'diff', '--no-index', '--', '/dev/null', changed[-1]], cwd=repo, capture_output=True, check=False).stdout
(out / 'implementation.patch').write_bytes(patch)
for name in ['baseline-source.json', 'run.py', 'baseline.py', 'finish_checks.py', 'retain.py', 'check-evidence.py', 'review.md']:
    shutil.copyfile(scratch / name, out / name)
write('environment.json', dict(machine='Apple M5 Max', physical_cores=18, memory_bytes=137438953472, architecture='arm64', macos='26.5.2 (25F84)', rust='rustc 1.97.1 (8bab26f4f 2026-07-14)', ffmpeg_prefix='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix', agents='primary agent only', live_gui=False, device_run=False))

export = json.loads((scratch / 'export-source-results.json').read_text())
assert export['report']['passed'] and not export['report']['failures']
write('export-results.json', export)
movie = Path(export['movie'])
shutil.copyfile(movie, out / 'shared-automation.mp4')
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
            item = archive.gettarinfo(str(path), arcname='shared-automation.deadpan/' + str(path.relative_to(package)))
            item.uid = item.gid = item.mtime = 0
            item.uname = item.gname = ''
            with path.open('rb') as source:
                archive.addfile(item, source)
(out / 'shared-automation.deadpan.tar.gz').write_bytes(gzip.compress(stream.getvalue(), mtime=0))
suite = (scratch / 'suites-final2.log').read_text()
assert not re.search(r'test result: FAILED|; [1-9][0-9]* (?:failed|ignored);', suite)
baseline = (scratch / 'baseline-final.log').read_text()
counts = re.search(r'test result: FAILED\. (\d+) passed; (\d+) failed; 0 ignored;', baseline)
assert counts
write('results.json', dict(suite_passed=sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', suite))), focused_passed=15, baseline_passed=int(counts[1]), baseline_expected_failures=int(counts[2]), export_tests=1, summary=export['report']['summary']))
write('manifest.json', {p.name: digest(p) for p in sorted(out.iterdir()) if p.is_file() and p.name != 'manifest.json'})
print('Retained', len(list(out.iterdir())), 'files in', out)
