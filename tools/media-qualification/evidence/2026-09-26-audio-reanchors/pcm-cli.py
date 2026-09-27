import json
import os
from pathlib import Path
import subprocess
import sys
import time

repo = Path('/Users/michael/Code/deadpan')
out = Path(sys.argv[1])
out.mkdir(exist_ok=False)
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-media-compatible-xyhilms4/prefix')
commands = [
    ['cargo', 'test', '-p', 'deadpan-audio', '--test', 'reanchors', '--locked'],
    ['cargo', 'test', '-p', 'deadpan-cli', '--test', 'project_commands', '--locked'],
]
report = []
for index, command in enumerate(commands):
    print(json.dumps({'started': command}), flush=True)
    started = time.monotonic()
    with (out / f'{index}.log').open('w') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    report.append(dict(command=command, exit_code=result.returncode, seconds=time.monotonic()-started, log=f'{index}.log'))
    (out / 'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report[-1]), flush=True)
    if result.returncode:
        sys.exit(result.returncode)
