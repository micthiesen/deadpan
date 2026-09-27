"""Verify the reviewed, data-only Kestrel digest refresh after the existing run."""
import hashlib
import json
import os
import subprocess
import time
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-moment-paste-20260926')
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix')
while not (scratch / 'remaining/report.json').exists():
    time.sleep(5)
result = subprocess.run(['python3', str(scratch / 'gate.py'), str(scratch / 'gate-3')], env=env)
print(json.dumps({'final_gate_exit': result.returncode}), flush=True)
source = json.loads((scratch / 'gate-3/source-after.json').read_text())
out = scratch / 'final-ui'
out.mkdir(exist_ok=False)
report = {'commands': []}
for mode in ('visual', 'performance'):
    command = ['cargo', 'run', '-p', 'deadpan-app']
    if mode == 'performance':
        command.append('--release')
    command += ['--features', 'ui-harness', '--locked', '--', '--ui-check', '--mode', mode,
                '--scenario', 'original-moment', '--kestrel-source',
                '/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift',
                '--output', str(scratch / ('ui-moment-final-' + mode))]
    print(json.dumps({'started_final_ui': mode, 'command': command}), flush=True)
    started = time.monotonic()
    with (out / (mode + '.log')).open('w') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    record = {'mode': mode, 'command': command, 'exit_code': result.returncode, 'seconds': time.monotonic() - started, 'log': mode + '.log'}
    report['commands'].append(record)
    print(json.dumps(record), flush=True)
report['source_unchanged'] = all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in source.items())
(out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
