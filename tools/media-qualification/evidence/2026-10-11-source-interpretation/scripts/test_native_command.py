from pathlib import Path
import hashlib, json, os, re, subprocess, time

root = Path(__file__).parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
rows = []
for name, args in [
    ('native-entry-format', ['fmt', '--all']),
    ('native-entry-check', ['check', '--locked', '--workspace', '--all-targets', '--features', 'deadpan-app/ui-harness']),
    ('native-entry-interpretation', ['test', '--locked', '-p', 'deadpan-app', '--bin', 'deadpan-app', 'interpretation']),
    ('native-entry-shots', ['test', '--locked', '-p', 'deadpan-app', '--bin', 'deadpan-app', 'shots']),
    ('native-entry-readonly', ['test', '--locked', '-p', 'deadpan-app', '--bin', 'deadpan-app', 'readonly']),
    ('native-entry-build', ['build', '--locked', '-p', 'deadpan-app', '--features', 'ui-harness']),

]:
    print('START', name, flush=True)
    start = time.monotonic()
    logpath = root/(name+'.log')
    with logpath.open('w') as log:
        result = subprocess.run(['cargo', *args], cwd='/Users/michael/Code/deadpan', env=env, stdout=log, stderr=subprocess.STDOUT)
    log = logpath.read_text()
    passed = sum(map(int, re.findall(r'test result: ok\. (\d+) passed', log)))
    rows.append(dict(name=name, command=['cargo', *args], exit=result.returncode, passed=passed if args[0]=='test' else None, seconds=time.monotonic()-start, sha256=hashlib.sha256(logpath.read_bytes()).hexdigest()))
    (root/'native-entry-tests.json').write_text(json.dumps(rows, indent=2)+'\n')
    print('FINISH', name, result.returncode, 'passed', passed, flush=True)
    if result.returncode or (args[0]=='test' and passed==0):
        print(log[-16000:])
        raise SystemExit(result.returncode or 1)
