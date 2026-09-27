"""Repeat the unmodified playback suite, retaining exact failures and backtraces."""
import json
import os
from pathlib import Path
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
out = Path('/tmp/deadpan-gap-bindings-20260926/playback-rerun')
out.mkdir(exist_ok=False)
command = ['cargo', 'test', '--locked', '-p', 'deadpan-playback', '--lib']
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-media-compatible-xyhilms4/prefix', RUST_BACKTRACE='1')
started = time.monotonic()
with (out / 'output.log').open('w') as log:
    result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
record = dict(command=command, exit_code=result.returncode, seconds=time.monotonic() - started,
              changes_to_test_or_production_timeouts=False,
              system_load_observation='sysctl vm.loadavg was denied by the sandbox; no numeric load measurement available')
(out / 'report.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record), flush=True)
raise SystemExit(result.returncode)
