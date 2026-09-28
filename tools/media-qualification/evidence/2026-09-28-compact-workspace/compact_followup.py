"""Resume only affected compact-workspace gates; keep completed app results."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
runner = Path('/tmp/deadpan-sound-allowances-h1t6i96x/run.py')
stage = sys.argv[1]
prefix = sys.argv[2]
if stage == 'ui':
    commands = [
        ('format', ['cargo', 'fmt', '--all']),
        ('format-check', ['cargo', 'fmt', '--all', '--', '--check']),
        ('clippy-ui', ['cargo', 'clippy', '--workspace', '--all-targets', '--features', 'deadpan-app/ui-harness', '--locked', '--', '-D', 'warnings']),
        ('ui-artifacts', ['cargo', 'test', '--workspace', '--features', 'deadpan-app/ui-harness', '--locked', '--no-run', '--message-format=json']),
    ]
    for scenario in sys.argv[3:] or ['sound-placement', 'room-tone', 'nested-pause', 'workspace', 'gain']:
        name = 'visual-' + scenario
        commands.append((name, ['target/debug/deadpan-app', '--ui-check', '--scenario', scenario, '--retain-projects', '--output', str(root / (prefix + '-' + name)), '--kestrel-source', '/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift']))
elif stage == 'base':
    commands = [
        ('clippy-base', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']),
        ('base-artifacts', ['cargo', 'test', '--workspace', '--locked', '--no-run', '--message-format=json']),
        ('base-app-tests', ['python3', str(root / 'feature_tests.py'), prefix + '-base-artifacts', 'base']),
    ]
elif stage == 'release':
    commands = [
        ('build', ['cargo', 'build', '--workspace', '--release', '--features', 'deadpan-app/ui-harness', '--locked']),
        ('performance', ['target/release/deadpan-app', '--ui-check', '--mode', 'performance', '--output', str(root / (prefix + '-performance')), '--kestrel-source', '/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift']),
    ]
else:
    raise ValueError(stage)
failed_visual = False
for suffix, command in commands:
    name = prefix + '-' + suffix
    assert not (root / (name + '.json')).exists(), name
    print(json.dumps({'starting': name, 'command': command}), flush=True)
    result = subprocess.run(['python3', str(runner), name, *command],
                            env=dict(os.environ, DEADPAN_CHECK_OUTPUT=str(root)),
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    report = json.loads((root / (name + '.json')).read_text())
    print(json.dumps({key: report[key] for key in ('exit_code', 'seconds', 'source_manifest_sha256')}), flush=True)
    if result.returncode:
        print(result.stdout[-12000:], flush=True)
        if stage == 'ui' and suffix.startswith('visual-'):
            failed_visual = True
            continue
        raise SystemExit(result.returncode)
if failed_visual:
    raise SystemExit(1)
print('Compact follow-up passed: ' + stage, flush=True)
