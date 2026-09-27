"""Finish the current gate, then record a stable gate and uncovered targets/replays."""
import hashlib
import json
import os
import re
import subprocess
import time
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-moment-paste-20260926')
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix')
while not (scratch / 'gate-1/report.json').exists():
    time.sleep(5)
result = subprocess.run(['python3', str(scratch / 'gate.py'), str(scratch / 'gate-2')], env=env)
print(json.dumps({'stable_gate_exit': result.returncode}), flush=True)
out = scratch / 'remaining'
out.mkdir(exist_ok=False)
commands = [
    ['cargo', 'test', '--workspace', '--locked', '--no-fail-fast',
     '--exclude', 'deadpan-app', '--exclude', 'deadpan-audio',
     '--exclude', 'deadpan-cli', '--exclude', 'deadpan-core',
     '--exclude', 'deadpan-dsp', '--exclude', 'deadpan-fileclone'],
    ['cargo', 'run', '-p', 'deadpan-app', '--features', 'ui-harness', '--locked', '--',
     '--ui-check', '--scenario', 'original-moment', '--kestrel-source',
     '/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift',
     '--output', str(scratch / 'ui-moment')],
    ['cargo', 'run', '-p', 'deadpan-app', '--release', '--features', 'ui-harness', '--locked', '--',
     '--ui-check', '--mode', 'performance', '--scenario', 'original-moment', '--kestrel-source',
     '/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift',
     '--output', str(scratch / 'ui-moment-performance')],
]
source = json.loads((scratch / 'gate-2/source-after.json').read_text())
assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in source.items())
report = {'commands': [], 'source_count': len(source)}
for index, command in enumerate(commands):
    print(json.dumps({'started_remaining': index, 'command': command}), flush=True)
    started = time.monotonic()
    with (out / f'{index}.log').open('w') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    rows = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', (out / f'{index}.log').read_text())
    record = {'command': command, 'exit_code': result.returncode, 'seconds': time.monotonic() - started, 'log': f'{index}.log'}
    if rows:
        record['tests'] = dict(zip(('passed', 'failed', 'ignored'), (sum(int(r[i]) for r in rows) for i in range(3))))
    report['commands'].append(record)
    (out / 'progress.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(record), flush=True)
report['source_unchanged'] = all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in source.items())
(out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
