import json
import os
from pathlib import Path
import subprocess
import sys
import time

out = Path('/tmp/deadpan-source-range-20260926/focused-2')
out.mkdir(exist_ok=False)
commands = [
    ['cargo', 'test', '-p', 'deadpan-core', '--test', 'legacy_audio_selection', '--locked'],
    ['cargo', 'test', '-p', 'deadpan-media', '--test', 'source_import_timing', '--locked'],
    ['cargo', 'test', '-p', 'deadpan-store', '--test', 'migration', '--locked'],
    ['cargo', 'test', '-p', 'deadpan-cli', '--test', 'audio_context', '--locked'],
]
report = []
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-media-compatible-xyhilms4/prefix')
for index, command in enumerate(commands):
    print(json.dumps({'started': command}), flush=True)
    start = time.monotonic()
    with (out / f'{index}.log').open('w') as log:
        result = subprocess.run(command, cwd='/Users/michael/Code/deadpan', env=env,
                                stdout=log, stderr=subprocess.STDOUT)
    report.append({'command': command, 'exit_code': result.returncode,
                   'seconds': time.monotonic() - start, 'log': f'{index}.log'})
    print(json.dumps(report[-1]), flush=True)
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    if result.returncode:
        sys.exit(result.returncode)
