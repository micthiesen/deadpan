from pathlib import Path
import json, os, subprocess, time
root = Path(__file__).parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
command = ['cargo', 'check', '--locked', '--workspace', '--all-targets']
start = time.monotonic()
with (root / 'workspace-check.log').open('w') as log:
    result = subprocess.run(command, cwd='/Users/michael/Code/deadpan', env=env, stdout=log, stderr=subprocess.STDOUT)
report = dict(command=command, exit=result.returncode, seconds=time.monotonic()-start)
(root / 'workspace-check.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report), flush=True)
if result.returncode:
    print((root / 'workspace-check.log').read_text()[-14000:])
raise SystemExit(result.returncode)
