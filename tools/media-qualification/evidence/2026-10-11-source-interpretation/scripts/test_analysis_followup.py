from pathlib import Path
import hashlib, json, os, re, subprocess, time

root = Path(__file__).parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
rows = []
for name, args in [
    ('analysis-followup-format', ['fmt', '--all']),
    ('analysis-followup-check', ['check', '--locked', '--workspace', '--all-targets', '--features', 'deadpan-app/ui-harness']),
    ('analysis-followup-media', ['test', '--locked', '-p', 'deadpan-media', '--test', 'picture_scan']),
    ('analysis-followup-jobs', ['test', '--locked', '-p', 'deadpan-jobs', '--lib']),
    ('analysis-followup-cli', ['test', '--locked', '-p', 'deadpan-cli', '--test', 'shots']),
    ('analysis-followup-tracker', ['test', '--locked', '-p', 'deadpan-track', '--test', 'worker']),
    ('analysis-followup-faces', ['test', '--locked', '-p', 'deadpan-track', '--test', 'faces']),
    ('analysis-followup-app', ['test', '--locked', '-p', 'deadpan-app', '--bin', 'deadpan-app', 'shots']),

]:
    print('START', name, flush=True)
    start = time.monotonic()
    logpath = root/(name+'.log')
    with logpath.open('w') as log:
        result = subprocess.run(['cargo', *args], cwd='/Users/michael/Code/deadpan', env=env, stdout=log, stderr=subprocess.STDOUT)
    log = logpath.read_text()
    passed = sum(map(int, re.findall(r'test result: ok\. (\d+) passed', log)))
    rows.append(dict(name=name, command=['cargo', *args], exit=result.returncode, passed=passed if args[0]=='test' else None, seconds=time.monotonic()-start, sha256=hashlib.sha256(logpath.read_bytes()).hexdigest()))
    (root/'analysis-followup-tests.json').write_text(json.dumps(rows, indent=2)+'\n')
    print('FINISH', name, result.returncode, 'passed', passed, flush=True)
    if result.returncode or (args[0]=='test' and passed==0):
        print(log[-16000:])
        raise SystemExit(result.returncode or 1)
