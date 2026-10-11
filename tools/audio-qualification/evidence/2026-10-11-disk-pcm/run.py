from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
import time

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
name, *command = sys.argv[1:]
start = time.monotonic()
with (root / (name + '.log')).open('x') as log:
    result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
record = dict(command=command, exit=result.returncode, seconds=time.monotonic()-start,
              log_sha256=hashlib.sha256((root / (name + '.log')).read_bytes()).hexdigest())
(root / (name + '.json')).write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record))
raise SystemExit(result.returncode)
