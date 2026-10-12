from pathlib import Path
import subprocess
root = Path(__file__).parent

def run(name, *args):
    subprocess.run(['python3', str(root / 'run.py'), name, *args], check=True)
    if args[:2] == ('cargo', 'test'):
        subprocess.run(['python3', str(root / 'pin-binaries.py'), name], check=True)

run('format-final', 'cargo', 'fmt', '--all')
run('cache-tests-final', 'cargo', 'test', '--locked', '-p', 'deadpan-audio', 'persistent_', '--', '--nocapture')
run('storage-tests', 'cargo', 'test', '--locked', '-p', 'deadpan-cli', '--lib', 'storage::tests')
run('diagnostics-tests', 'cargo', 'test', '--locked', '-p', 'deadpan-app', '--lib', 'preview::diagnostics::tests')
run('transport-tests', 'cargo', 'test', '--locked', '-p', 'deadpan-app', '--lib', 'transport')
run('clippy', 'cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings')
run('release-build', 'cargo', 'build', '--release', '--locked', '-p', 'deadpan-cli', '--bin', 'deadpan-cli', '--example', 'perf')
