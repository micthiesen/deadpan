"""Run one bounded UI-fix gate, preserving terminal results under unique names."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
runner = Path('/tmp/deadpan-sound-allowances-h1t6i96x/run.py')
prefix, visual_name = sys.argv[1:3]
scenarios = sys.argv[3:] or ['gain']
environment = dict(os.environ, DEADPAN_CHECK_OUTPUT=str(root))
commands = [
    (prefix + '-format', ['cargo', 'fmt', '--all']),
    (prefix + '-format-check', ['cargo', 'fmt', '--all', '--', '--check']),
    (prefix + '-clippy-ui', ['cargo', 'clippy', '--workspace', '--all-targets', '--features', 'deadpan-app/ui-harness', '--locked', '--', '-D', 'warnings']),
    (prefix + '-ui-artifacts', ['cargo', 'test', '--workspace', '--features', 'deadpan-app/ui-harness', '--locked', '--no-run', '--message-format=json']),
    (prefix + '-ui-app-tests', ['python3', str(root / 'feature_tests.py'), prefix + '-ui-artifacts']),
]
for scenario in scenarios:
    name = visual_name if len(scenarios) == 1 else visual_name + '-' + scenario
    commands.append((name, ['target/debug/deadpan-app', '--ui-check', '--scenario', scenario, '--retain-projects', '--output', str(root / name), '--kestrel-source', '/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift']))
for name, command in commands:
    assert not (root / (name + '.json')).exists(), name
    print(json.dumps({'starting': name, 'command': command}), flush=True)
    result = subprocess.run(['python3', str(runner), name, *command], env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    report_path = root / (name + '.json')
    if report_path.exists():
        print(report_path.read_text(), flush=True)
    if result.returncode:
        print(result.stdout[-12000:], flush=True)
        raise SystemExit(result.returncode)
print('Focused UI gates passed.', flush=True)
