from pathlib import Path
import hashlib
import json
import subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
command = ['python3', str(root / 'run.py'), 'build-before', 'cargo', 'build', '--locked', '-p', 'deadpan-encode', '--example', 'qualify_hdr']
subprocess.run(command, check=True)
binary = repo / 'target/debug/examples/qualify_hdr'
(root / 'before-binary.json').write_text(json.dumps(dict(path=str(binary), sha256=hashlib.sha256(binary.read_bytes()).hexdigest()), indent=2)+'\n')
for attempt in range(1, 11):
    output = root / f'capture-{attempt}'
    output.mkdir()
    result = subprocess.run(['python3', str(root / 'run.py'), f'capture-{attempt}', str(binary), str(output), '640', '360', '30', '1'])
    assert result.returncode == 0
    log = (root / f'capture-{attempt}.log').read_text()
    if 'aggregate declared media samples exceed admission bound' in log:
        print('Retained the failing emitted file in', output, flush=True)
        break
else:
    raise RuntimeError('No correction path triggered in ten runs; outputs retained')
