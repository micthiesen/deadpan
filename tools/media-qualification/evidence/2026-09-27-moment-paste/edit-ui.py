from pathlib import Path
p=Path('crates/deadpan-app/src/preview.rs')
s=p.read_text()
edits={
'else if pending.is_empty() { "NORMAL" }':'else if self.moment.active && self.view == View::Source { "VISUAL" } else if pending.is_empty() { "NORMAL" }',
'else { 98.0 };':'else { 174.0 };',
'else if self.view == View::Sequence { 142.0 }':'else if self.view == View::Sequence { if self.moment.copied.is_some() { 188.0 } else { 142.0 } }',
'                self.playback_controls(ui);':'                self.moment_controls(ui);\n                self.playback_controls(ui);',
'Range reuse is not available yet.':'Select a range in Original with v, move with h/l, then copy with y.',
'Original playback, range cuts/reuse, Repeat/Retime':'Original playback, range cuts/replacement, named registers, Repeat/Retime',
'                        style::key_hint(ui, "u", "undo");':'                        if self.moment.copied.is_some() { style::key_hint(ui, "p / P", "paste after / before"); }\n                        style::key_hint(ui, "u", "undo");',
'                        style::key_hint(ui, ":sequence", if self.focused_workflow()':'                        style::key_hint(ui, "v", if self.moment.active { "finish selection" } else { "select moment" });\n                        style::key_hint(ui, "y", "copy moment");\n                        style::key_hint(ui, ":sequence", if self.focused_workflow()',
'                        (",i / :insert",':'                        ("v / :select · y / :yank", "In Original, start or finish a half-open time selection. Move with h/l or counted motions. y copies the range; Esc cancels selection. The Out frame is excluded. Copy is session-local; named and persistent registers are not available yet."),\n                        ("p / P · :paste / :paste-before", "In Your edit, paste the copied Original moment after / before the selected beat in the displayed group. An empty group accepts a paste at its start. Each paste is one undoable transaction; later audio keeps its sampling phase."),\n                        (",i / :insert",',
}
for a,b in edits.items():
    if s.count(a)!=1: raise SystemExit(f'expected one {a!r}, found {s.count(a)}')
    s=s.replace(a,b)
p.write_text(s)
