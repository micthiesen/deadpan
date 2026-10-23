from pathlib import Path
import json, hashlib, difflib, sys
q = Path('/tmp/deadpan-compact-json-size-qualification-2026-10-23')
s = Path('/tmp/deadpan-compact-json-size-2026-10-23')
name = sys.argv[1]
rows = json.loads((s / 'draft-source-hashes.json').read_text())['files']
row = next(row for row in rows if row['path'] == name)
old = Path(name).read_bytes()
new = (s / 'source' / name).read_bytes()
frozen = json.loads((s / 'build-source-identities.json').read_text())
assert hashlib.sha256(old).hexdigest() == row['original_sha256'], name
assert hashlib.sha256(new).hexdigest() == frozen[name], name
diff = list(difflib.unified_diff(old.decode().splitlines(keepends=True), new.decode().splitlines(keepends=True)))
body = ''.join('@@\n' if line.startswith('@@') else line for line in diff[2:])
patch = '*** Begin Patch\n*** Update File: /Users/michael/Code/deadpan/' + name + '\n' + body + '*** End Patch'
record = {'path': name, 'before_sha256': hashlib.sha256(old).hexdigest(), 'after_sha256': hashlib.sha256(new).hexdigest()}
(q / (name.replace('/', '__') + '.adoption.json')).write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'patch': patch, 'record': record}))
