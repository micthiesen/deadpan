from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

root = Path(__file__).resolve().parent
name, cwd, *command = sys.argv[1:]
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
if name.startswith('baseline'):
    env['CARGO_TARGET_DIR'] = str(root / 'baseline-target')
if name.startswith('export'):
    env['DEADPAN_PREVIEW_EXPORT_KEEP'] = '1'
    env['DEADPAN_PREVIEW_EXPORT_RESULTS'] = str(root / (name + '-results.json'))
started = datetime.now(timezone.utc).isoformat()
begin = time.monotonic()
with (root / (name + '.log')).open('x') as log:
    result = subprocess.run(command, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT)
record = dict(name=name, cwd=cwd, command=command, started_utc=started, seconds=time.monotonic()-begin, exit=result.returncode, log_sha256=hashlib.sha256((root / (name + '.log')).read_bytes()).hexdigest())
(root / (name + '.json')).write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record))
raise SystemExit(result.returncode)
