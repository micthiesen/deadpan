from pathlib import Path
import subprocess

root = Path(__file__).parent

def run(name, args, pin=False):
    subprocess.run(['python3', str(root/'run.py'), name, *args], check=True)
    if pin:
        subprocess.run(['python3', str(root/'pin-binaries.py'), name], check=True)

run('format-final', ['cargo', 'fmt', '--all'])
run('transport-final', ['cargo', 'test', '--locked', '-p', 'deadpan-app', '--features', 'ui-harness', '--bin', 'deadpan-app', 'transport::'], True)
run('controller-final', ['cargo', 'test', '--locked', '-p', 'deadpan-playback', 'preparation_progress'], True)
run('input-admission-final', ['cargo', 'test', '--locked', '-p', 'deadpan-audio', '--lib', 'shortened_streamed_output_still_admits_unused_input_dependencies'], True)
run('clippy-final', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings'])
run('format-check-final', ['cargo', 'fmt', '--all', '--', '--check'])
run('sound-replay-final', ['cargo', 'xtask', 'replays', '--scenario', 'sound-playback', '--output', str(root/'sound-replay-final'), '--jobs', '1'])
