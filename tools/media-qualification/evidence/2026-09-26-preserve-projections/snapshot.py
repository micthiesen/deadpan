import hashlib
import json
import shutil
import subprocess
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
out = Path('/tmp/deadpan-preserve-projection-20260926')
previous = Path('/tmp/deadpan-audio-projection-20260926')
sources = json.loads((previous / 'gate-1/source-after.json').read_text())
for name, digest in sources.items():
    assert hashlib.sha256((repo / name).read_bytes()).hexdigest() == digest, name
paths = set(sources) | {'AGENTS.md', 'docs/STRUCTURAL_SPLICE_DESIGN.md',
    'docs/AUDIO_INPUT_TAPES.md', 'docs/REQUIREMENTS.md', 'docs/spec/AGENT_HANDOFF.md',
    'docs/spec/DEADPAN_SPEC.md', 'docs/OWNED_AUDIO_CLOCKS.md'}
for name in sorted(paths):
    destination = out / 'before' / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(repo / name, destination)
head = subprocess.check_output(['git', '-c', 'core.fsmonitor=false', 'rev-parse', 'HEAD'], cwd=repo).decode().strip()
(out / 'baseline.json').write_text(json.dumps(dict(head=head, source_count=len(sources),
    source_hashes=sources, previous_checkpoint=str(previous / 'checkpoint')), indent=2) + '\n')
print(json.dumps(dict(source_count=len(sources), previous_gate_matches=True, head=head)))
