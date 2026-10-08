from pathlib import Path
import json
import os
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
root = Path(__file__).resolve().parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix', DEADPAN_REQUIRE_SYNTHETIC_WORKER='1')
commands = [
    ('format', ['cargo', 'fmt', '--all', '--', '--check']),
    ('final-clippy', ['cargo', 'clippy', '-p', 'deadpan-models', '-p', 'deadpan-app', '--all-targets', '--features', 'ui-harness', '--locked', '--', '-D', 'warnings']),
    ('catalog-fuzz', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-models', '-E', 'test(adversarial_pack_manifests)']),
    ('ui-tests', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-app', '--features', 'ui-harness', '--no-fail-fast']),
    ('doc-tests', ['cargo', 'test', '--workspace', '--features', 'deadpan-cli/synthetic-worker', '--locked', '--doc', '--no-fail-fast']),
    ('python-tests', ['python3', '-m', 'unittest', 'discover', '-s', 'tools/model-qualification/tests', '-p', 'test_*.py']),
    ('replays', ['cargo', 'xtask', 'replays', '--scenario', 'ai-pause,ai-variants,ai-extension,ai-compare,model-packs', '--output', str(root/'replays'), '--jobs', '1']),
    ('bundle', ['cargo', 'xtask', 'bundle', '--output', str(root/'bundle'), '--allow-dirty', '--ltx-checkout', '/Users/michael/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4']),
    ('bundle-verify', ['cargo', 'xtask', 'bundle-verify', str(root/'bundle/Deadpan.app'), '--ai-models-from', '/private/tmp/deadpan-resume-20261006/scoped-home/Library/Application Support/Deadpan/Models/ltx-2.3-q4-bridge/1', '--keep']),
]
results = []
for name, command in commands:
    start = time.monotonic()
    with (root/(name+'.log')).open('x') as log:
        process = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    record = {'name': name, 'command': command, 'exit_code': process.returncode, 'seconds': time.monotonic()-start}
    results.append(record)
    (root/'verification-results.json').write_text(json.dumps(results, indent=2)+'\n')
    print(json.dumps(record), flush=True)
    if process.returncode:
        print((root/(name+'.log')).read_text()[-6500:], flush=True)
        raise SystemExit(process.returncode)
