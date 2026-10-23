from pathlib import Path
import hashlib
import json
import os
import re
import shutil
import subprocess
import time

S = Path('/tmp/deadpan-insert-split-proof-prototype-v2-2026-10-23')
SOURCE = S / 'source'
TARGET = S / 'target'
ENV = dict(os.environ, CARGO_TARGET_DIR=str(TARGET), CARGO_TERM_COLOR='never',
           DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
ENV.pop('DEADPAN_PAUSE_ROUTE_PROFILE', None)
STAGES = ['rustc', 'cargo', 'format', 'core-unit-build', 'core-split-proof',
          'core-wide-reference', 'core-integration', 'store-preparation',
          'release-app', 'strict']
STATE = {'stage': 'preflight'}
OWNED = False


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(name, value):
    (S / name).write_text(json.dumps(value, indent=2) + '\n')


def save_exclusive(name, value):
    with (S / name).open('x') as output:
        output.write(json.dumps(value, indent=2) + '\n')


def guard():
    expected = json.loads((S / 'frozen-sources.json').read_text())
    actual = {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob('*.rs')}
    assert actual == expected, 'private Rust sources changed'
    inputs = json.loads((S / 'baseline-inputs.json').read_text())
    assert all(sha(SOURCE / p) == h for p, h in inputs.items()), 'pinned input changed'
    review = json.loads((S / 'source-review.json').read_text())
    assert review['status'] == 'no_findings' and not review['findings']
    assert review['source_map_sha256'] == sha(S / 'frozen-sources.json')
    pins = json.loads((S / 'driver-pins.json').read_text())
    required = {'build.py', 'expected-build-stages.json', 'expected-tests.json'}
    assert required <= pins.keys(), 'build protocol/manifests must be pinned'
    assert all(sha(S / p) == h for p, h in pins.items()), 'reviewed harness input changed'
    assert json.loads((S / 'expected-build-stages.json').read_text()) == STAGES
    provenance = json.loads((S / 'baseline-provenance.json').read_text())
    assert sha(provenance['matched_diagnostic_app']) == provenance['matched_diagnostic_app_sha256']


def artifacts(stage, names, profile):
    found = {}
    for line in (S / (stage + '.stdout')).read_text().splitlines():
        if not line.startswith('{'):
            continue
        message = json.loads(line)
        target = message.get('target', {})
        name = target.get('name')
        if (message.get('reason') != 'compiler-artifact' or name not in names
                or not message.get('profile', {}).get('test')
                or not message.get('executable')):
            continue
        assert name not in found, (stage, 'duplicate test executable', name)
        path = Path(message['executable']).resolve(strict=True)
        assert path.is_relative_to((TARGET / profile / 'deps').resolve()), (stage, path)
        assert path.is_file(), (stage, path)
        found[name] = {'built': str(path), 'cargo_profile': profile, 'profile': message['profile'],
                       'target': target, 'sha256': sha(path)}
    assert set(found) == set(names), (stage, found.keys(), names)
    return found


def retain(record, stage, name):
    path = Path(record['built'])
    assert sha(path) == record['sha256']
    dest = S / 'test-bin' / stage / path.name
    dest.parent.mkdir(parents=True, exist_ok=True)
    assert not dest.exists(), ('refuse to overwrite test binary', dest)
    shutil.copy2(path, dest)
    assert sha(dest) == record['sha256']
    return {**record, 'name': name, 'retained': str(dest)}


def passed_tests(stage, expected=None, summaries=1):
    output = (S / (stage + '.stdout')).read_text()
    executed = re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', output, re.M)
    totals = re.findall(
        r'^test result: ok\. ([0-9]+) passed; 0 failed; 0 ignored;', output, re.M)
    assert len(totals) == summaries and all(int(n) > 0 for n in totals), (stage, totals)
    assert sum(map(int, totals)) == len(executed), (stage, totals, executed)
    assert len(set(executed)) == len(executed), (stage, 'duplicate executed test names')
    if expected is not None:
        assert expected and sorted(executed) == sorted(expected), (stage, executed, expected)
        assert totals == [str(len(expected))], (stage, totals)
    return {'executed_tests': executed, 'passed_per_summary': list(map(int, totals)),
            'failed': 0, 'ignored': 0}


def cargo_executions(stage, records, expected_names):
    # Cargo's actual Running line must agree with its compiler-artifact path.
    output = (S / (stage + '.stderr')).read_text()
    executions = re.findall(r'^\s*Running tests/([A-Za-z0-9_]+)\.rs \((.+)\)$', output, re.M)
    assert sorted(name for name, _ in executions) == sorted(expected_names), (stage, executions)
    for name, raw in executions:
        path = Path(raw)
        if not path.is_absolute():
            path = SOURCE / path
        assert path.resolve(strict=True) == Path(records[name]['built'])
        assert sha(path) == records[name]['sha256']
        records[name]['executed'] = str(path.resolve())


def main():
    global OWNED
    assert not TARGET.exists(), 'fresh private target required'
    assert not (S / 'build-receipt.json').exists(), 'do not overwrite an earlier attempt'
    assert not (S / 'build-terminal.json').exists(), 'do not reuse a terminal build'
    assert not (S / 'test-binaries.json').exists(), 'do not reuse earlier test receipts'
    save_exclusive('build-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
                                       'script': str(Path(__file__)), 'fresh_target': True})
    OWNED = True
    guard()
    expected = json.loads((S / 'expected-tests.json').read_text())
    rows = []
    test_bins = {}
    unit = None
    commands = [
        ('rustc', ['rustc', '--version', '--verbose']),
        ('cargo', ['cargo', '--version']),
        ('format', ['cargo', 'fmt', '--all', '--check']),
        ('core-unit-build', ['cargo', 'test', '--locked', '-p', 'deadpan-core',
                             '--lib', '--no-run', '--message-format=json-render-diagnostics']),
        ('core-split-proof', None),
        ('core-wide-reference', None),
        ('core-integration', ['cargo', 'test', '--locked', '-p', 'deadpan-core',
                              '--test', 'semantic_refresh', '--test', 'command_work',
                              '--test', 'audio_reference', '--message-format=json-render-diagnostics']),
        ('store-preparation', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-store',
                               '--test', 'compound_preparation', '--message-format=json-render-diagnostics']),
        ('release-app', ['cargo', 'build', '--locked', '--release', '-p', 'deadpan-app',
                         '--features', 'ui-harness', '--bin', 'deadpan-app']),
        ('strict', ['cargo', 'clippy', '--locked', '--release', '-p', 'deadpan-core',
                    '-p', 'deadpan-store', '-p', 'deadpan-app', '--all-targets',
                    '--features', 'ui-harness', '--', '-D', 'warnings']),
    ]
    assert [name for name, _ in commands] == STAGES
    for name, command in commands:
        STATE['stage'] = name
        guard()
        if name in ('core-split-proof', 'core-wide-reference'):
            assert unit is not None and sha(unit['retained']) == unit['sha256']
            command = [unit['retained']]
            command += (['insert_split_proof_'] if name == 'core-split-proof'
                        else ['--exact', expected[name][0]])
        assert command
        started = time.monotonic()
        with (S / (name + '.stdout')).open('w') as out, (S / (name + '.stderr')).open('w') as err:
            result = subprocess.run(command, cwd=SOURCE, env=ENV, stdout=out, stderr=err)
        row = {'name': name, 'command': command, 'cwd': str(SOURCE),
               'exit': result.returncode, 'seconds': time.monotonic() - started,
               'validation': 'pending'}
        rows.append(row)
        save('build-receipt.json', rows)
        print(json.dumps(row), flush=True)
        guard()
        if result.returncode:
            raise SystemExit(result.returncode)
        if name in ('rustc', 'cargo'):
            assert (S / (name + '.stdout')).read_text().startswith(name + ' 1.97.1 ')
        if name == 'core-unit-build':
            records = artifacts(name, ['deadpan_core'], 'debug')
            unit = retain(records['deadpan_core'], name, 'deadpan_core')
            test_bins[name] = unit
            row['test_binary'] = unit
        if name in ('core-split-proof', 'core-wide-reference'):
            row['tests'] = passed_tests(name, expected[name])
            assert sha(unit['retained']) == unit['sha256']
            test_bins[name] = {**unit, 'executed': unit['retained'],
                               'tests': row['tests']}
            row['test_binary'] = test_bins[name]
        if name in ('core-integration', 'store-preparation'):
            target_names = (expected[name]['targets'] if name == 'core-integration'
                            else ['compound_preparation'])
            profile = 'debug' if name == 'core-integration' else 'release'
            records = artifacts(name, target_names, profile)
            cargo_executions(name, records, target_names)
            row['tests'] = passed_tests(
                name, None if name == 'core-integration' else expected[name],
                summaries=3 if name == 'core-integration' else 1)
            test_bins[name] = {target: retain(record, name, target)
                               for target, record in records.items()}
            row['test_binaries'] = test_bins[name]
        row['validation'] = 'passed'
        save('test-binaries.json', test_bins)
        save('build-receipt.json', rows)
    assert [row['name'] for row in rows] == STAGES
    assert all(row['exit'] == 0 and row['validation'] == 'passed' for row in rows)
    control = json.loads((S / 'baseline.json').read_text())['control']
    assert sha(control['path']) == control['sha256']
    qualified = json.loads(Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23/release-native-binaries.json').read_text())
    binaries = {}
    for name in ['deadpan-app', 'deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track']:
        path = TARGET / 'release' / name if name == 'deadpan-app' else Path(qualified[name]['executed'])
        if name != 'deadpan-app':
            assert sha(path) == qualified[name]['sha256']
        dest = S / 'bin' / name
        dest.parent.mkdir(exist_ok=True)
        assert not dest.exists(), ('refuse to overwrite app/helper', dest)
        shutil.copy2(path, dest)
        assert sha(path) == sha(dest)
        binaries[name] = {'source': str(path), 'executed': str(dest), 'sha256': sha(dest),
                          'freshly_built': name == 'deadpan-app', 'qualified_helper_reuse': name != 'deadpan-app'}
    save('binaries.json', binaries)
    shutil.copy2(TARGET / '.rustc_info.json', S / 'cargo-rustc-info.json')
    guard()
    save_exclusive('build-terminal.json', {'status': 'complete', 'steps': len(rows),
                                'stage_names': STAGES, 'app_sha256': binaries['deadpan-app']['sha256']})
    print(json.dumps({'terminal': True, 'steps': len(rows), 'app': binaries['deadpan-app']['sha256']}), flush=True)


if __name__ == '__main__':
    try:
        main()
    except BaseException as error:
        failure = {'status': 'failed', **STATE,
                   'exception': type(error).__name__, 'message': str(error)}
        if OWNED:
            try:
                save_exclusive('build-terminal.json', failure)
            except FileExistsError:
                pass
        else:
            save_exclusive(f'refused-build-{time.time_ns()}.json', failure)
        raise
