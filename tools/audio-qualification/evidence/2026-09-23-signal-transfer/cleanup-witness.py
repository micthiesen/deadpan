from pathlib import Path
import gzip, json, os, subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-transfer-20260923')
copy = scratch / 'mutant'
relative = 'crates/deadpan-media/tests/host_boundary.rs'
baseline = (repo / relative).read_text()
changes = [
    ("(exec 3>&2 2>/dev/null; sleep 1; printf alive > '{}'", r'''(exec 3>&2 2>/dev/null; sleep 1; status=$?; printf '%s' \"$status\" > '{}' '''.rstrip()),
    ('assert!(start.elapsed() < Duration::from_secs(2));', 'let cleanup = start.elapsed();\n    eprintln!("cleanup={cleanup:?}; immediate_marker={:?}", fs::read_to_string(&marker));\n    assert!(cleanup < Duration::from_secs(2));'),
    ('assert!(!marker.exists());', 'assert!(!marker.exists(), "sleep exit status: {:?}; cleanup={cleanup:?}", fs::read_to_string(&marker));'),
]
start = baseline.index('fn successful_leader_exit_cleans_up_descendants_with_inherited_pipes()')
end = baseline.index('\n#[test]', start)
modified = baseline[start:end]
for old, new in changes:
    assert modified.count(old) == 1, (old, modified.count(old))
    modified = modified.replace(old, new)
(copy / relative).write_text(baseline[:start] + modified + baseline[end:])
env = os.environ.copy()
env['DEADPAN_FFMPEG_PREFIX'] = '/tmp/deadpan-media-compatible-xyhilms4/prefix'
env['CARGO_TARGET_DIR'] = str(scratch / 'mutation-target')
command = ['cargo', 'test', '-p', 'deadpan-media', '--locked', '--test', 'host_boundary', 'successful_leader_exit_cleans_up_descendants_with_inherited_pipes', '--', '--exact', '--nocapture']
reports = []
try:
    for attempt in range(1, 21):
        result = subprocess.run(command, cwd=copy, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        with gzip.open(scratch / f'cleanup-witness-{attempt}.log.gz', 'wt') as log:
            log.write(result.stdout)
        row = {'attempt': attempt, 'exit_code': result.returncode, 'tail': result.stdout[-1800:]}
        reports.append(row)
        print(json.dumps(row), flush=True)
        if result.returncode:
            assert 'sleep exit status:' in result.stdout, 'Unexpected failure'
            break
finally:
    (copy / relative).write_text(baseline)
    (scratch / 'cleanup-witness-report.json').write_text(json.dumps({'changes': changes, 'command': command, 'attempts': reports}, indent=2) + '\n')
