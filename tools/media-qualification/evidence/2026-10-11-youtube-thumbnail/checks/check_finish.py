from pathlib import Path
import os, subprocess, time, json, hashlib

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
steps = [
    ('finish-format', ['cargo', 'fmt', '--all']),
    ('finish-fmt-check', ['cargo', 'fmt', '--all', '--', '--check']),
    ('finish-app-clippy', ['cargo', 'clippy', '-p', 'deadpan-app', '--all-targets', '--locked', '--', '-D', 'warnings']),
    ('finish-harness-clippy', ['cargo', 'clippy', '-p', 'deadpan-app', '--all-targets', '--features', 'ui-harness', '--locked', '--', '-D', 'warnings']),
    ('finish-plan-warning', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-plan', '--test', 'audio_definition', '--test-threads', '1', '-E', 'test(intrinsic_policy_window_preserves_bound_support_outside_the_visible_run)']),
    ('finish-app-checks', ['cargo', 'nextest', 'run', '--locked', '-p', 'deadpan-app', '--features', 'ui-harness', '--test-threads', '1', '-E', 'test(youtube) | test(empty_selected_beat_yanks_and_pastes_at_its_sibling_slot_without_cursor_inference)']),
    ('finish-youtube-replay', ['cargo', 'xtask', 'replays', '--jobs', '1', '--scenario', 'youtube', '--output', str(root / 'finish-youtube-replay')]),
    ('finish-bundle', ['cargo', 'xtask', 'bundle', '--output', '/tmp/deadpan-youtube-thumbnail-final-bundle-20261011']),
    ('finish-bundle-verify', ['cargo', 'xtask', 'bundle-verify', '/tmp/deadpan-youtube-thumbnail-final-bundle-20261011/Deadpan.app']),
]
rows = []
for name, command in steps:
    print('START', name, flush=True)
    start = time.monotonic()
    log = root / (name + '.log')
    with log.open('x') as output:
        result = subprocess.run(command, cwd=repo, env=env, stdout=output, stderr=subprocess.STDOUT)
    rows.append(dict(name=name, command=command, exit=result.returncode, seconds=time.monotonic()-start, log_sha256=hashlib.sha256(log.read_bytes()).hexdigest()))
    (root / 'finish-checks.json').write_text(json.dumps(rows, indent=2) + '\n')
    print('FINISH', name, result.returncode, flush=True)
    if result.returncode:
        print(log.read_text(errors='replace')[-10000:], flush=True)
        raise SystemExit(result.returncode)
