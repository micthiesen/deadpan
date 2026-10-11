from pathlib import Path
import subprocess

root = Path(__file__).parent

def run(name, command, expected=0, pin=False):
    result = subprocess.run(['python3', str(root / 'run.py'), name, *command])
    if result.returncode != expected:
        raise SystemExit(result.returncode or 1)
    if pin:
        subprocess.run(['python3', str(root / 'pin-binaries.py'), name], check=True)

run('disk-quota-final', ['cargo', 'test', '--locked', '-p', 'deadpan-audio', '--test',
    'stages', 'disk_cache::disk_stage_matches_memory_and_releases_evicted_storage',
    '--', '--exact'], pin=True)
run('baseline', ['python3', str(root / 'baseline.py')], expected=101, pin=True)
run('clippy-final', ['cargo', 'clippy', '--locked', '--workspace', '--all-targets',
    '--features', 'ui-harness', '--', '-D', 'warnings'])
run('format-check', ['cargo', 'fmt', '--all', '--', '--check'])
run('export-final', ['env', 'DEADPAN_PREVIEW_EXPORT_KEEP=1',
    'DEADPAN_PREVIEW_EXPORT_RESULTS=' + str(root / 'export.json'),
    'cargo', 'test', '--release', '--locked', '-p', 'deadpan-cli', '--test',
    'preview_export', 'disk_prepared_preserve_export_matches_its_committed_preview',
    '--', '--exact', '--nocapture'], pin=True)
