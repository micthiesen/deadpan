"""Format the latest shared checkout and record the subsequent formatting check."""
import json
from pathlib import Path
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
out = Path('/tmp/deadpan-gap-bindings-20260926/post-format')
out.mkdir(exist_ok=False)
commands = [['cargo', 'fmt', '--all'], ['cargo', 'fmt', '--all', '--', '--check']]
records = []
for index, command in enumerate(commands):
    started = time.monotonic()
    with (out / f'{index}.log').open('w') as log:
        result = subprocess.run(command, cwd=repo, stdout=log, stderr=subprocess.STDOUT)
    records.append(dict(command=command, exit_code=result.returncode, seconds=time.monotonic() - started))
    (out / 'report.json').write_text(json.dumps(records, indent=2) + '\n')
    print(json.dumps(records[-1]), flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)
