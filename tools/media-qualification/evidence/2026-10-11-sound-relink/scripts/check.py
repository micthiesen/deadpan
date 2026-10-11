from pathlib import Path
import gzip
import hashlib
import json
import re
import subprocess
import sys

repo = Path('/Users/michael/Code/deadpan')
root = repo/'tools/media-qualification/evidence/2026-10-11-sound-relink'
committed = '--committed' in sys.argv
def read(path):
    if committed:
        return subprocess.check_output(['git', 'show', f'HEAD:{path.relative_to(repo)}'], cwd=repo)
    return path.read_bytes()
count = 0
for line in read(root/'SHA256SUMS').decode().splitlines():
    digest, name = line.split('  ', 1)
    assert hashlib.sha256(read(root/name)).hexdigest() == digest, name
    count += 1
for name, digest in json.loads(read(root/'source-identity.json'))['files'].items():
    assert hashlib.sha256(read(repo/name)).hexdigest() == digest, name
for row in json.loads(read(root/'compression.json')):
    data = gzip.decompress(read(root/row['path']))
    assert len(data) == row['original_bytes']
    assert hashlib.sha256(data).hexdigest() == row['original_sha256']
docs = ['docs/COMMANDS.md', 'docs/ORIGINAL_MEDIA.md', 'docs/RECOVERY.md',
        'docs/REQUIREMENTS.md', 'docs/SINGLE_ORIGINAL.md', 'docs/SOUND_EVENTS.md',
        'docs/UI_FEEDBACK.md', 'docs/spec/AGENT_HANDOFF.md',
        'docs/KEYBINDING_COMPATIBILITY.md', 'docs/qualification/sound-recovery-2026-10-11.md',
        str((root/'README.md').relative_to(repo))]
links = 0
for name in docs:
    path = repo/name
    for target in re.findall(r'\]\(([^)]+)\)', read(path).decode()):
        if '://' in target or target.startswith('#'):
            continue
        assert (path.parent/target.split('#')[0].strip('<>')).exists(), (name, target)
        links += 1
report = json.loads(gzip.decompress(read(root/'replays-final-3/sound-relink/report.json.gz')))
assert not report['failed']
assert all(check['passed'] for scenario in report['scenarios'] for check in scenario['checks'])
print(f'Verified {count} evidence files, compressed originals, source hashes and {links} local links; committed={committed}')
