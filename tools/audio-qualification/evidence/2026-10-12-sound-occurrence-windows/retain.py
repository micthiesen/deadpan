import gzip, hashlib, json, re, shutil, subprocess
from pathlib import Path
s = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/audio-qualification/evidence/2026-10-12-sound-occurrence-windows'
out.mkdir(parents=True, exist_ok=True)
def sha(path):
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()
def write(name, value):
    (out / name).write_text(json.dumps(value, indent=2) + '\n')
records = []
for path in sorted(s.glob('*.json')):
    record = json.loads(path.read_text())
    if isinstance(record, dict) and 'log_sha256' in record:
        assert sha(s / (record['name'] + '.log')) == record['log_sha256']
        records.append(record)
write('commands.json', records)
for path in sorted(s.glob('*.log')):
    (out / (path.name + '.gz')).write_bytes(gzip.compress(path.read_bytes(), mtime=0))
base = subprocess.check_output(['git', 'rev-parse', '35f526b7'], cwd=repo, text=True).strip()
changed = subprocess.check_output(['git', 'diff', '--name-only', base, '--', 'crates'], cwd=repo, text=True).splitlines()
new = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard', '--', 'crates'], cwd=repo, text=True).splitlines()
sources = sorted(set(changed + new + ["crates/deadpan-audio/tests/audio_definition.rs", "crates/deadpan-audio/tests/audio_definition/voice_occurrence.rs", 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates/deadpan-audio/src/routed.rs', 'crates/deadpan-audio/src/retained_sound_input.rs', 'native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav']))
write('identity.json', dict(base_commit=base, sources={name: sha(repo/name) for name in sources}, binaries={name: sha(repo/name) for name in ['target/release/deadpan-app', 'target/release/deadpan-cli']}))
patch = subprocess.check_output(['git', 'diff', base, '--', 'crates'], cwd=repo)
for name in new:
    patch += subprocess.run(['git', 'diff', '--no-index', '--', '/dev/null', name], cwd=repo, capture_output=True).stdout
(out / 'implementation.patch').write_bytes(patch)
for name in ['run.py', 'checks.py', 'retain.py', 'check-evidence.py', 'README.md', 'review.md']:
    shutil.copyfile(s/name, out/name)
results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (s/'plan-audio-playback.log').read_text())
assert results and not any(int(row[1]) for row in results)
write('test-counts.json', dict(passed=sum(int(row[0]) for row in results), failed=0, ignored=sum(int(row[2]) for row in results), result_groups=len(results)))
write('environment.json', dict(machine='Apple M5 Max', physical_cores=18, memory_bytes=137438953472, macos='26.5.2 (25F84)', rust='1.97.1', agents='primary only', live_gui=False, physical_device=False))
write('manifest.json', {path.name: sha(path) for path in sorted(out.iterdir()) if path.is_file() and path.name != 'manifest.json'})
print('Retained', len(list(out.iterdir())), 'files')
