from pathlib import Path
import hashlib, json

scratch=Path('/tmp/deadpan-group-navigation-20260926')
report=json.loads((scratch/'ui-nested/report.json').read_text())
scenario=next(row for row in report['scenarios'] if row['name']=='nested-pause')
shortcut=next(row for row in report['scenarios'] if row['name']=='kestrel-shortcuts')
actual=shortcut['checks'][0]['actual']
result=dict(exit_code=1,failed=report['failed'],scenario_checks=len(scenario['checks']),
    scenario_steps=len(scenario['steps']),screenshots=len(list((scratch/'ui-nested').glob('*.png'))),
    findings=scenario['findings'],shortcut_passed=not shortcut['failed'],
    shortcut_checked=actual['routing_cases'],shortcut_reservations=actual['reserved_bindings'],
    shortcut_conflicts=actual['conflicts'],live_source_sha256=actual['live_source_sha256'],
    binary_sha256=report['metadata']['binary_sha256'],
    run_stage='Before review cursor fix and keyboard regression additions; zero GUI assertions at any stage')
(scratch/'gui-exit.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
