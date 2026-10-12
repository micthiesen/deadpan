import pathlib, subprocess, sys

directory = pathlib.Path(__file__).resolve().parent
commands = [
    ('fmt-check', ['cargo', 'fmt', '--all', '--check']),
    ('clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings']),
    ('release-partial', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-audio', '--test', 'audio_definition', 'sound_clocks::partial', '--', '--nocapture']),
    ('release-store', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-store', '--test', 'source_registration', 'partial_sound_register', '--', '--nocapture']),
    ('release', ['cargo', 'build', '--locked', '--release', '-p', 'deadpan-app', '-p', 'deadpan-cli', '--features', 'ui-harness']),
    ('release-export', ['env', 'DEADPAN_PREVIEW_EXPORT_KEEP=1', 'DEADPAN_PREVIEW_EXPORT_RESULTS=/tmp/deadpan-sound-capture-20261012/export-results.json', 'cargo', 'test', '--locked', '--release', '-p', 'deadpan-cli', '--test', 'preview_export', 'partial_attached_sound_copies_preserve_preview_and_verified_export', '--', '--nocapture']),
]
for name, command in commands:
    print('Starting ' + name, flush=True)
    result = subprocess.run(['python3', str(directory / 'run.py'), name, *command])
    if result.returncode:
        sys.exit(result.returncode)
print('All checks passed', flush=True)
