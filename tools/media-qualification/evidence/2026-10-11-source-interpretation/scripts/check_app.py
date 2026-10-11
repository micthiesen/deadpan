from pathlib import Path
import hashlib, json, os, re, subprocess, time
root = Path(__file__).parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
command = ['cargo', 'test', '--locked', '-p', 'deadpan-app', '--bin', 'deadpan-app', 'interpretation']
start = time.monotonic()
logpath = root/'app-interpretation.log'
with logpath.open('w') as log:
    result = subprocess.run(command, cwd='/Users/michael/Code/deadpan', env=env, stdout=log, stderr=subprocess.STDOUT)
log = logpath.read_text()
passed = sum(map(int, re.findall(r'test result: ok\. (\d+) passed', log)))
report = dict(command=command, exit=result.returncode, passed=passed, seconds=time.monotonic()-start, sha256=hashlib.sha256(logpath.read_bytes()).hexdigest())
(root/'app-check.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report), flush=True)
print(log[-12000:])
raise SystemExit(result.returncode or (1 if passed == 0 else 0))
