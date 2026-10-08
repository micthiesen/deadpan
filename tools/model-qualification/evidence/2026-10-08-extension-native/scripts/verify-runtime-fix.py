from pathlib import Path
import hashlib
import json
import os
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
root = Path(__file__).resolve().parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix', DEADPAN_REQUIRE_SYNTHETIC_WORKER='1')
bundle = root / 'bundle-2/Deadpan.app'
cli = bundle / 'Contents/MacOS/deadpan-cli'
commands = [
    ('format-3', ['cargo', 'fmt', '--all', '--', '--check']),
    ('runtime-clippy', ['cargo', 'clippy', '-p', 'deadpan-cli', '--all-targets', '--features', 'synthetic-worker', '--locked', '--', '-D', 'warnings']),
    ('runtime-tests', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-cli', '--features', 'synthetic-worker', '-E', 'test(generation::runtime) + test(models::tests)', '--no-fail-fast']),
    ('gain-pipe-check', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-app', '--features', 'ui-harness', '-E', 'test(invalid_edits_are_atomic_and_range_changes_do_not_drop_hidden_keys)']),
    ('bundle-2', ['cargo', 'xtask', 'bundle', '--output', str(bundle.parent), '--allow-dirty', '--ltx-checkout', '/Users/michael/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4']),
    ('bundle-verify-2', ['cargo', 'xtask', 'bundle-verify', str(bundle), '--ai-models-from', '/private/tmp/deadpan-resume-20261006/scoped-home/Library/Application Support/Deadpan/Models/ltx-2.3-q4-bridge/1', '--keep']),
    ('install-extension', [str(cli), 'models', 'import', 'ltx-2.3-q4-extension', '/private/tmp/deadpan-resume-20261006/scoped-home/Library/Application Support/Deadpan/Models/ltx-2.3-q4-bridge/1', '--accept-license']),
    ('real-matrix', ['python3', str(root/'qualify.py'), '--cli', str(cli)]),
    ('pixel-oracle', ['/Users/michael/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4/.venv/bin/python3', str(root/'oracle.py'), '--ffmpeg', str(bundle/'Contents/Resources/ai-runtime/bin/ffmpeg')]),
    ('acceptance-export', ['python3', str(root/'verify-accepted.py'), '--cli', str(cli)]),
]
results = []
for name, command in commands:
    if name == 'bundle-2':
        paths = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=repo).decode().split('\0')
        suffixes = {'.rs', '.py', '.json', '.toml', '.lock', '.h', '.hpp', '.cpp', '.mm', '.c', '.m', '.sh'}
        hashes = {path: hashlib.sha256((repo/path).read_bytes()).hexdigest() for path in sorted(paths) if path and (repo/path).is_file() and (repo/path).suffix in suffixes}
        source = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo).decode().strip(), 'files': hashes}
        (root/'bundle-2-source.json').write_text(json.dumps(source, indent=2)+'\n')
        (root/'bundle-2-source.patch').write_bytes(subprocess.check_output(['git', 'diff', '--binary', 'HEAD'], cwd=repo))
    start = time.monotonic()
    with (root/(name+'.log')).open('x') as log:
        process = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    record = {'name': name, 'command': command, 'exit_code': process.returncode, 'seconds': time.monotonic()-start}
    results.append(record)
    (root/'runtime-fix-results.json').write_text(json.dumps(results, indent=2)+'\n')
    print(json.dumps(record), flush=True)
    if process.returncode:
        print((root/(name+'.log')).read_text()[-6500:], flush=True)
        raise SystemExit(process.returncode)
