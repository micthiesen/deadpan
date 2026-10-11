from pathlib import Path
import gzip
import hashlib
import json
import re

repo = Path('/Users/michael/Code/deadpan')
evidence = repo / 'tools/media-qualification/evidence/2026-10-11-youtube-limits'
for line in (evidence / 'SHA256SUMS').read_text().splitlines():
    digest, name = line.split('  ', 1)
    assert hashlib.sha256((evidence / name).read_bytes()).hexdigest() == digest
identity = json.loads((evidence / 'source-identity.json').read_text())
for name, digest in identity['files'].items():
    assert hashlib.sha256((repo / name).read_bytes()).hexdigest() == digest
for row in json.loads((evidence / 'compression.json').read_text()):
    data = gzip.decompress((evidence / row['path']).read_bytes())
    assert len(data) == row['original_bytes']
    assert hashlib.sha256(data).hexdigest() == row['original_sha256']
checked = 0
for name in ['docs/REQUIREMENTS.md', 'docs/spec/AGENT_HANDOFF.md', 'docs/YOUTUBE_IMPORT.md',
             'docs/qualification/youtube-limits-2026-10-11.md',
             str((evidence / 'README.md').relative_to(repo))]:
    source = repo / name
    for link in re.findall(r'\]\(([^)]+)\)', source.read_text()):
        if '://' in link or link.startswith('#'):
            continue
        assert (source.parent / link.split('#')[0].strip('<>')).exists(), (name, link)
        checked += 1
print(f'Verified manifest, compressed originals, reviewed source hashes and {checked} local documentation links')
