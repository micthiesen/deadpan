from pathlib import Path
import hashlib, json, subprocess
root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
for name, args in [
    ('tail-format', ['cargo', 'fmt', '--all']),
    ('tail-format-check', ['cargo', 'fmt', '--all', '--check']),
    ('tail-transport', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-app', 'transport::', '--', '--test-threads=1']),
    ('tail-worker', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-app', 'worker::', '--', '--test-threads=1']),
    ('tail-clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings']),
]:
    subprocess.run(['python3', str(root / 'run.py'), name, *args], check=True)
names = json.loads((root / 'measurements/identity.json').read_text())['source_sha256']
hashes = {name: hashlib.sha256((repo / name).read_bytes()).hexdigest() for name in names}
(root / 'final-source.json').write_text(json.dumps(hashes, indent=2) + '\n')
