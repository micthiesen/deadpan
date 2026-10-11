from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
steps = [
    ('format-final', ['cargo', 'fmt', '--all']),
    ('final-focused-3', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-cli', '--lib', '--test', 'privacy_network_audit', '-E', 'test(youtube::) | binary(privacy_network_audit)']),
    ('gate', ['cargo', 'xtask', 'gate']),
    ('real-app-job', ['env', 'DEADPAN_REAL_YOUTUBE_URL=https://youtu.be/Z4C82eyhwgU', 'DEADPAN_REAL_YOUTUBE_REQUIRE_THUMBNAIL=1', 'cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-app', '--bin', 'deadpan-app', '-E', 'test(real_url_import_through_the_app_job_when_requested)', '--success-output', 'immediate']),
    ('chaos', ['cargo', 'xtask', 'chaos', '--minutes', '0.25', '--seed', '20261011', '--only', 'cli-boundaries', '--output', str(root / 'chaos')]),
    ('bundle', ['cargo', 'xtask', 'bundle', '--output', '/tmp/deadpan-youtube-network-bundle-20261011']),
    ('bundle-verify', ['cargo', 'xtask', 'bundle-verify', '/tmp/deadpan-youtube-network-bundle-20261011/Deadpan.app']),
]
for name, command in steps:
    print('START', name, flush=True)
    result = subprocess.run(['python3', str(root / 'run.py'), name, *command])
    if result.returncode:
        print((root / (name + '.log')).read_text(errors='replace')[-6000:], flush=True)
        sys.exit(result.returncode)
