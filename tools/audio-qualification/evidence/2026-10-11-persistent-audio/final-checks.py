from pathlib import Path
import subprocess
root = Path(__file__).parent
def run(name, *args):
    subprocess.run(['python3', str(root / 'run.py'), name, *args], check=True)
    if args[:2] == ('cargo', 'test'):
        subprocess.run(['python3', str(root / 'pin-binaries.py'), name], check=True)
run('format-reviewed', 'cargo', 'fmt', '--all')
run('cache-tests-reviewed', 'cargo', 'test', '--locked', '-p', 'deadpan-audio', 'persistent_', '--', '--nocapture')
run('diagnostics-bin', 'cargo', 'test', '--locked', '-p', 'deadpan-app', '--bin', 'deadpan-app', 'preview::diagnostics::tests')
run('transport-bin', 'cargo', 'test', '--locked', '-p', 'deadpan-app', '--bin', 'deadpan-app', 'transport')
run('clippy-reviewed', 'cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings')
run('replays', 'cargo', 'xtask', 'replays', '--scenario', 'diagnostics,storage,original-playback,sound-playback', '--output', str(root / 'replays'), '--jobs', '1')
run('release-reviewed', 'cargo', 'build', '--release', '--locked', '-p', 'deadpan-cli', '--bin', 'deadpan-cli', '--example', 'perf')
