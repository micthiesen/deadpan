from pathlib import Path
import hashlib, json, os, subprocess, time
root = Path(__file__).parent
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
rows = []
for name, args in [
    ('native-all-current', ['test', '--locked', '-p', 'deadpan-source']),
    ('media-sessions-current', ['test', '--locked', '-p', 'deadpan-media', '--test', 'source_session']),
    ('store-source-current', ['test', '--locked', '-p', 'deadpan-store', '--test', 'source_registration']),
    ('workspace-clippy', ['clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings']),
    ('format-current', ['fmt', '--all', '--', '--check']),
]:
    print('START', name, flush=True)
    start = time.monotonic()
    logpath = root/(name+'.log')
    with logpath.open('w') as log:
        result = subprocess.run(['cargo', *args], cwd='/Users/michael/Code/deadpan', env=env, stdout=log, stderr=subprocess.STDOUT)
    rows.append(dict(name=name, command=['cargo', *args], exit=result.returncode, seconds=time.monotonic()-start, sha256=hashlib.sha256(logpath.read_bytes()).hexdigest()))
    (root/'native-all-checks.json').write_text(json.dumps(rows, indent=2)+'\n')
    print('FINISH', name, result.returncode, flush=True)
    if result.returncode:
        print(logpath.read_text()[-15000:])
        raise SystemExit(result.returncode)
