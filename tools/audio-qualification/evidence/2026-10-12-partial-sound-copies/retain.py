import gzip, hashlib, json, re, shutil, subprocess
from pathlib import Path

scratch = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/audio-qualification/evidence/2026-10-12-partial-sound-copies'
out.mkdir(parents=True, exist_ok=True)

def sha(path):
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()

def write(name, value):
    (out / name).write_text(json.dumps(value, indent=2) + '\n')

records = []
for path in sorted(scratch.glob('*.json')):
    record = json.loads(path.read_text())
    if isinstance(record, dict) and 'log_sha256' in record:
        assert sha(scratch / (record['name'] + '.log')) == record['log_sha256']
        records.append(record)
write('commands.json', records)
for path in sorted(scratch.glob('*.log')):
    (out / (path.name + '.gz')).write_bytes(gzip.compress(path.read_bytes(), mtime=0))
base = subprocess.check_output(['git', 'rev-parse', 'b041989e'], cwd=repo, text=True).strip()
changed = subprocess.check_output(['git', 'diff', '--name-only', base, '--', 'crates'], cwd=repo, text=True).splitlines()
new = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard', '--', 'crates'], cwd=repo, text=True).splitlines()
fixtures = ['native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav', 'native/deadpan-source/tests/fixtures/cfr-bframes.mp4', 'native/deadpan-source/tests/fixtures/offset-bframes.mp4']
sources = sorted(set(changed + new + fixtures + ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml']))
write('identity.json', dict(base_commit=base, sources={name: sha(repo/name) for name in sources}, binaries={name: sha(repo/name) for name in ['target/release/deadpan-app', 'target/release/deadpan-cli']}))
patch = subprocess.check_output(['git', 'diff', base, '--', 'crates'], cwd=repo)
for name in new:
    patch += subprocess.run(['git', 'diff', '--no-index', '--', '/dev/null', name], cwd=repo, capture_output=True).stdout
(out / 'implementation.patch').write_bytes(patch)
for name in ['run.py', 'checks.py', 'retain.py', 'check-evidence.py', 'README.md', 'review.md', 'export-results.json']:
    shutil.copyfile(scratch/name, out/name)
export = json.loads((scratch/'export-results.json').read_text())
movie = Path(export['movie'])
shutil.copyfile(movie, out/'partial-sounds.mp4')
shutil.make_archive(str(out/'project'), 'zip', root_dir=Path(export['package']).parent)
write('export-artifact.json', dict(original_path=str(movie), sha256=sha(movie), bytes=movie.stat().st_size))
results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (scratch/'all-audio-core-store.log').read_text())
assert results and not any(int(row[1]) for row in results)
write('test-counts.json', dict(passed=sum(int(row[0]) for row in results), failed=0, ignored=sum(int(row[2]) for row in results), result_groups=len(results)))
write('environment.json', dict(machine='Apple M5 Max', physical_cores=18, memory_bytes=137438953472, macos='26.5.2 (25F84)', rust='1.97.1', agents='primary only', live_gui=False, physical_device=False))
write('manifest.json', {path.name: sha(path) for path in sorted(out.iterdir()) if path.is_file() and path.name != 'manifest.json'})
print('Retained', len(list(out.iterdir())), 'files')
