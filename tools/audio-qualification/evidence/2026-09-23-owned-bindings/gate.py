from pathlib import Path
import hashlib, json, os, subprocess, time

out = Path('/tmp/deadpan-audio-bindings-20260923/gate')
out.mkdir(exist_ok=True)
env = os.environ.copy()
env['DEADPAN_FFMPEG_PREFIX'] = '/tmp/deadpan-media-compatible-xyhilms4/prefix'

def hashes():
    paths = set(subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard'], text=True).splitlines())
    return {p: hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in sorted(paths) if p.startswith(('crates/', 'native/')) or p in ('Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml', '.rustfmt.toml', 'rustfmt.toml')}

initial = hashes()
(out / 'source-hashes.json').write_text(json.dumps(initial, indent=2) + '\n')
commands = [
    ['cargo', 'fmt', '--all', '--', '--check'],
    ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'],
    ['cargo', 'test', '--workspace', '--locked'],
    ['cargo', 'build', '--workspace', '--locked'],
    ['cargo', 'run', '-p', 'deadpan-cli', '--', 'doctor'],
    ['cargo', 'run', '-p', 'deadpan-app', '--locked', '--', '--smoke-test'],
]
report = []
for i, command in enumerate(commands):
    start = time.monotonic()
    with (out / f'{i}.log').open('w') as log:
        result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
    row = {'command': command, 'exit_code': result.returncode, 'seconds': time.monotonic() - start}
    report.append(row)
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(row), flush=True)
    if result.returncode:
        print((out / f'{i}.log').read_text()[-8000:], flush=True)
        raise SystemExit(result.returncode)
assert initial == hashes(), 'Source files changed during gate'
print(f'All {len(initial)} source and fixture hashes unchanged through gate', flush=True)
