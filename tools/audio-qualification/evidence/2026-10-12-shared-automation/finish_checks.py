"""Run the final gates in sequence, never sharing a Cargo build with another run."""
from pathlib import Path
import subprocess
import sys

scratch = Path(__file__).resolve().parent
repo = '/Users/michael/Code/deadpan'

def run(name, command, cwd=repo, expected=0):
    result = subprocess.run([sys.executable, str(scratch / 'run.py'), name, cwd, *command])
    if result.returncode != expected:
        raise SystemExit(result.returncode or 1)

run('clippy-final', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings'])
run('fmt', ['cargo', 'fmt', '--all', '--check'])
run('whitespace', ['git', 'diff', '--check'])
subprocess.run([sys.executable, str(scratch / 'baseline.py')], check=True)
run('baseline-final', ['cargo', 'test', '--locked', '--message-format=json', '-p', 'deadpan-audio', '--test', 'audio_definition', 'sound_clocks::group_mix'], str(scratch / 'baseline'), 101)
run('export', ['cargo', 'test', '--release', '--locked', '--message-format=json', '-p', 'deadpan-cli', '--test', 'preview_export', 'post_shared_automation_preserves_preview_and_verified_export', '--', '--exact', '--nocapture'])
