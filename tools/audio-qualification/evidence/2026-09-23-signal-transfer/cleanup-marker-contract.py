from pathlib import Path
import json, subprocess, tempfile

reports = []
for killed, returned, guarded in [(True, True, False), (True, True, True), (False, False, True), (False, True, True)]:
    with tempfile.TemporaryDirectory(prefix='deadpan-marker-contract-') as name:
        directory = Path(name)
        if returned:
            (directory / 'cleanup-returned').write_text('returned')
        delay = '''sleep 10 &
child=$!
kill -KILL "$child"
wait "$child"
status=$?
''' if killed else '''sleep 0.02
status=$?
'''
        continuation = '''[ "$status" -eq 0 ] && [ -f cleanup-returned ] && printf alive > survived
''' if guarded else '''printf alive > survived
'''
        script = delay + '''printf '%s' "$status" > delay-status
''' + continuation + 'exit 0\n'
        result = subprocess.run(['/bin/sh', '-c', script], cwd=directory, env={}, text=True, capture_output=True, timeout=5)
        status = int((directory / 'delay-status').read_text())
        marker = (directory / 'survived').exists()
        expected = returned and not killed if guarded else True
        assert result.returncode == 0 and marker == expected
        assert (status != 0) == killed
        reports.append({'killed_delay': killed, 'host_returned': returned, 'guarded': guarded, 'delay_status': status, 'marker': marker, 'script': script, 'stderr': result.stderr})
output = {'cases': reports, 'result': 'pass'}
Path('/tmp/deadpan-transfer-20260923/cleanup-marker-contract.json').write_text(json.dumps(output, indent=2) + '\n')
print(json.dumps(output))
