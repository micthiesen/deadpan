"""Capture this increment against its saved working-tree baseline, without Git writes."""
import difflib, hashlib, json, subprocess
from pathlib import Path
repo = Path('/Users/michael/Code/deadpan')
out = Path('/tmp/deadpan-sound-events-20260927')
before = out / 'before'
paths = set(subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=repo).decode().split('\0'))
paths.update(str(p.relative_to(before)) for p in before.rglob('*') if p.is_file())
changes = []
diff = []
for rel in sorted(paths):
    if not rel or not (rel.startswith(('crates/', 'native/')) or (rel.startswith('docs/') and rel.endswith('.md')) or rel in ('AGENTS.md', 'README.md', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'rustfmt.toml')):
        continue
    old, new = before / rel, repo / rel
    old_bytes = old.read_bytes() if old.is_file() else None
    new_bytes = new.read_bytes() if new.is_file() else None
    if old_bytes == new_bytes:
        continue
    changes.append({'path': rel, 'before_sha256': hashlib.sha256(old_bytes).hexdigest() if old_bytes is not None else None, 'after_sha256': hashlib.sha256(new_bytes).hexdigest() if new_bytes is not None else None})
    try:
        diff.extend(difflib.unified_diff((old_bytes or b'').decode().splitlines(True), (new_bytes or b'').decode().splitlines(True), fromfile='before/'+rel, tofile='after/'+rel))
    except UnicodeDecodeError:
        diff.append('Binary changed: '+rel+'\n')
(out/'increment.json').write_text(json.dumps(changes, indent=2)+'\n')
(out/'increment.diff').write_text(''.join(diff))
print(json.dumps({'changed': len(changes), 'paths': [c['path'] for c in changes]}, indent=2))
