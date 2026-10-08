from pathlib import Path
import hashlib
import json
import os
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
root = Path(__file__).resolve().parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix', DEADPAN_REQUIRE_SYNTHETIC_WORKER='1')
commands = [
    ('format-2', ['cargo', 'fmt', '--all', '--', '--check']),
    ('final-clippy-2', ['cargo', 'clippy', '-p', 'deadpan-app', '--all-targets', '--features', 'ui-harness', '--locked', '--', '-D', 'warnings']),
    ('replays-2', ['cargo', 'xtask', 'replays', '--scenario', 'ai-pause,ai-extension,model-packs', '--output', str(root/'replays-2'), '--jobs', '1']),
    ('bundle', ['cargo', 'xtask', 'bundle', '--output', str(root/'bundle'), '--allow-dirty', '--ltx-checkout', '/Users/michael/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4']),
    ('bundle-verify', ['cargo', 'xtask', 'bundle-verify', str(root/'bundle/Deadpan.app'), '--ai-models-from', '/private/tmp/deadpan-resume-20261006/scoped-home/Library/Application Support/Deadpan/Models/ltx-2.3-q4-bridge/1', '--keep']),
]
results = []
for name, command in commands:
    if name == 'bundle':
        paths = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=repo).decode().split('\0')
        suffixes = {'.rs', '.py', '.json', '.toml', '.lock', '.h', '.hpp', '.cpp', '.mm', '.c', '.m', '.sh'}
        hashes = {path: hashlib.sha256((repo/path).read_bytes()).hexdigest() for path in sorted(paths) if path and (repo/path).is_file() and (repo/path).suffix in suffixes}
        source = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo).decode().strip(), 'files': hashes}
        (root/'bundle-source.json').write_text(json.dumps(source, indent=2)+'\n')
        (root/'bundle-source.patch').write_bytes(subprocess.check_output(['git', 'diff', '--binary', 'HEAD'], cwd=repo))
    start = time.monotonic()
    with (root/(name+'.log')).open('x') as log:
        process = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    record = {'name': name, 'command': command, 'exit_code': process.returncode, 'seconds': time.monotonic()-start}
    results.append(record)
    (root/'remaining-results.json').write_text(json.dumps(results, indent=2)+'\n')
    print(json.dumps(record), flush=True)
    if process.returncode:
        print((root/(name+'.log')).read_text()[-6500:], flush=True)
        raise SystemExit(process.returncode)
