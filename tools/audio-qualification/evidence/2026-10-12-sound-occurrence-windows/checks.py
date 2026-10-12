import pathlib, subprocess, sys
directory = pathlib.Path(__file__).resolve().parent
commands = [
    ('fmt-check', ['cargo', 'fmt', '--all', '--check']),
    ('plan-audio-playback', ['cargo', 'test', '--locked', '-p', 'deadpan-plan', '-p', 'deadpan-audio', '-p', 'deadpan-playback']),
    ('clippy', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings']),
    ('release-windows', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-plan', '--test', 'sound_route_sampling', 'occurrence_windows', '--', '--nocapture']),
    ('release-pcm', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-audio', '--test', 'audio_definition', 'cropped_occurrence_transport_selects_complete_nested_preserve_output', '--', '--nocapture']),
    ('release', ['cargo', 'build', '--locked', '--release', '-p', 'deadpan-app', '-p', 'deadpan-cli', '--features', 'ui-harness']),
]
for name, command in commands:
    print('Starting ' + name, flush=True)
    result = subprocess.run(['python3', str(directory / 'run.py'), name, *command])
    if result.returncode:
        sys.exit(result.returncode)
print('All checks passed', flush=True)
