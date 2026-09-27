"""Check local file targets in this increment's documentation."""
import json
from pathlib import Path
import re
import sys

repo=Path('/Users/michael/Code/deadpan')
paths=['AGENTS.md','README.md','docs/GROUP_NAVIGATION.md','docs/NATIVE_WORKSPACE.md',
       'docs/FRAMING.md','docs/INSERT_TIME.md','docs/PLAYBACK.md','docs/UI_FEEDBACK.md',
       'docs/design/README.md','docs/spec/AGENT_HANDOFF.md','docs/REQUIREMENTS.md',
       'docs/qualification/group-navigation-2026-09-26.md',
       'tools/media-qualification/evidence/2026-09-26-group-navigation/README.md']
missing=[]
checked=0
for relative in paths:
    path=repo/relative
    for match in re.finditer(r'\[[^\]]*\]\(([^)]+)\)',path.read_text()):
        target=match[1].strip('<>').split('#',1)[0]
        if not target or '://' in target or target.startswith('mailto:'): continue
        checked+=1
        if not (path.parent/target).exists(): missing.append(dict(file=relative,target=target))
print(json.dumps(dict(files=len(paths),local_targets=checked,missing=missing,
    scope='File targets only; fragments and external URLs not checked'),indent=2))
sys.exit(bool(missing))
