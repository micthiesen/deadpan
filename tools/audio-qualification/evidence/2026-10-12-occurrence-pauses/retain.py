import datetime
import gzip
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import zipfile

scratch = pathlib.Path(__file__).resolve().parent
repo = pathlib.Path('/Users/michael/Code/deadpan')
out = repo / 'tools/audio-qualification/evidence/2026-10-12-occurrence-pauses'
out.mkdir(parents=True, exist_ok=True)
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()

def write(name, value):
    (out / name).write_text(json.dumps(value, indent=2) + '\n')

records = []
counts = {}
for path in sorted(scratch.glob('*.json')):
    value = json.loads(path.read_text())
    if isinstance(value, dict) and 'log_sha256' in value:
        log = scratch / (value['name'] + '.log')
        assert sha(log) == value['log_sha256']
        records.append(value)
        groups = [tuple(map(int, match)) for match in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', log.read_text())]
        if groups:
            counts[value['name']] = dict(exit=value['exit'], passed=sum(group[0] for group in groups), failed=sum(group[1] for group in groups), ignored=sum(group[2] for group in groups))
write('commands.json', records)
write('test-counts.json', counts)
for path in scratch.glob('*.log'):
    (out / (path.name + '.gz')).write_bytes(gzip.compress(path.read_bytes(), mtime=0))

base = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
changed = subprocess.check_output(['git', 'diff', '--name-only', '--', 'crates', 'native'], cwd=repo, text=True).splitlines()
untracked = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard', '--', 'crates', 'native'], cwd=repo, text=True).splitlines()
sources = sorted(set(changed + untracked + ['Cargo.lock', 'rust-toolchain.toml', 'native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav', 'native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav', 'native/deadpan-source/tests/fixtures/cfr-bframes.mp4']))
write('identity.json', dict(base_commit=base, sources={name: sha(repo / name) for name in sources}, debug_cli_sha256=sha(scratch / 'qualified-debug-cli'), final_release_binaries={name: sha(repo / name) for name in ['target/release/deadpan-cli', 'target/release/deadpan-app']}))
patch = subprocess.check_output(['git', 'diff', '--binary', '--', 'crates', 'native'], cwd=repo)
for path in untracked:
    result = subprocess.run(['git', 'diff', '--no-index', '--binary', '--', '/dev/null', path], cwd=repo, capture_output=True)
    assert result.returncode == 1
    patch += result.stdout
(out / 'implementation.patch.gz').write_bytes(gzip.compress(patch, mtime=0))

environment = dict(recorded_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), ffmpeg_prefix='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix', sole_agent=True, live_gui=False, audio_device=False)
for name, command in [('hardware', ['sysctl', '-n', 'machdep.cpu.brand_string', 'hw.physicalcpu', 'hw.memsize']), ('os', ['sw_vers']), ('rust', ['rustc', '--version'])]:
    environment[name] = subprocess.check_output(command, text=True).strip()
write('environment.json', environment)
for name in ['self-review.txt', 'run.py', 'checks.py', 'release-checks.py', 'qualify-final.py', 'retain.py']:
    shutil.copyfile(scratch / name, out / name)
for name in ['debug-export-result.json', 'release-export-result.json', 'final-result.json']:
    (out / (name + '.gz')).write_bytes(gzip.compress((scratch / name).read_bytes(), mtime=0))
for mode in ['debug', 'release']:
    fixture = json.loads((scratch / (mode + '-export-result.json')).read_text())
    package = pathlib.Path(fixture['package'])
    with zipfile.ZipFile(out / (mode + '-project.zip'), 'w', zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(package.rglob('*')):
            if path.is_file():
                archive.write(path, str(path.relative_to(package.parent)))
    shutil.copyfile(fixture['movie'], out / (mode + '-occurrence-pauses.mp4'))
write('manifest.json', {path.name: sha(path) for path in sorted(out.iterdir()) if path.is_file() and path.name != 'manifest.json'})
print(json.dumps(counts, indent=2))
