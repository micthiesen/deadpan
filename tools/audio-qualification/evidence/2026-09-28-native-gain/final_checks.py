"""Serial final stages; run native inspection between base and release stages."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
runner = Path('/tmp/deadpan-sound-allowances-h1t6i96x/run.py')
environment = dict(os.environ, DEADPAN_CHECK_OUTPUT=str(root))
stage = sys.argv[1]
live_keys = '/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift'
if stage == 'ui':
    artifact = sys.argv[2]
    commands = [
        ('final-ui-app-tests', ['python3', str(root / 'feature_tests.py'), artifact]),
        ('full-ui-visual', ['target/debug/deadpan-app', '--ui-check', '--output', str(root / 'full-ui-visual'), '--kestrel-source', live_keys]),
    ]
elif stage == 'base':
    commands = [
        ('final-base-clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']),
        ('final-base-artifacts', ['cargo', 'test', '--workspace', '--locked', '--no-run', '--message-format=json']),
        ('final-base-app-tests', ['python3', str(root / 'feature_tests.py'), 'final-base-artifacts', 'base']),
    ]
elif stage == 'release':
    commands = [
        ('release-build', ['cargo', 'build', '--workspace', '--release', '--features', 'deadpan-app/ui-harness', '--locked']),
        ('full-ui-performance', ['target/release/deadpan-app', '--ui-check', '--mode', 'performance', '--output', str(root / 'full-ui-performance'), '--kestrel-source', live_keys]),
    ]
else:
    raise ValueError(stage)
for name, command in commands:
    assert not (root / (name + '.json')).exists(), name
    print(json.dumps({'starting': name, 'command': command}), flush=True)
    result = subprocess.run(['python3', str(runner), name, *command], env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    report_path = root / (name + '.json')
    if report_path.exists():
        report = json.loads(report_path.read_text())
        print(json.dumps({'name':name,'exit_code':report.get('exit_code'),'seconds':report.get('seconds'),'source':report['source_manifest_sha256']}), flush=True)
    if result.returncode:
        print(result.stdout[-12000:], flush=True)
        raise SystemExit(result.returncode)
print('Stage passed: ' + stage, flush=True)
