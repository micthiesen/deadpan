from pathlib import Path
import os, subprocess, json, hashlib, time

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
rows = []
steps = [
    ('production-release', ['python3', str(root / 'production_probe.py'), 'release']),
    ('production-bundle', ['python3', str(root / 'production_probe.py'), 'bundle']),
    ('milestone-gate', ['cargo', 'xtask', 'gate']),
    ('milestone-replays', ['cargo', 'xtask', 'replays', '--jobs', '2', '--output', str(root / 'milestone-replays')]),
    ('milestone-performance-build', ['cargo', 'build', '--release', '--locked', '-p', 'deadpan-app', '--features', 'ui-harness']),
    ('milestone-performance', ['deadpan-app', '--ui-check', '--scenario', 'source-interpretation', '--mode', 'performance', '--output', str(root / 'milestone-performance')]),
]
for name, command in steps:
    print('START', name, flush=True)
    current_env = dict(env)
    if command[0] == 'deadpan-app':
        current_env['PATH'] = str(repo / 'target/release') + os.pathsep + current_env['PATH']
    start = time.monotonic()
    log_path = root / (name + '.log')
    with log_path.open('x') as log:
        result = subprocess.run(command, cwd=repo, env=current_env, stdout=log, stderr=subprocess.STDOUT)
    row = dict(name=name, command=command, exit=result.returncode, seconds=time.monotonic()-start, log_sha256=hashlib.sha256(log_path.read_bytes()).hexdigest())
    if command[0] == 'deadpan-app':
        row['binary_sha256'] = hashlib.sha256((repo/'target/release/deadpan-app').read_bytes()).hexdigest()
    rows.append(row)
    (root/'milestone-checks.json').write_text(json.dumps(rows, indent=2)+'\n')
    print('FINISH', name, result.returncode, flush=True)
    if result.returncode:
        print(log_path.read_text(errors='replace')[-10000:], flush=True)
        if name == 'milestone-performance-build':
            break
raise SystemExit(any(row['exit'] for row in rows))
