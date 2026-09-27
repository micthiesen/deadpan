from pathlib import Path

repo=Path('/Users/michael/Code/deadpan')
for name,old,new in [
    ('README.md','workflow with reversible root edits, limited sequence audition','workflow with reversible current-depth edits, limited sequence audition'),
    ('AGENTS.md','Cached insertion, root editing and history may proceed','Cached insertion, current-depth editing and history may proceed'),
    ('docs/REQUIREMENTS.md','captures an interior root-beat boundary and selects the committed right fragment.','captures an interior boundary in the selected child and selects the committed right fragment.'),
]:
    path=repo/name
    text=path.read_text()
    assert text.count(old)==1, (name,old)
    path.write_text(text.replace(old,new))
