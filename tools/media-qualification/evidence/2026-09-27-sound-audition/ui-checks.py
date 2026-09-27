"""Run both production replay modes; retain an independent report for each failure."""
from pathlib import Path
import json, subprocess

scratch = Path('/tmp/deadpan-authored-sounds-20260927')
gate = json.loads((scratch / 'gate-01/report.json').read_text())
assert len(gate['commands']) == 7
assert all(gate['commands'][i]['exit_code'] == 0 for i in [3, 5, 6])
for mode in ('visual', 'performance'):
    command = ['cargo', 'run', '-p', 'deadpan-app']
    if mode == 'performance':
        command.append('--release')
    command += ['--features', 'ui-harness', '--locked', '--', '--ui-check', '--mode', mode, '--scenario', 'sound-playback', '--kestrel-source', '/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift', '--output', str(scratch / f'ui-{mode}')]
    subprocess.run(['python3', str(scratch / 'run-checks.py'), str(scratch / f'ui-{mode}-command'), json.dumps([command])], check=False)
subprocess.run(['python3', str(scratch / 'final-check.py')], check=True)
