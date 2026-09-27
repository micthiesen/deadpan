from pathlib import Path
r=Path('/Users/michael/Code/deadpan')
changes={
 'README.md': [('repeat/delete/Hold-duration commands, durable undo/redo', 'repeat/delete/Hold-duration commands, [exact speed and pitch editing](docs/RETIME_EDITING.md), durable undo/redo')],
 'docs/ARCHITECTURE.md': [('root-beat Split/Repeat/delete/Hold-duration commands, atomic Source/Hold pause insertion', 'current-depth Split/Repeat/delete/Hold-duration and Retime commands, atomic Source/Hold pause insertion')],
 'docs/REQUIREMENTS.md': [('wrap/update structural repeats, and change Hold duration/provider.', 'wrap/update structural repeats and [Retimes](RETIME_EDITING.md), and change Hold duration/provider.'), ('Native current-depth Split/Repeat/delete/Hold-duration edits use that path', 'Native current-depth Split/Repeat/Retime/delete/Hold-duration edits use that path')],
}
for name,pairs in changes.items():
 p=r/name;text=p.read_text()
 for before,after in pairs:
  assert text.count(before)==1,(name,before,text.count(before))
  text=text.replace(before,after)
 p.write_text(text)
