from pathlib import Path
import os
import subprocess

s = Path(__file__).resolve().parent
os.environ['CARGO_TARGET_DIR'] = '/tmp/deadpan-follow-creep-2026-10-24/target'
os.environ['CARGO_PROFILE_DEV_OPT_LEVEL'] = '2'
os.environ['DEADPAN_REQUIRE_PHYSICAL_MAC'] = '1'
os.environ['DEADPAN_FFMPEG_PREFIX'] = '/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
selection = ('package(deadpan-jobs) | package(deadpan-track) | package(deadpan-app) '
             '| test(picture_proposals) | test(faces) '
             '| binary(hostile_analysis_workers) | binary(proxy_resume)')
jobs = [
    ('official-check-final', ['cargo', 'xtask', 'check', '--since',
        '889a06f0738a7347faf1bda53463cd7e35ae07b1', '--filter', selection]),
    ('app-units-optimized', ['cargo', 'test', '--locked', '-p', 'deadpan-app',
        '--features', 'ui-harness', '--bin', 'deadpan-app']),
    ('changed-doctests', ['cargo', 'test', '--locked', '--no-fail-fast', '-p',
        'deadpan-jobs', '-p', 'deadpan-cli', '-p', 'deadpan-app', '--doc']),
    ('release-first', ['cargo', 'build', '--locked', '--release', '-p', 'deadpan-app',
        '-p', 'deadpan-cli', '-p', 'deadpan-media-worker', '-p', 'deadpan-transcribe',
        '-p', 'deadpan-track', '--features', 'deadpan-app/ui-harness']),
]
for label, command in jobs:
    code = subprocess.run(['python', str(s / 'run.py'), label, '--', *command], check=False).returncode
    if code:
        raise SystemExit(f'{label} exited {code}; inspect retained results')
