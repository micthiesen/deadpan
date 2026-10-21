from pathlib import Path
import gzip
import hashlib
import json

scratch = Path('/tmp/deadpan-scoped-mute-2026-10-20')
output = Path('tools/media-qualification/evidence/2026-10-21-scoped-mute-macros')
output.mkdir(exist_ok=True)

def keep(source, name=None, compress=False):
    source = Path(source)
    name = name or source.name
    content = source.read_bytes()
    if compress:
        content = gzip.compress(content, mtime=0)
        name += '.gz'
    (output / name).write_bytes(content)

for path in scratch.iterdir():
    if path.is_file() and path.suffix in ['.log', '.json', '.sh', '.txt', '.diff', '.exit', '.py']:
        keep(path, compress=path.suffix in ['.log', '.diff'])
keep(scratch / 'old-witness/scoped_mute.rs', 'old-witness-scoped-mute.rs', True)

sources = json.loads((scratch / 'source-identities.json').read_text())
for name, expected in sources.items():
    assert hashlib.sha256(Path(name).read_bytes()).hexdigest() == expected, name
    if name.endswith(('tests/scoped_mute.rs', 'scoped/mute.rs', 'scoped_plays/mute_macros.rs')):
        keep(name, name.replace('/', '__'), True)

captures = []
summary = {}
first = json.loads((scratch / 'visual-first/report.json').read_text())
assert first['failed'] is True
keep(scratch / 'visual-first/report.json', 'visual-first-report.json', True)
keep(scratch / 'visual-first/scoped-plays-129.png', 'visual-first-scoped-plays-129.png')
captures.append({'file': 'visual-first-scoped-plays-129.png', 'inspected_by': 'root',
                 'result': 'Long record hint prunes macro execute hint at 960x640; fixed with concise record/run labels.'})
for mode in ['visual', 'performance']:
    report = json.loads((scratch / mode / 'report.json').read_text())
    assert report['failed'] is False, mode
    keep(scratch / mode / 'report.json', mode + '-report.json', True)
    scenario = next(s for s in report['scenarios'] if s['name'] == 'scoped-plays')
    summary[mode] = {
        'failed': report['failed'], 'checks': len(scenario['checks']),
        'warnings': [f['message'][:200] for f in scenario.get('findings', [])],
        'timings': {t['name']: t['summary'] for t in scenario['timings']
                    if t['name'] in ['input_to_state_ms', 'input_to_picture_complete_ms']},
    }
    if mode == 'visual':
        labels = ['One-play mute macros retain explicit true and false intent',
                  'One-play mute recording retains visible keys at minimum size',
                  'One-play gain recording retains visible keys at minimum size']
        for label in labels:
            step = next(s for s in scenario['steps'] if s['input'] == label and s.get('screenshot'))
            name = mode + '-' + step['screenshot']
            keep(scratch / mode / step['screenshot'], name)
            captures.append({'file': name, 'checkpoint': label, 'inspected_by': 'root'})
(output / 'captures.json').write_text(json.dumps(captures, indent=2) + '\n')
(output / 'replay-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
manifest = {p.name: {'bytes': p.stat().st_size,
                     'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in sorted(output.iterdir()) if p.is_file() and p.name != 'inventory.json'}
(output / 'inventory.json').write_text(json.dumps(manifest, indent=2) + '\n')
for name, item in manifest.items():
    assert hashlib.sha256((output / name).read_bytes()).hexdigest() == item['sha256']
print(f'{len(manifest)} evidence files, {sum(x["bytes"] for x in manifest.values())} bytes; inventory verified')
