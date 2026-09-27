from pathlib import Path

repo=Path('/Users/michael/Code/deadpan')
replacements={
'AGENTS.md':[
('splits root beats at the cursor, wraps/updates root-beat Repeats, deletes root beats, changes existing root Hold durations', 'navigates ordinary Sequence groups with Enter/Backspace, splits their direct children at the cursor, wraps/updates Repeats, deletes selected beats, changes existing Hold durations'),
('history, root-beat Camera previews', 'history, current-depth Camera previews'),
('Root edits capture session and revision, reject hidden descendants, and resolve through core/store commands.', 'Beat edits capture session, revision, absolute cursor and Sequence scope; reject non-direct targets and resolve through core/store commands.'),
('Native completion carries its exact cursor and selects the visible root group for a hidden Hold.', 'Native completion carries its exact cursor and scope and selects the visible enclosing child for a hidden Hold.'),
],
'README.md':[
('[root-beat Split](docs/STRUCTURAL_SPLIT.md)', '[current-depth Split](docs/STRUCTURAL_SPLIT.md)'),
],
'docs/REQUIREMENTS.md':[
('text/IME suppression, root `s`/`rr`/`dd`, exact Hold commands and history.', 'text/IME suppression, Sequence Enter/Backspace and breadcrumbs, current-depth `s`/`rr`/`dd`, exact Hold commands and history.'),
('Native root-beat Split/Repeat/delete/Hold-duration edits', 'Native current-depth Split/Repeat/delete/Hold-duration edits'),
('per-play escalation, nested native selection,', 'per-play escalation, native occurrence selection,'),
],
}
for name,pairs in replacements.items():
    path=repo/name
    text=path.read_text()
    for old,new in pairs:
        if text.count(old)!=1:
            raise SystemExit(f'{name}: expected one anchor, found {text.count(old)}: {old}')
        text=text.replace(old,new)
    path.write_text(text)
