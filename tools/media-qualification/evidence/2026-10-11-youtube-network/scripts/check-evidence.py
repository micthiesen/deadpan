from pathlib import Path
import gzip
import hashlib
import json
import re

repo=Path('/Users/michael/Code/deadpan')
for folder in ['2026-10-11-mux-preroll','2026-10-11-youtube-network']:
    evidence=repo/'tools/media-qualification/evidence'/folder
    for line in (evidence/'SHA256SUMS').read_text().splitlines():
        digest,name=line.split('  ',1)
        assert hashlib.sha256((evidence/name).read_bytes()).hexdigest()==digest,(folder,name)
    for name,digest in json.loads((evidence/'source-identity.json').read_text())['files'].items():
        assert hashlib.sha256((repo/name).read_bytes()).hexdigest()==digest,name
    for row in json.loads((evidence/'compression.json').read_text()):
        data=gzip.decompress((evidence/row['path']).read_bytes())
        assert len(data)==row['original_bytes'] and hashlib.sha256(data).hexdigest()==row['original_sha256']
names=['docs/PRIVACY.md','docs/ADVERSARIAL.md','docs/YOUTUBE_IMPORT.md','docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md',
       'docs/qualification/youtube-network-2026-10-11.md','docs/qualification/mux-preroll-2026-10-11.md','docs/qualification/youtube-acceptance-2026-10-11.md',
       'crates/deadpan-cli/tests/fixtures/youtube-connect.md',
       'tools/media-qualification/evidence/2026-10-11-youtube-network/README.md','tools/media-qualification/evidence/2026-10-11-mux-preroll/README.md']
count=0
for name in names:
    source=repo/name
    for link in re.findall(r'\]\(([^)]+)\)',source.read_text()):
        if '://' in link or link.startswith('#'):continue
        assert (source.parent/link.split('#')[0].strip('<>')).exists(),(name,link)
        count+=1
print('Verified both evidence manifests, compressed originals, reviewed source hashes and',count,'local documentation links')
