from pathlib import Path
import hashlib, json, shutil
root = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-selected-hold-duration-next-2026-10-21')
listing = json.loads((scratch/'baseline-list.json').read_text())
binary = Path(listing['rust-suites']['deadpan-app::bin/deadpan-app']['binary-path'])
frozen_binary = scratch/'baseline-app-tests'
shutil.copy2(binary, frozen_binary)
sources = {}
for path in [*root.joinpath('crates/deadpan-core/src').rglob('*.rs'),
             *root.joinpath('crates/deadpan-app/src').rglob('*.rs')]:
    relative = path.relative_to(root)
    target = scratch/'baseline-source'/relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, target)
    sources[str(relative)] = hashlib.sha256(target.read_bytes()).hexdigest()
result = {
    'base_commit': 'f384a85810f51be4d52ebfbc86b24071ef111268',
    'binary': str(binary),
    'executed_binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'frozen_binary': str(frozen_binary),
    'frozen_binary_sha256': hashlib.sha256(frozen_binary.read_bytes()).hexdigest(),
    'expected_failure': 'final public hold_parameter is None instead of Some("hold-duration 2f")',
    'positive_controls': 'qualified Original/RoomTone; selected identities; exact pools 4/0; staged 45->47->45; changed recipes; final full-Compound mapping; exact inverse and JSON; unchanged workspace',
    'source_sha256': sources,
}
assert result['executed_binary_sha256'] == result['frozen_binary_sha256']
(scratch/'baseline.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({key:value for key,value in result.items() if key != 'source_sha256'},indent=2))
print(f'Frozen {len(sources)} Rust source files')
