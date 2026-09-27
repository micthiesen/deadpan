"""Verify the CLI schema expectation correction after the complete stable gate."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-gap-branches-20260926')
gate = scratch / 'gate-4'
out = scratch / 'post-validation'
out.mkdir(exist_ok=False)
old = json.loads((gate / 'source-after.json').read_text())


def seal():
    return {path: hashlib.sha256((repo / path).read_bytes()).hexdigest() for path in old}


before = seal()
changed = [path for path in old if old[path] != before[path]]
assert changed == ['crates/deadpan-cli/tests/project_commands.rs'], changed
(out / 'source-before.json').write_text(json.dumps(before, indent=2) + '\n')
commands = [
    ['cargo', 'fmt', '--all', '--', '--check'],
    ['cargo', 'clippy', '-p', 'deadpan-cli', '--all-targets', '--locked', '--', '-D', 'warnings'],
    ['cargo', 'test', '-p', 'deadpan-cli', '--test', 'project_commands', '--locked'],
]
records = []
for index, command in enumerate(commands):
    start = time.monotonic()
    with (out / f'{index}.log').open('w') as log:
        result = subprocess.run(command, cwd=repo,
            env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix'),
            stdout=log, stderr=subprocess.STDOUT)
    record = dict(command=command, exit_code=result.returncode,
                  seconds=time.monotonic()-start, log=f'{index}.log')
    records.append(record)
    print(json.dumps(record), flush=True)
    if result.returncode:
        break
after = seal()
(out / 'source-after.json').write_text(json.dumps(after, indent=2) + '\n')
report = dict(commands=records, changed_since_full_gate=changed,
              source_count=len(after), source_unchanged=before == after,
              production_sources_unchanged=True,
              passed=len(records) == len(commands) and all(r['exit_code'] == 0 for r in records) and before == after)
(out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
raise SystemExit(0 if report['passed'] else 1)
