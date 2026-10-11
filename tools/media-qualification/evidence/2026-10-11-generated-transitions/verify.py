from pathlib import Path
import os
import subprocess

root = Path(__file__).parent
env = dict(os.environ,
           DEADPAN_GENERATED_TRANSITIONS_PACKAGE=str(root / 'two-holds.deadpan'),
           DEADPAN_GENERATED_PREVIEW_PACKAGE='/tmp/deadpan-generated-lookahead-20261011/conditioned.deadpan')
commands = [
    ('format', ['cargo', 'fmt', '--all']),
    ('fixture-tests', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-app', '--features', 'ui-harness', 'generated_playback_tests', '--', '--ignored', '--nocapture', '--test-threads=1']),
    ('final-worker-tests', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-app', '--features', 'ui-harness', 'worker::', '--', '--test-threads=1']),
    ('build', ['cargo', 'build', '--locked', '--release', '-p', 'deadpan-app', '--features', 'ui-harness']),
    ('clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings']),
    ('format-check', ['cargo', 'fmt', '--all', '--check']),
]
for name, command in commands:
    subprocess.run(['python3', str(root / 'run.py'), name, *command], env=env, check=True)
