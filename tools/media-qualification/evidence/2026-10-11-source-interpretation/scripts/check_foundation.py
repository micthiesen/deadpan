from pathlib import Path
import hashlib, json, os, subprocess, time
root = Path(__file__).parent
repo = '/Users/michael/Code/deadpan'
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
rows = []
for name, args in [
    ('core-interpretation', ['test', '--locked', '-p', 'deadpan-core', '--test', 'source_interpretation']),
    ('native-interpretation', ['test', '--locked', '-p', 'deadpan-source', '--test', 'interpretation']),
    ('media-sessions', ['test', '--locked', '-p', 'deadpan-media', '--test', 'source_session']),
    ('store-interpretation', ['test', '--locked', '-p', 'deadpan-store', '--test', 'source_registration', 'interpretation::']),
]:
    print('START', name, flush=True)
    start = time.monotonic()
    logpath = root / (name+'.log')
    with logpath.open('w') as log:
        result = subprocess.run(['cargo', *args], cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    rows.append(dict(name=name, command=['cargo', *args], exit=result.returncode, seconds=time.monotonic()-start, log_sha256=hashlib.sha256(logpath.read_bytes()).hexdigest()))
    (root/'foundation-checks.json').write_text(json.dumps(rows, indent=2)+'\n')
    print('FINISH', name, result.returncode, flush=True)
    if result.returncode:
        print(logpath.read_text()[-14000:])
        raise SystemExit(result.returncode)
