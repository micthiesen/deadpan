import difflib
import hashlib
import json
import re
import subprocess
from pathlib import Path
from urllib.parse import unquote

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-original-audition-20260927')
before = scratch / 'before'
all_paths = subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=repo).decode().split('\0')
paths = sorted({p for p in all_paths if p and (p.startswith(('crates/', 'native/')) or p in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'rustfmt.toml', 'README.md', 'AGENTS.md') or (p.startswith('docs/') and p.endswith('.md')))})
rows = []
patch = []
for name in paths:
    path = repo / name
    if not path.is_file():
        continue
    data = path.read_bytes()
    old = (before / name).read_bytes() if (before / name).is_file() else b''
    if data == old:
        continue
    rows.append({'path': name, 'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)})
    patch.extend(difflib.unified_diff(old.decode().splitlines(True), data.decode().splitlines(True), fromfile='before/' + name, tofile='after/' + name))
(scratch / 'increment.json').write_text(json.dumps(rows, indent=2) + '\n')
(scratch / 'increment.diff').write_text(''.join(patch))
review = json.loads((scratch / 'review.json').read_text())
assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in review['source_sha256'].items()), 'Source changed after review snapshot'
missing = []
checked = 0
for row in rows:
    if not row['path'].endswith('.md'):
        continue
    path = repo / row['path']
    for target in re.findall(r'\]\(([^)]+)\)', path.read_text()):
        target = target.strip('<>')
        if '://' in target or target.startswith(('#', 'mailto:')):
            continue
        target = re.sub(r':\d+$', '', unquote(target.split('#')[0]))
        if not target:
            continue
        checked += 1
        if not (path.parent / target).exists():
            missing.append({'path': row['path'], 'target': target})
links = {'documents': sum(r['path'].endswith('.md') for r in rows), 'local_file_targets': checked, 'missing': missing}
(scratch / 'doc-links.json').write_text(json.dumps(links, indent=2) + '\n')
assert not missing, missing
design = json.loads((repo / 'docs/design/manifest.json').read_text())
for row in design['images']:
    assert hashlib.sha256((repo / 'docs/design' / row['image']).read_bytes()).hexdigest() == row['sha256']
    assert hashlib.sha256((repo / 'docs/design' / row['prompt']).read_bytes()).hexdigest() == row['prompt_sha256']
print(json.dumps({'changed_paths': len(rows), 'reviewed_source_paths': len(review['source_sha256']), 'doc_links': links, 'design_boards_and_prompts': len(design['images'])}))
