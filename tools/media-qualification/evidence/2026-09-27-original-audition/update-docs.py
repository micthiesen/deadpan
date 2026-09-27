from pathlib import Path
repo=Path('/Users/michael/Code/deadpan')
changes={
 'README.md': [('limited sequence audition and legacy reopening', 'limited Original/edit audition with selection loops, and legacy reopening')],
 'AGENTS.md': [('current-depth Camera previews and limited sequence audition are implemented', 'current-depth Camera previews and limited Original/edit audition with selection loops are implemented')],
 'docs/AUDIO_MASTERING.md': [('[Sequence audition](PLAYBACK.md)', '[Original/edit audition](PLAYBACK.md)')],
 'docs/DEVELOPMENT.md': [('Space plays/pauses [limited sequence audition](PLAYBACK.md). Run', 'Space plays/pauses [limited Original/edit audition](PLAYBACK.md); Shift+Space loops the selected moment or beat. Run')],
 'docs/spec/AGENT_HANDOFF.md': [('[sequence audition contract](../PLAYBACK.md)', '[Original/edit audition contract](../PLAYBACK.md)')],
 'docs/ARCHITECTURE.md': [('Limited sequence audition with bounded queues', 'Limited Original/edit audition and selection loops with bounded queues'), ('and limited sequence audition. Full editing', 'and limited Original/edit audition with selection loops. Full editing')],
 'docs/REQUIREMENTS.md': [('Limited [sequence audition](PLAYBACK.md)', 'Limited [Original/edit audition](PLAYBACK.md)'), ('Native sequence audition adds device-clock picture coalescing and exact paused-sample resume.', 'Native Original/edit audition adds device-clock picture coalescing, context-bounded selection loops and exact paused-sample resume.')],
 'docs/design/README.md': [('[Sequence audition](../PLAYBACK.md) has real Space Play/Pause beside the picture,', '[Original/edit audition](../PLAYBACK.md) has Space Play/Pause and Shift+Space selection loops beside the picture,')],
}
for name,edits in changes.items():
 p=repo/name;s=p.read_text()
 for old,new in edits:
  assert s.count(old)==1,(name,old,s.count(old))
  s=s.replace(old,new)
 p.write_text(s)
