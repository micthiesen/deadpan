from pathlib import Path
import difflib, hashlib, json, subprocess
r = Path('/Users/michael/Code/deadpan')
s = Path('/tmp/deadpan-retime-20260927')
paths = subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=r).decode().split('\0')
changed = {}
patch = []
for name in sorted(set(p for p in paths if p and p.startswith(('crates/', 'native/')) and p.endswith(('.rs', '.toml')))):
    path = r / name
    old = s / 'before' / name
    if not path.is_file():
        continue
    data = path.read_bytes()
    before = old.read_bytes() if old.exists() else b''
    if data == before:
        continue
    changed[name] = hashlib.sha256(data).hexdigest()
    patch.extend(difflib.unified_diff(before.decode().splitlines(True), data.decode().splitlines(True), fromfile='before/' + name, tofile='after/' + name))
(s / 'review-paths.json').write_text(json.dumps(changed, indent=2) + '\n')
(s / 'increment.patch').write_text(''.join(patch))
print(json.dumps({'count':len(changed), 'paths':list(changed)}))
