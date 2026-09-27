"""Run one production UI scenario and retain its actual outcome without masking failure."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-nested-sequence-20260926')
assert (scratch / 'gate-1/report.json').is_file(), 'Finish the serial gate first'
assert not (scratch / 'gui').exists(), 'Never reuse visual output'
command = ['cargo', 'run', '--locked', '-p', 'deadpan-app', '--features',
           'ui-harness', '--', '--ui-check', '--output', str(scratch / 'gui'),
           '--scenario', 'nested-pause']
started = time.monotonic()
with (scratch / 'gui.log').open('w') as log:
    result = subprocess.run(command, cwd=repo,
                            env=dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix'),
                            stdout=log, stderr=subprocess.STDOUT)
record = dict(command=command, exit_code=result.returncode,
              seconds=time.monotonic()-started)
report_path = scratch / 'gui/report.json'
if report_path.exists():
    report = json.loads(report_path.read_text())
    scenario = next(s for s in report['scenarios'] if s['name'] == 'nested-pause')
    shortcuts = next(s for s in report['scenarios'] if s['name'] == 'kestrel-shortcuts')
    audit = shortcuts['checks'][0]
    record.update(scenario_checks=len(scenario['checks']),
                  scenario_steps=len(scenario['steps']),
                  scenario_failed_checks=sum(not c['passed'] for c in scenario['checks']),
                  scenario_findings=scenario['findings'],
                  screenshots=len(list((scratch/'gui').rglob('*.png'))),
                  shortcut_passed=audit['passed'],
                  shortcut_checked=audit['actual'].get('routing_cases'),
                  shortcut_reservations=audit['actual'].get('reserved_bindings'),
                  shortcut_conflicts=audit['actual'].get('conflicts'))
(scratch / 'gui-exit.json').write_text(json.dumps(record, indent=2)+'\n')
print(json.dumps(record), flush=True)
sys.exit(result.returncode)
