from pathlib import Path
import hashlib
import json
import os
import subprocess
import time

root = Path(__file__).resolve().parent
cli = Path('/tmp/deadpan-inference-pause-final-bin-2026-10-17/deadpan-cli')
runtime = Path('/Users/michael/Library/Caches/Deadpan/ltx-runtime/ltx-2-mlx-3392d75934120b7e69eefbe55893f7ef82be92a4')
env = os.environ.copy()
env.update({
    'DEADPAN_BRIDGE_RUNTIME_SOURCE': str(runtime),
    'DEADPAN_BRIDGE_PYTHON': str(runtime / '.venv/bin/python3'),
    'DEADPAN_BRIDGE_WORKER': '/Users/michael/Code/deadpan/tools/model-qualification/worker.py',
    'DEADPAN_BRIDGE_FFMPEG': '/opt/homebrew/bin/ffmpeg',
    'DEADPAN_BRIDGE_FFPROBE': '/opt/homebrew/bin/ffprobe',
})
command = [str(cli), 'generate-hold', str(root / 'fixture.deadpan'), '--hold', 'extension', '--mode', 'auto', '--motion', 'still', '--seed', '42107']
started = time.monotonic()
with (root / 'complete.json').open('x') as output, (root / 'complete.stderr').open('x') as errors:
    result = subprocess.run(command, env=env, stdout=output, stderr=errors)
record = {'command': command, 'exit_code': result.returncode, 'seconds': time.monotonic()-started,
          'binary_sha256': hashlib.sha256(cli.read_bytes()).hexdigest()}
(root / 'complete-execution.json').write_text(json.dumps(record, indent=2)+'\n')
print(json.dumps(record), flush=True)
if result.returncode:
    print((root / 'complete.stderr').read_text()[-4000:], flush=True)
    raise SystemExit(result.returncode)
generated = json.loads((root / 'complete.json').read_text())
assert generated['state'] == 'Ready', generated
after = subprocess.run([str(cli), 'project', 'dump', str(root/'fixture.deadpan'), '--json'], capture_output=True, text=True, check=True)
(root/'after-ready.json').write_text(after.stdout)
assert json.loads(after.stdout) == json.loads((root/'before.json').read_text()), 'Ready changed authored state'
print('Ready with unchanged authored document', flush=True)
