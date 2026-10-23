from pathlib import Path
import difflib
import hashlib
import json
import re

root = Path(__file__).resolve().parents[1]
source = root / 'source'
notes = root / 'worker-notes'
digest = lambda data: hashlib.sha256(data).hexdigest()
baseline = json.loads((notes / 'before-all-file-identities.json').read_text())
owned = json.loads((notes / 'source-hashes.json').read_text())['changed_files']
current = {str(p.relative_to(source)): digest(p.read_bytes()) for p in source.rglob('*') if p.is_file()}
changed = sorted(p for p in baseline.keys() | current.keys() if baseline.get(p) != current.get(p))
assert changed == sorted(owned), changed
assert not (baseline.keys() - current.keys())
new = sorted(current.keys() - baseline.keys())
assert new == ['crates/deadpan-core/src/one_leaf_replay.rs', 'crates/deadpan-core/tests/semantic_leaf_replay.rs']
full_patch = []
production_patch = []
before = {}
proposed = {}
inventory = {}
for path in changed:
    after_data = (source / path).read_bytes()
    after_snapshot = notes / 'after' / (path + '.txt')
    after_snapshot.parent.mkdir(parents=True, exist_ok=True)
    after_snapshot.write_bytes(after_data)
    before[path] = baseline.get(path)
    proposed[path] = current[path]
    original = notes / 'before' / (path + '.txt')
    old = original.read_bytes() if path in baseline else b''
    if path in baseline:
        assert digest(old) == baseline[path], path
    patch = ''.join(difflib.unified_diff(old.decode().splitlines(keepends=True), after_data.decode().splitlines(keepends=True), fromfile='a/' + path if old else '/dev/null', tofile='b/' + path))
    full_patch.append(patch)
    if '/tests/' not in path and not path.endswith(('/tests.rs', '/audio_reference.rs', '/insert_time/retimed.rs')):
        production_patch.append(patch)
    old_tests = set(re.findall(r'#\[test\]\s*fn (one_leaf_replay_\w+)\(', old.decode()))
    names = [n for n in re.findall(r'#\[test\]\s*fn (one_leaf_replay_\w+)\(', after_data.decode()) if n not in old_tests]
    if names:
        inventory[path] = names
rust_baseline = {p: h for p, h in baseline.items() if p.endswith('.rs')}
rust_current = {p: h for p, h in current.items() if p.endswith('.rs')}
nonowned = {p for p in baseline if p not in changed}
assert all(baseline[p] == current[p] for p in nonowned)
inputs = json.loads((root / 'input-identities.json').read_text())
assert all(current[p] == h for p, h in inputs.items())
control = Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23/release-native-bin/deadpan-app')
control_hash = digest(control.read_bytes())
assert control_hash == 'a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f'
hashes = dict(before=before, proposed=proposed, baseline_rust=len(rust_baseline), proposed_rust=len(rust_current), all_nonowned_rust_unchanged=len(set(rust_baseline) & nonowned), all_baseline_files=len(baseline), all_nonowned_baseline_files_unchanged=len(nonowned), changed_files=changed, new_files=new, six_inputs_unchanged=True, control_app_sha256=control_hash, uncompiled=True, unformatted=True)
for path, data in [(notes / 'source-hashes.json', hashes), (root / 'proposed-source-identities.json', rust_current), (notes / 'all-proposed-file-identities.json', current)]:
    path.write_text(json.dumps(data, indent=2) + '\n')
(notes / 'prototype.patch').write_text(''.join(full_patch))
(notes / 'production-review.patch').write_text(''.join(production_patch))
tests = dict(new_core_tests=sum(len(v) for p, v in inventory.items() if '/deadpan-core/' in p), new_store_tests=sum(len(v) for p, v in inventory.items() if '/deadpan-store/' in p), names=inventory, executed=False)
(notes / 'test-inventory.json').write_text(json.dumps(tests, indent=2) + '\n')
artifacts = {}
for path in ['worker-notes/HANDOFF.md', 'worker-notes/prototype.patch', 'worker-notes/production-review.patch', 'worker-notes/source-hashes.json', 'worker-notes/test-inventory.json', 'original-source-identities.json', 'proposed-source-identities.json', 'input-identities.json', 'source-basis.json']:
    data = (root / path).read_bytes()
    artifacts[path] = dict(bytes=len(data), sha256=digest(data))
(notes / 'handoff-artifacts.json').write_text(json.dumps(artifacts, indent=2) + '\n')
print(json.dumps(dict(changed=len(changed), baseline_rust=len(rust_baseline), proposed_rust=len(rust_current), nonowned_rust=len(set(rust_baseline) & nonowned), tests=tests, patch_sha256=artifacts['worker-notes/prototype.patch']['sha256']), indent=2))
