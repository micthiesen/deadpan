"""Rerun the exact workspace-test binary with the original backtrace setting."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

repo = Path('/Users/michael/Code/deadpan')
binary = repo / 'target/debug/deps/deadpan_playback-34f16a24a1d54369'
out = Path('/tmp/deadpan-gap-bindings-20260926/playback-exact')
out.mkdir(exist_ok=False)
checksum = hashlib.sha256(binary.read_bytes()).hexdigest()
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-media-compatible-xyhilms4/prefix')
env.pop('RUST_BACKTRACE', None)
started = time.monotonic()
with (out / 'output.log').open('w') as log:
    result = subprocess.run([str(binary)], cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
record = dict(command=[str(binary)], sha256=checksum, exit_code=result.returncode,
              seconds=time.monotonic() - started, test_threads='default',
              changed_timeouts_or_assertions=False, rust_backtrace='unset as in the original full gate',
              reason='Package-only Cargo retry compiled a different feature-unified binary; backtrace instrumentation timed out the intentionally panicking test. This runs the original exact workspace artifact.')
assert hashlib.sha256(binary.read_bytes()).hexdigest() == checksum
(out / 'report.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record), flush=True)
raise SystemExit(result.returncode)
