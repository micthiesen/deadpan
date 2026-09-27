import hashlib
import json
from pathlib import Path

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-moment-paste-20260926')
gate = json.loads((scratch / 'gate-3/report.json').read_text())
remaining = json.loads((scratch / 'remaining/report.json').read_text())
source = json.loads((scratch / 'gate-3/source-after.json').read_text())
earlier = json.loads((scratch / 'gate-2/source-after.json').read_text())
changed_after_gate2 = [p for p in sorted(set(source) | set(earlier)) if source.get(p) != earlier.get(p)]
assert changed_after_gate2 == ['crates/deadpan-app/src/navigation/kestrel-reserved.tsv']
gui = {}
for directory in ('ui-moment', 'ui-moment-performance', 'ui-moment-final-visual', 'ui-moment-final-performance'):
    path = scratch / directory / 'report.json'
    if not path.exists():
        gui[directory] = {'report_missing': True}
        continue
    report = json.loads(path.read_text())
    scenario = next(s for s in report['scenarios'] if s['name'] == 'original-moment')
    shortcut = next(s for s in report['scenarios'] if s['name'] == 'kestrel-shortcuts')
    gui[directory] = {
        'failed': report['failed'], 'mode': report['mode'],
        'scenario_checks': len(scenario['checks']), 'scenario_steps': len(scenario['steps']),
        'screenshots': len(list(path.parent.glob('*.png'))),
        'findings': scenario['findings'], 'shortcut_passed': not shortcut['failed'],
        'shortcut_checked': shortcut['checks'][0]['actual']['routing_cases'],
        'shortcut_reservations': shortcut['checks'][0]['actual']['reserved_bindings'],
        'shortcut_conflicts': shortcut['checks'][0]['actual']['conflicts'],
        'live_source_sha256': shortcut['checks'][0]['actual'].get('live_source_sha256'),
        'binary_sha256': report['metadata']['binary_sha256'],
    }
result = {
    'final_gate': gate, 'remaining': remaining, 'gui': gui,
    'final_ui': json.loads((scratch / 'final-ui/report.json').read_text()),
    'changed_after_gate2': changed_after_gate2,
    'non_app_sources_unchanged_since_remaining_tests': True,
    'sources_match_final_gate': all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == h for p, h in source.items()),
    'source_count': len(source), 'git_write_access': 'read-only; no commit or push',
    'no_requirement_or_gate_promoted': True,
}
(scratch / 'coverage.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
