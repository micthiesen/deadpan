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
package = Path('/tmp/deadpan-audio-ahead-20261011/future.deadpan')
pins = {'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'fixture': str(package), 'runs': []}
host = {}
for name, command in [('os', ['sw_vers']), ('cpu', ['sysctl', '-n', 'machdep.cpu.brand_string']),
                      ('memory', ['sysctl', '-n', 'hw.memsize']), ('power', ['pmset', '-g', 'batt']),
                      ('thermal', ['pmset', '-g', 'therm'])]:
    result = subprocess.run(command, text=True, capture_output=True)
    host[name] = dict(command=command, exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
host['started_unix_seconds'] = time.time()
(root/'device-final-host.json').write_text(json.dumps(host, indent=2)+'\n')
for name, cache in [('final-cold', str(root/'final-device-cache')), ('final-restart', str(root/'final-device-cache')), ('final-disabled', 'off')]:
    command = ['/usr/bin/time', '-l', str(binary), 'playback', str(package),
               '--pictures', 'original', '--seconds', '10', '--start-frame', '240', '--audio-cache', cache]
    started = time.monotonic()
    with (root/f'device-{name}.json').open('xb') as out, (root/f'device-{name}.log').open('xb') as err:
        result = subprocess.run(command, cwd=repo, env=env, stdout=out, stderr=err)
    record = dict(command=command, exit=result.returncode, seconds=time.monotonic()-started,
                  report_sha256=hashlib.sha256((root/f'device-{name}.json').read_bytes()).hexdigest(),
                  log_sha256=hashlib.sha256((root/f'device-{name}.log').read_bytes()).hexdigest())
    pins['runs'].append(record)
    (root/'device-final-runs.json').write_text(json.dumps(pins, indent=2)+'\n')
    print(json.dumps(record), flush=True)
    if result.returncode: raise SystemExit(result.returncode)
