import datetime, hashlib, json, os, pathlib, re, subprocess, sys, time
root = pathlib.Path('/Users/michael/Code/deadpan')
scratch = pathlib.Path('/tmp/deadpan-tail-sends-20261012')
name, *command = sys.argv[1:]
env = os.environ.copy()
env['DEADPAN_FFMPEG_PREFIX'] = '/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
start = time.monotonic()
receipt = {'name': name, 'command': command, 'cwd': str(root), 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
           'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
           'diff_sha256': hashlib.sha256(subprocess.check_output(['git','diff','--binary'],cwd=root)).hexdigest()}
path = scratch / (name+'.log')
with path.open('wb') as log:
    result = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
receipt.update(exit=result.returncode, seconds=time.monotonic()-start, log_sha256=hashlib.sha256(path.read_bytes()).hexdigest())
receipt['binaries'] = {}
for token in command:
    candidate = root / token
    if token.startswith('target/') and candidate.is_file():
        receipt['binaries'][token] = hashlib.sha256(candidate.read_bytes()).hexdigest()
text = path.read_text(errors='replace')
for binary in set(re.findall(r'Running .*?\((target/[^)]+)\)', text)):
    p=root/binary
    if p.is_file(): receipt['binaries'][binary] = hashlib.sha256(p.read_bytes()).hexdigest()
(scratch/(name+'.json')).write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({'name':name,'exit':result.returncode,'seconds':receipt['seconds']}))
print(text[-5000:])
sys.exit(result.returncode)
