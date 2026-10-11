from pathlib import Path
import subprocess
root = Path(__file__).parent
for name, args in [
    ('format-3', ['cargo', 'fmt', '--all']),
    ('format-final-2', ['cargo', 'fmt', '--all', '--check']),
    ('clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings']),
    ('perf-build-final', ['cargo', 'build', '--locked', '--release', '-p', 'deadpan-cli', '--example', 'perf']),
]:
    subprocess.run(['python3', str(root / 'run.py'), name, *args], check=True)
