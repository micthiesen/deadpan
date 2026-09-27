from pathlib import Path
import difflib, hashlib, json

repo=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-group-navigation-20260926')
before=scratch/'before'
paths={p.relative_to(before) for p in before.rglob('*') if p.is_file()}
paths.update(p.relative_to(repo) for p in (repo/'crates/deadpan-app').rglob('*') if p.is_file())
paths.update(p.relative_to(repo) for p in (repo/'docs').rglob('*.md') if p.is_file())
changes=[]
patch=[]
for rel in sorted(paths):
    old=(before/rel).read_bytes() if (before/rel).exists() else b''
    new=(repo/rel).read_bytes() if (repo/rel).exists() else b''
    if old==new: continue
    changes.append({'path':str(rel),'sha256':hashlib.sha256(new).hexdigest(),'bytes':len(new)})
    try:
        patch.extend(difflib.unified_diff(old.decode().splitlines(True),new.decode().splitlines(True),fromfile=f'before/{rel}',tofile=f'after/{rel}'))
    except UnicodeDecodeError: pass
(scratch/'increment.json').write_text(json.dumps(changes,indent=2)+'\n')
(scratch/'increment.diff').write_text(''.join(patch))
print(json.dumps({'changed':len(changes),'paths':[v['path'] for v in changes]},indent=2))
