from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
steps = [
    ('final-format', ['cargo', 'fmt', '--all']),
    ('clippy', ['cargo', 'clippy', '--locked', '-p', 'deadpan-cli', '--all-targets', '--', '-D', 'warnings']),
    ('cli-tests', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-cli', '--lib', '--no-fail-fast']),
    ('import-integration', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-media-worker', '--test', 'youtube_import', '--test', 'remux_real_media', '--no-fail-fast']),
    ('app-job-tests', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-app', '--lib', '-E', 'test(youtube::tests)']),
    ('bundle', ['cargo', 'xtask', 'bundle', '--output', '/tmp/deadpan-youtube-limits-bundle-20261011']),
    ('bundle-verify', ['cargo', 'xtask', 'bundle-verify', '/tmp/deadpan-youtube-limits-bundle-20261011/Deadpan.app']),
]
for name, command in steps:
    print('START', name, flush=True)
    result = subprocess.run(['python3', str(root / 'run.py'), name, *command])
    if result.returncode:
        print((root / (name + '.log')).read_text(errors='replace')[-6000:], flush=True)
        sys.exit(result.returncode)
