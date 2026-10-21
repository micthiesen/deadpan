from pathlib import Path
import gzip
import hashlib
import json
import shutil

scratch = Path('/tmp/deadpan-scoped-gain-2026-10-20')
output = Path('tools/media-qualification/evidence/2026-10-20-scoped-gain-macros')
output.mkdir()

def keep(source, name=None, compress=False):
    source = Path(source)
    name = name or source.name
    content = source.read_bytes()
    if compress:
        content = gzip.compress(content, mtime=0)
        name += '.gz'
    (output / name).write_bytes(content)

for path in scratch.iterdir():
    if not path.is_file():
        continue
    if path.suffix in ['.log', '.json', '.sh', '.txt', '.diff', '.exit']:
        keep(path, compress=path.suffix in ['.log', '.diff'])
for name in ['identity.py', 'pin-old.py', 'collect.py']:
    keep(scratch / name)
for name in ['native-gain-first-failure.rs', 'native-gain-second-failure.rs']:
    keep(scratch / name, compress=True)
keep(scratch / 'old-witness/scoped_gain.rs', 'old-witness-scoped-gain.rs', True)

sources = json.loads((scratch / 'source-identities.json').read_text())
for name, expected in sources.items():
    assert hashlib.sha256(Path(name).read_bytes()).hexdigest() == expected, name
    # The patch retains tracked files; these complete new files make it portable.
    if name.endswith(('planner/scoped_gain.rs', 'tests/scoped_gain.rs',
                      'scoped/gain.rs', 'scoped_plays/gain_macros.rs')):
        keep(name, name.replace('/', '__'), True)

captures = []
for mode in ['visual-first', 'performance-first', 'visual', 'performance']:
    report = json.loads((scratch / mode / 'report.json').read_text())
    assert report['failed'] is False, mode
    keep(scratch / mode / 'report.json', mode + '-report.json', True)
    scenario = next(s for s in report['scenarios'] if s['name'] == 'scoped-plays')
    if mode == 'visual':
        labels = ['One-play gain recording retains visible keys at minimum size',
                  'Relative gain dot retains the selected play and its independent peers']
    elif mode == 'visual-first':
        labels = ['Relative gain dot retains the selected play and its independent peers']
    else:
        labels = []
    for label in labels:
        step = next(s for s in scenario['steps'] if s['input'] == label and s.get('screenshot'))
        name = mode + '-' + step['screenshot']
        keep(scratch / mode / step['screenshot'], name)
        captures.append({'file': name, 'checkpoint': label,
                         'inspected_by': 'root',
                         'result': 'Stale pause/framing-only macro hints; corrected in final UI.'
                         if mode == 'visual-first' else
                         '960x640: selected play, clocks, gain controls, recording/dot and key hints legible.'})
(output / 'captures.json').write_text(json.dumps(captures, indent=2) + '\n')

reviews = json.loads((output / 'reviews.json').read_text())
reviews['final_ui'] = {'root_finding': 'First replay capture exposed stale pause/framing-only macro hints and missing gain recording key pair.',
                      'fix': 'Updated scoped macro hints and added configured gain keys during recording, with painted-text/footer checks and two 960x640 captures.',
                      'independent_recheck': 'gain_general_review found no concrete issue.'}
reviews['test_fixture_recheck'] = 'gain_verification_review found no success-unwrapping assumption remaining in expected-refusal paths.'
(output / 'reviews.json').write_text(json.dumps(reviews, indent=2) + '\n')
manifest = {p.name: {'bytes': p.stat().st_size,
                     'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in sorted(output.iterdir()) if p.is_file()}
(output / 'inventory.json').write_text(json.dumps(manifest, indent=2) + '\n')
for name, item in manifest.items():
    assert hashlib.sha256((output / name).read_bytes()).hexdigest() == item['sha256']
print(f'{len(manifest)} evidence files, {sum(x["bytes"] for x in manifest.values())} bytes; inventory verified')
