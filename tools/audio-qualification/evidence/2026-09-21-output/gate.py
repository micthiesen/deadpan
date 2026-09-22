import datetime
import json
import os
from pathlib import Path
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
out = Path('/tmp/deadpan-output-20260921/gate')
out.mkdir(exist_ok=True)
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-media-compatible-xyhilms4/prefix')
report = {'schema_version': 1, 'base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(), 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'checks': [], 'status': 'running', 'skipped': ['Native app GUI/startup smoke: no app or UI lifecycle integration changed; output lifecycle is exercised by the separate hardware probe.', 'Physical disconnect/rate switch/suspend, loopback/listening and full editing/inference stress remain open.']}
checks = [('format', ['cargo', 'fmt', '--all', '--', '--check']), ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']), ('test', ['cargo', 'test', '--workspace', '--locked']), ('build', ['cargo', 'build', '--workspace', '--locked']), ('doctor', ['cargo', 'run', '-p', 'deadpan-cli', '--', 'doctor'])]
for name, command in checks:
    started = time.monotonic()
    with (out / (name + '.log')).open('w') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    report['checks'].append({'name': name, 'command': command, 'exit_code': result.returncode, 'seconds': round(time.monotonic() - started, 3), 'log': name + '.log'})
    (out / 'gate.json').write_text(json.dumps(report, indent=2) + '\n')
    print(name, result.returncode, flush=True)
    if result.returncode:
        report['status'] = 'failed'
        break
else:
    report['status'] = 'passed'
report['completed_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
(out / 'gate.json').write_text(json.dumps(report, indent=2) + '\n')
raise SystemExit(0 if report['status'] == 'passed' else 1)
