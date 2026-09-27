import json
import os
from pathlib import Path
import subprocess
import sys
import time

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=False)
commands = [
    ['cargo', 'test', '--locked', '-p', 'deadpan-audio', '--test', 'gap_bindings'],
    ['cargo', 'test', '--locked', '-p', 'deadpan-cli', '--test', 'project_commands', '--test', 'audio_inspection'],
]
report = []
for index, command in enumerate(commands):
    print(json.dumps({'started': index, 'command': command}), flush=True)
    start = time.monotonic()
    with (out / f'{index}.log').open('w') as log:
        result = subprocess.run(command, cwd='/Users/michael/Code/deadpan',
            env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-media-compatible-xyhilms4/prefix'),
            stdout=log, stderr=subprocess.STDOUT)
    record = {'command': command, 'exit_code': result.returncode, 'seconds': time.monotonic()-start}
    report.append(record)
    (out / 'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(record), flush=True)
    if result.returncode:
        print((out / f'{index}.log').read_text()[-12000:], flush=True)
        sys.exit(result.returncode)
