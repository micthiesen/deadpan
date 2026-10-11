from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
steps = [
    ('network-tests-5', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-cli', '--lib', '--test', 'privacy_network_audit', '-E', 'test(youtube::) | binary(privacy_network_audit)']),
    ('debug-build', ['cargo', 'build', '--locked', '-p', 'deadpan-cli', '-p', 'deadpan-media-worker']),
    ('helper-probe', [str(repo / 'target/debug/deadpan-cli'), 'downloader', 'status', '--probe']),
    ('real-import', [str(repo / 'target/debug/deadpan-cli'), 'project', 'create-from-url', str(root / 'real.deadpan'), 'https://youtu.be/Z4C82eyhwgU']),
    ('real-validate', [str(repo / 'target/debug/deadpan-cli'), 'project', 'validate', str(root / 'real.deadpan')]),
]
for name, command in steps:
    print('START', name, flush=True)
    result = subprocess.run(['python3', str(root / 'run.py'), name, *command])
    if name == 'debug-build' and result.returncode == 0:
        (root / 'debug-binaries.json').write_text(json.dumps({name: hashlib.sha256((repo / 'target/debug' / name).read_bytes()).hexdigest() for name in ['deadpan-cli', 'deadpan-media-worker']}, indent=2) + '\n')
    if result.returncode:
        print((root / (name + '.log')).read_text(errors='replace')[-6000:], flush=True)
        sys.exit(result.returncode)
