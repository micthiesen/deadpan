import json
import pathlib
import subprocess
import tempfile
import time

work = pathlib.Path(tempfile.mkdtemp(prefix='deadpan-original-final-gate-'))
pathlib.Path('/tmp/deadpan-original-final-gate-current.txt').write_text(str(work))
commands = [
    ['cargo', 'fmt', '--all', '--', '--check'],
    ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'],
    ['cargo', 'test', '--workspace', '--locked'],
    ['cargo', 'build', '--workspace', '--locked'],
    ['cargo', 'run', '-p', 'deadpan-cli', '--', 'doctor'],
    ['python3', '-m', 'unittest', 'discover', '-s', 'tools/audio-qualification', '-p', 'test_*.py', '-v'],
    ['python3', '-m', 'unittest', 'discover', '-s', 'tools/model-qualification/tests', '-p', 'test_*.py', '-v'],
    ['python3', '-m', 'unittest', 'discover', '-s', 'tools/media-qualification/ffv1', '-p', 'test_*.py', '-v'],
    ['python3', '-m', 'unittest', 'discover', '-s', 'tools/media-qualification/host', '-p', 'test_*.py', '-v'],
]
report = []
print(work, flush=True)
for index, command in enumerate(commands):
    start = time.monotonic()
    log = work / f'{index}.log'
    with log.open('w') as output:
        result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT)
    item = {'command': command, 'exit_code': result.returncode, 'seconds': time.monotonic() - start}
    report.append(item)
    (work / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(item), flush=True)
    if result.returncode:
        print(log.read_text()[-12000:])
        raise SystemExit(result.returncode)
