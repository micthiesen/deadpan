from pathlib import Path
import json, os, subprocess, time
s = Path('/tmp/deadpan-compact-json-size-2026-10-23')
(s / 'measure-wait-owner.json').write_text(json.dumps({'pid': os.getpid(), 'script': str(Path(__file__)), 'waiting_for': 'six successful resumed build rows, binary manifest and completed build marker'}, indent=2) + '\n')
while True:
    try:
        rows = json.loads((s / 'build-resumed-receipt.json').read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        rows = []
    if any(row['exit'] != 0 for row in rows):
        raise SystemExit('Private build or focused test failed; no measurement started')
    output = (s / 'build-resumed-driver.stdout').read_text()
    if len(rows) == 6 and (s / 'binaries.json').exists() and '"done": true' in output:
        break
    time.sleep(2)
command = ['python', str(s / 'measure.py')]
started = time.monotonic()
with (s / 'measure-driver.stdout').open('w') as out, (s / 'measure-driver.stderr').open('w') as err:
    result = subprocess.run(command, stdout=out, stderr=err)
(s / 'measure-receipt.json').write_text(json.dumps({'command': command, 'exit': result.returncode, 'seconds': time.monotonic() - started}, indent=2) + '\n')
raise SystemExit(result.returncode)
