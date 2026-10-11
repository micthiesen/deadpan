from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
steps = [
    ('app-job-tests-corrected', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-app', '--bin', 'deadpan-app', '-E', 'test(youtube::tests)']),
    ('lingering-followup', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-cli', '--lib', '--test-threads', '1', '-E', 'test(cli_policy_rejects_incomplete_and_out_of_range_values_without_replacing) | test(settings_survive_restart_and_are_private)']),
    ('bundle', ['cargo', 'xtask', 'bundle', '--output', '/tmp/deadpan-youtube-limits-bundle-20261011']),
    ('bundle-verify', ['cargo', 'xtask', 'bundle-verify', '/tmp/deadpan-youtube-limits-bundle-20261011/Deadpan.app']),
]
for name, command in steps:
    print('START', name, flush=True)
    result = subprocess.run(['python3', str(root / 'run.py'), name, *command])
    if result.returncode:
        print((root / (name + '.log')).read_text(errors='replace')[-6000:], flush=True)
        sys.exit(result.returncode)
subprocess.run(['python3', str(root / 'packaged.py')], check=True)
