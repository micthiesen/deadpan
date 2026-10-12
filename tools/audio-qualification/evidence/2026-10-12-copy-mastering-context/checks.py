import pathlib, subprocess, sys
root = pathlib.Path(__file__).resolve().parent
common = ['env', 'DEADPAN_PREVIEW_EXPORT_KEEP=1']
commands = [
    ('fmt-check', ['cargo', 'fmt', '--all', '--check']),
    ('debug', common + ['DEADPAN_COPY_MASTERING_RESULTS='+str(root/'debug-result.json'), 'cargo', 'test', '--locked', '-p', 'deadpan-cli', '--test', 'preview_export', 'partial_copy_preserves_authored_mix_and_uses_current_mastering_context', '--', '--nocapture']),
    ('clippy', ['cargo', 'clippy', '--locked', '-p', 'deadpan-cli', '--all-targets', '--', '-D', 'warnings']),
    ('release', common + ['DEADPAN_COPY_MASTERING_RESULTS='+str(root/'release-result.json'), 'DEADPAN_PREVIEW_EXPORT_RESULTS='+str(root/'export-result.json'), 'cargo', 'test', '--locked', '--release', '-p', 'deadpan-cli', '--test', 'preview_export', 'partial_', '--', '--nocapture']),
]
for name, command in commands:
    print('Starting '+name, flush=True)
    result = subprocess.run(['python3', str(root/'run.py'), name, *command])
    if result.returncode:
        sys.exit(result.returncode)
