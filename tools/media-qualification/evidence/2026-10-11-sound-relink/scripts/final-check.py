from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
already_passed = [
    'complete_candidate_audit_enumerates_new_unannotated_branches',
    'register_family_expands_custom_paths_and_keeps_mark_capture_separate',
    'configured_dot_path_retains_policy_and_teaches_its_supported_scope',
]
steps = [
    ('format', ['cargo', 'fmt', '--all', '--check']),
    ('ui-tests-final', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-app', '--features', 'ui-harness', '--no-fail-fast', '-E', ' & '.join(f'not test({name})' for name in already_passed)]),
    ('clippy-ui', ['cargo', 'clippy', '--locked', '-p', 'deadpan-app', '--all-targets', '--features', 'ui-harness', '--', '-D', 'warnings']),
    ('clippy-app', ['cargo', 'clippy', '--locked', '-p', 'deadpan-app', '--all-targets', '--', '-D', 'warnings']),
    ('replays-final', ['cargo', 'xtask', 'replays', '--scenario', 'sound-relink,relink', '--output', str(root/'replays-final'), '--jobs', '1']),
]
for name, command in steps:
    print('START', name, flush=True)
    result = subprocess.run(['python3', str(root/'run.py'), name, *command])
    if result.returncode:
        print((root/(name+'.log')).read_text(errors='replace')[-5000:], flush=True)
        sys.exit(result.returncode)
