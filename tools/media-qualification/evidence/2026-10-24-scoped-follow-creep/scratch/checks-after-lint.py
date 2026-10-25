#!/usr/bin/env python3
"""Sequential scoped verification, retaining every actual invocation."""
import json
import os
from pathlib import Path
import subprocess

s = Path(__file__).resolve().parent
os.environ['CARGO_TARGET_DIR'] = '/tmp/deadpan-follow-creep-2026-10-24/target'
os.environ['DEADPAN_FFMPEG_PREFIX'] = '/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
paths = json.loads((s / 'source-files.json').read_text())
lanes = [
    ('format-after-lint', ['rustfmt', '--check', '--edition', '2024', '--config',
                      'skip_children=true', *paths], 0),
    ('app-check-after-lint', ['cargo', 'check', '--locked', '-p', 'deadpan-app',
                         '--all-targets', '--features', 'ui-harness'], 0),
    ('clippy-after-lint', ['cargo', 'clippy', '--locked', '-p', 'deadpan-core',
                      '-p', 'deadpan-plan', '-p', 'deadpan-cli', '-p', 'deadpan-render',
                      '-p', 'deadpan-app', '--all-targets', '--features',
                      'deadpan-app/ui-harness', '--', '-D', 'warnings'], 0),
    ('commands-write', ['cargo', 'test', '--locked', '-p', 'deadpan-app',
                        '--features', 'ui-harness', 'commands_reference'], 101),
]
for label, command, expected in lanes:
    env = os.environ.copy()
    if label == 'commands-write':
        env['DEADPAN_UPDATE_COMMANDS_MD'] = '1'
    code = subprocess.run(['python', str(s / 'run.py'), label, '--', *command],
                          env=env, check=False).returncode
    if code != expected:
        raise SystemExit(f'{label}: actual {code}, expected {expected}; inspect retained streams')
