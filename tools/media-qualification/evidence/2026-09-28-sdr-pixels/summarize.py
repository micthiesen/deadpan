"""Summarize recorded exits and assert source coverage without rerunning tests."""
import json
from pathlib import Path
import re

root = Path(__file__).resolve().parent
commands = {path.stem: json.loads(path.read_text()) for path in sorted(root.glob('sdr[0-9][0-9]-*.json'))}
assert len(commands) == 11 and all('exit_code' in value for value in commands.values())
assert {name for name, value in commands.items() if value['exit_code']} == {'sdr01-clippy', 'sdr07-picture'}
log = (root/'sdr03-workspace.log').read_text()
results = [tuple(map(int, row)) for row in re.findall(
    r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;', log)]
assert results and 'test result: FAILED.' not in log
totals = dict(zip(('passed', 'failed', 'ignored', 'measured', 'filtered'), map(sum, zip(*results))))
assert totals['failed'] == 0
source_keys = ('sdr02-clippy', 'sdr03-workspace', 'sdr04-artifacts', 'sdr05-metal', 'sdr06-python', 'sdr11-format')
manifests = {name: json.loads((root/('source-'+commands[name]['source_manifest_sha256']+'.json')).read_text())
             for name in source_keys}
def rust_scope(manifest):
    return {key: value for key, value in manifest.items()
            if key.startswith(('crates/', 'native/', '.cargo/')) or key in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml')}
unchanged = all(rust_scope(value) == rust_scope(manifests['sdr02-clippy']) for value in manifests.values())
assert unchanged
native = {}
for name in ('picture-fixed', 'picture-sanitizers', 'encoder-control'):
    report = json.loads((root/(name+'.json')).read_text())
    assert report['result'].startswith('passed') and report['source_unchanged_during_run'] and not report['process_faults']
    assert all(row['passed'] for row in report['assertions'])
    assert all(row['passed'] for row in report['final_file_admission'])
    native[name] = {'result': report['result'], 'admission_assertions': len(report['assertions']),
                    'checks': sum(len(case['checks']) for case in report['cases']),
                    'final_file_admission': {'checks': len(report['final_file_admission']), 'all_passed': True},
                    'cases': [{key: case[key] for key in ('name', 'status', 'decoded_vs_actual') if key in case}
                              for case in report['cases']]}
metal = json.loads((root/'metal.json').read_text())
assert metal['status'] == 'passed' and all(row['passed'] for row in metal['checks'])
summary = {'workspace': {'targets_with_test_result': len(results), **totals,
                         'seconds': commands['sdr03-workspace']['seconds']},
           'rust_and_cargo_sources_unchanged_between_checks': unchanged,
           'source_comparisons': source_keys,
           'python': {'tests': 101, 'seconds': commands['sdr06-python']['seconds']},
           'metal': {'checks': len(metal['checks']), 'adapter': metal['adapter'],
                     'codes_compared': sum(case['comparison']['compared_codes'] for case in metal['cases']),
                     'maximum_code_error': max(value for case in metal['cases'] for value in case['comparison']['maximum_plane_difference'])},
           'native': native,
           'commands': {key: {'exit_code': value['exit_code'], 'seconds': value['seconds']} for key, value in commands.items()}}
(root/'verification.json').write_text(json.dumps(summary, indent=2)+'\n')
print(json.dumps(summary, indent=2))
