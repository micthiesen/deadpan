from pathlib import Path
import subprocess
root = Path(__file__).parent
def run(name, *args):
    subprocess.run(['python3', str(root / 'run.py'), name, *args], check=True)
run('format-visual', 'cargo', 'fmt', '--all')
run('clippy-visual', 'cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings')
run('visual-replays', 'cargo', 'xtask', 'replays', '--scenario', 'diagnostics,storage', '--output', str(root / 'visual-replays'), '--jobs', '1')
