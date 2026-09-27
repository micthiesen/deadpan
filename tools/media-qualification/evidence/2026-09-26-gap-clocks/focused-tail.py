import json
import os
from pathlib import Path
import subprocess
import sys
import time

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=False)
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-media-compatible-xyhilms4/prefix')
commands = [
    ['cargo', 'test', '--locked', '-p', 'deadpan-audio', '--test', 'gap_definition'],
    ['cargo', 'test', '--locked', '-p', 'deadpan-cli', '--test', 'audio_inspection'],
]
records = []
for index, command in enumerate(commands):
    print(json.dumps({'started': index, 'command': command}), flush=True)
    started = time.monotonic()
    with (out / f'{index}.log').open('w') as log:
        result = subprocess.run(command, cwd='/Users/michael/Code/deadpan', env=env, stdout=log, stderr=subprocess.STDOUT)
    record = {'command': command, 'exit_code': result.returncode,
              'seconds': time.monotonic() - started, 'log': f'{index}.log'}
    records.append(record)
    print(json.dumps(record), flush=True)
    if result.returncode:
        break
(out / 'report.json').write_text(json.dumps(records, indent=2) + '\n')
sys.exit(0 if len(records) == len(commands) and all(x['exit_code'] == 0 for x in records) else 1)
