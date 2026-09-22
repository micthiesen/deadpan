import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
out = Path('/tmp/deadpan-preview-presentation-20260921/gate')
out.mkdir(exist_ok=True)
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-media-compatible-xyhilms4/prefix')
report = {'schema_version': 1, 'base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(), 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'checks': [], 'status': 'running'}
checks = [('format', ['cargo', 'fmt', '--all', '--', '--check']), ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']), ('test', ['cargo', 'test', '--workspace', '--locked']), ('build', ['cargo', 'build', '--workspace', '--locked']), ('doctor', ['cargo', 'run', '-p', 'deadpan-cli', '--', 'doctor']), ('native-smoke', ['cargo', 'run', '-p', 'deadpan-app', '--', '--smoke-test'])]
for name, command in checks:
    started = time.monotonic()
    log_path = out / (name + '.log')
    with log_path.open('w') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    check = {'name': name, 'command': command, 'exit_code': result.returncode, 'seconds': round(time.monotonic() - started, 3), 'log': name + '.log'}
    if name == 'test':
        counts = re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;', log_path.read_text())
        check['test_counts'] = dict(zip(['passed', 'failed', 'ignored'], [sum(int(row[i]) for row in counts) for i in range(3)]))
    report['checks'].append(check)
    (out / 'gate.json').write_text(json.dumps(report, indent=2) + '\n')
    print(name, result.returncode, check.get('test_counts', ''), flush=True)
    if result.returncode:
        report['status'] = 'failed'
        break
else:
    report['status'] = 'passed'
report['completed_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
(out / 'gate.json').write_text(json.dumps(report, indent=2) + '\n')
raise SystemExit(0 if report['status'] == 'passed' else 1)
