from pathlib import Path
import os, subprocess, hashlib, json, time
root = Path(__file__).parent
binary = Path('/Users/michael/Code/deadpan/target/debug/deadpan-app')
env = dict(os.environ, PATH=str(binary.parent)+os.pathsep+os.environ['PATH'], DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
args = ['deadpan-app', '--ui-check', '--scenario', 'source-interpretation', '--mode', 'visual', '--output', str(root/'native-replay-1'), '--retain-projects']
started = time.monotonic()
with (root/'native-replay-1.log').open('w') as log:
    result = subprocess.run(args, env=env, stdout=log, stderr=subprocess.STDOUT)
record = dict(command=args, exit=result.returncode, seconds=time.monotonic()-started, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
(root/'native-replay-1-run.json').write_text(json.dumps(record, indent=2)+'\n')
print(json.dumps(record))
print((root/'native-replay-1.log').read_text()[-10000:])
raise SystemExit(result.returncode)
