from pathlib import Path
import hashlib
import json
import shutil
import subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
binary = root / 'bin'
binary.mkdir()
old = Path('/tmp/deadpan-generated-playback-performance-20261011/bin-verified')
for name in ('deadpan-app', 'deadpan-cli', 'deadpan-media-worker', 'deadpan-track', 'deadpan-transcribe'):
    shutil.copy2((repo / 'target/release' if name == 'deadpan-app' else old) / name, binary / name)
(root / 'binary-pins.json').write_text(json.dumps({str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in binary.iterdir()}, indent=2) + '\n')
for name in ('fixed-1', 'fixed-2'):
    command = ['time', '-l', str(binary / 'deadpan-app'), '--ui-check', '--scenario', 'generated-playback', '--mode', 'performance', '--hz', '120', '--project', str(root / 'two-holds.deadpan'), '--output', str(root / name)]
    subprocess.run(['python3', str(root / 'run.py'), name, *command], check=False)
subprocess.run(['python3', str(root / 'analyze.py'), 'baseline', 'fixed-1', 'fixed-2'], check=True)
