from pathlib import Path
import hashlib
import json
import os
import subprocess
import time

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
binary = repo / 'target/release/examples/perf'
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
package = root / 'long.deadpan'
pins = {'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'fixture': str(package), 'runs': []}
host = {}
for name, command in [('os', ['sw_vers']), ('cpu', ['sysctl', '-n', 'machdep.cpu.brand_string']),
                      ('memory', ['sysctl', '-n', 'hw.memsize']), ('power', ['pmset', '-g', 'batt']),
                      ('thermal', ['pmset', '-g', 'therm'])]:
    result = subprocess.run(command, text=True, capture_output=True)
    host[name] = dict(command=command, exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
host['started_unix_seconds'] = time.time()
(root/'device-long-host.json').write_text(json.dumps(host, indent=2)+'\n')
for name, cache in [('long-cold', str(root/'long-cache')), ('long-restart', str(root/'long-cache')), ('long-disabled', 'off')]:
    command = ['/usr/bin/time', '-l', str(binary), 'playback', str(package),
               '--pictures', 'original', '--seconds', '3', '--start-frame', '3600', '--audio-cache', cache]
    started = time.monotonic()
    with (root/f'device-{name}.json').open('xb') as out, (root/f'device-{name}.log').open('xb') as err:
        result = subprocess.run(command, cwd=repo, env=env, stdout=out, stderr=err)
    record = dict(command=command, exit=result.returncode, seconds=time.monotonic()-started,
                  report_sha256=hashlib.sha256((root/f'device-{name}.json').read_bytes()).hexdigest(),
                  log_sha256=hashlib.sha256((root/f'device-{name}.log').read_bytes()).hexdigest())
    pins['runs'].append(record)
    (root/'device-long-runs.json').write_text(json.dumps(pins, indent=2)+'\n')
    print(json.dumps(record), flush=True)
    if result.returncode: raise SystemExit(result.returncode)
