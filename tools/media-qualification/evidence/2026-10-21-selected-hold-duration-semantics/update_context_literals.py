from pathlib import Path
import re, json
root = Path('/Users/michael/Code/deadpan')
owned = {
    'crates/deadpan-app/src/project/scoped.rs',
    'crates/deadpan-app/src/project/service/macros.rs',
    'crates/deadpan-app/src/preview/macros.rs',
    'crates/deadpan-app/src/preview/semantic.rs',
    'crates/deadpan-app/src/preview/scoped/model.rs',
}
changed = {}
for file in root.joinpath('crates').rglob('*.rs'):
    relative = file.relative_to(root).as_posix()
    if relative.startswith('crates/deadpan-core/') or relative in owned:
        continue
    before = file.read_text()
    matches = list(re.finditer(r'^(\s*)mixed_hold_duration:(?!:)', before, re.MULTILINE))
    if not matches:
        continue
    assert 'selected_hold_duration:' not in before, relative
    after = re.sub(r'^(\s*)mixed_hold_duration:(?!:)',
                   lambda m: m.group(1)+'selected_hold_duration: None,\n'+m.group(1)+'mixed_hold_duration:(?!:)',
                   before, flags=re.MULTILINE)
    assert after.count('selected_hold_duration: None,') == len(matches), relative
    file.write_text(after)
    changed[relative] = len(matches)
Path('/tmp/deadpan-selected-hold-duration-next-2026-10-21/context-literals.json').write_text(json.dumps(changed,indent=2)+'\n')
print(json.dumps(changed,indent=2))
