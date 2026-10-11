from pathlib import Path
import subprocess

root = Path(__file__).parent
def run(name, args):
    subprocess.run(['python3', str(root/'run.py'), name, *args], check=True)

run('layout-format', ['cargo', 'fmt', '--all'])
run('layout-clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings'])
run('layout-format-check', ['cargo', 'fmt', '--all', '--', '--check'])
run('sound-layout-replay', ['cargo', 'xtask', 'replays', '--scenario', 'sound-playback', '--output', str(root/'sound-layout-replay'), '--jobs', '1'])
