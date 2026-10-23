from pathlib import Path
import difflib
import hashlib
import json
import re

S = Path('/tmp/deadpan-current-pause-route-profile-2026-10-23')
ROOT = Path('/Users/michael/Code/deadpan')
SOURCE = S / 'source'
EXISTING = {
    'crates/deadpan-app/src/project/service.rs',
    'crates/deadpan-app/src/project/service/macros.rs',
    'crates/deadpan-store/src/compound.rs',
    'crates/deadpan-core/src/lib.rs',
}
NEW = 'crates/deadpan-core/src/pause_route_profile.rs'


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def save(name, value):
    (S / name).write_text(json.dumps(value, indent=2) + '\n')


def main():
    assert not (S / 'frozen-sources.json').exists(), 'retain an earlier freeze rather than replace it'
    baseline = json.loads((S / 'baseline-sources.json').read_text())
    assert len(baseline) == 2053
    assert all(sha(ROOT / p) == h for p, h in baseline.items()), 'main source drifted'
    current = {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob('*.rs')}
    assert set(current) - set(baseline) == {NEW}
    assert not set(baseline) - set(current)
    changed = {p for p, h in baseline.items() if current[p] != h}
    assert changed == EXISTING, changed
    inputs = json.loads((S / 'baseline-inputs.json').read_text())
    assert len(inputs) == 6 and all(sha(SOURCE / p) == sha(ROOT / p) == h for p, h in inputs.items())
    patches = []
    for p in sorted(changed | {NEW}):
        before = (ROOT / p).read_text() if p != NEW else ''
        after = (SOURCE / p).read_text()
        for label, text in [('before', before), ('after', after)]:
            dest = S / 'source-retention' / label / (p + '.txt')
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(text)
        patches.extend(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
                                            fromfile='a/' + p, tofile='b/' + p))
    (S / 'overlay.patch').write_text(''.join(patches))
    helper = (SOURCE / NEW).read_text()
    names = re.findall(r'#\[test\]\s*fn (\w+)', helper)
    assert names and len(names) == len(set(names))
    store = (SOURCE / 'crates/deadpan-store/tests/compound_preparation.rs').read_text()
    store_names = re.findall(r'#\[test\]\s*fn (\w+)', store)
    assert len(store_names) == len(set(store_names)) == 12
    save('expected-tests.json', {'helper-unit': ['pause_route_profile::tests::' + n for n in names],
                                 'store-preparation': store_names})
    save('frozen-sources.json', current)
    save('overlay-audit.json', {'baseline_commit': '40959c5e26f5fc9713eda5a02ccd61377cc65a28',
                               'code_commit': '5fdae748ac2fdc67e1a11793c5ab69f6b61da833',
                               'baseline_rust': len(baseline), 'private_rust': len(current),
                               'changed': sorted(changed), 'new': [NEW], 'inputs_unchanged': 6,
                               'source_map_sha256': sha(S / 'frozen-sources.json'),
                               'patch_sha256': sha(S / 'overlay.patch'),
                               'helper_tests': len(names), 'store_tests': len(store_names)})
    print(json.dumps({'source_map_sha256': sha(S / 'frozen-sources.json'),
                      'helper_tests': len(names), 'store_tests': 12}), flush=True)


if __name__ == '__main__':
    main()
