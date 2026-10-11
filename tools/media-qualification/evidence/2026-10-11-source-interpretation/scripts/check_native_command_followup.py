from pathlib import Path
import hashlib, json, os, re, subprocess, time

root = Path(__file__).parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
rows = []
for name, args in [
    ('native-command-followup-format', ['fmt', '--all']),
    ('native-command-followup-workspace', ['check', '--locked', '--workspace', '--all-targets']),
]:
    print('START', name, flush=True)
    start = time.monotonic()
    logpath = root/(name+'.log')
    with logpath.open('w') as log:
        result = subprocess.run(['cargo', *args], cwd='/Users/michael/Code/deadpan', env=env, stdout=log, stderr=subprocess.STDOUT)
    log = logpath.read_text()
    rows.append(dict(name=name, command=['cargo', *args], exit=result.returncode, seconds=time.monotonic()-start, sha256=hashlib.sha256(logpath.read_bytes()).hexdigest()))
    (root/'native-command-followup-checks.json').write_text(json.dumps(rows, indent=2)+'\n')
    print('FINISH', name, result.returncode, flush=True)
    if result.returncode:
        print(log[-20000:])
        raise SystemExit(result.returncode)
