from pathlib import Path
import hashlib
import json
import os
import re
import shutil
import subprocess
import time

S = Path('/tmp/deadpan-single-leaf-net-prototype-2026-10-24')
SOURCE = S / 'source'
TARGET = S / 'target'
ENV = dict(os.environ, CARGO_TARGET_DIR=str(TARGET), CARGO_TERM_COLOR='never',
           DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
ENV.pop('DEADPAN_PAUSE_ROUTE_PROFILE', None)
STAGES = json.loads((S / 'expected-build-stages.json').read_text())
STATE = {'stage': 'preflight'}
OWNED = False
HELPER_MANIFEST = Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23/release-native-binaries.json')
HELPERS = ['deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track']


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(name, value):
    (S / name).write_text(json.dumps(value, indent=2) + '\n')


def save_exclusive(name, value):
    with (S / name).open('x') as output:
        output.write(json.dumps(value, indent=2) + '\n')


def source_map(root):
    return {str(p.relative_to(root)): sha(p) for p in sorted(root.rglob('*.rs'))}


def guard():
    frozen = json.loads((S / 'frozen-sources.json').read_text())
    actual = source_map(SOURCE)
    assert actual == frozen, 'private candidate Rust sources changed'
    base_provenance = json.loads((S / 'baseline-provenance.json').read_text())
    base_sources = json.loads((S / 'baseline-sources.json').read_text())
    matched_source = Path(base_provenance['matched_source'])
    assert sha(S / 'baseline-sources.json') == base_provenance['matched_source_map_sha256']
    assert source_map(matched_source) == base_sources, 'matched profile source changed'
    assert set(actual) == set(base_sources), 'Rust source path set changed'
    inputs = json.loads((S / 'baseline-inputs.json').read_text())
    assert all(sha(SOURCE / path) == digest for path, digest in inputs.items()), 'candidate input changed'
    assert all(sha(matched_source / path) == digest for path, digest in inputs.items()), 'matched input changed'
    review_path = S / 'source-review.json'
    assert review_path.is_file(), 'Root source review must be present before building'
    review = json.loads(review_path.read_text())
    assert review['status'] == 'no_findings' and not review['findings']
    assert review['source_map_sha256'] == sha(S / 'frozen-sources.json')
    pins = json.loads((S / 'driver-pins.json').read_text())
    required = {'build.py', 'measure.py', 'expected-build-stages.json',
                'expected-tests.json', 'check_profile.py'}
    assert required <= pins.keys(), 'build protocol/parser must be pinned before build'
    assert all(sha(S / path) == digest for path, digest in pins.items()), 'pinned harness input changed'
    assert json.loads((S / 'expected-build-stages.json').read_text()) == STAGES
    provenance = json.loads((S / 'baseline-provenance.json').read_text())
    baseline = json.loads((S / 'baseline.json').read_text())
    assert sha(provenance['matched_app']) == provenance['matched_app_sha256']
    assert baseline['matched_profile']['path'] == provenance['matched_app']
    assert baseline['matched_profile']['sha256'] == provenance['matched_app_sha256']
    assert sha(baseline['control']['path']) == baseline['control']['sha256']
    assert baseline['control']['sha256'] == 'e6abcd3a521697f504901f156df445e41681cfc824ff97793f62f4e0865802e1'
    baseline = json.loads((S / 'baseline.json').read_text())
    assert 'qualified_helper_manifest_sha256' in baseline
    assert sha(HELPER_MANIFEST) == baseline['qualified_helper_manifest_sha256']


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
        found[name] = {'built': str(path), 'cargo_profile': profile,
                       'profile': message['profile'], 'target': target, 'sha256': sha(path)}
    assert set(found) == set(names), (stage, sorted(found), names)
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


def test_results(stage, expected_suffixes=None, expected_summaries=1,
                 allow_duplicate_names=False):
    output = (S / (stage + '.stdout')).read_text()
    executed = re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', output, re.M)
    totals = re.findall(
        r'^test result: ok\. ([0-9]+) passed; 0 failed; 0 ignored;', output, re.M)
    assert len(totals) == expected_summaries and all(int(n) > 0 for n in totals), (
        stage, totals)
    assert sum(map(int, totals)) == len(executed), (stage, totals, executed)
    if not allow_duplicate_names:
        assert len(set(executed)) == len(executed), (stage, 'duplicate executed test names')
    if expected_suffixes:
        for suffix in expected_suffixes:
            assert any(name == suffix or name.endswith('::' + suffix) for name in executed), (
                stage, 'required test missing', suffix)
    return {'executed_tests': executed, 'passed_per_summary': list(map(int, totals)),
            'failed': 0, 'ignored': 0}


def cargo_executions(stage, records, expected_names):
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
    output_names = ['build-owner.json', 'build-receipt.json', 'build-terminal.json',
                    'test-binaries.json', 'binaries.json', 'cargo-rustc-info.json']
    output_names += [f'{stage}.{suffix}' for stage in STAGES for suffix in ('stdout', 'stderr')]
    assert all(not (S / name).exists() for name in output_names), 'refusing to overwrite a prior build output'
    assert not (S / 'test-bin').exists(), 'refusing to overwrite retained test binaries'
    guard()
    save_exclusive('build-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
                                       'script': str(Path(__file__)), 'fresh_target': True})
    OWNED = True
    expected = json.loads((S / 'expected-tests.json').read_text())
    rows, test_bins, unit = [], {}, None
    commands = [
        ('rustc', ['rustc', '--version', '--verbose']),
        ('cargo', ['cargo', '--version']),
        ('format', ['cargo', 'fmt', '--all', '--check']),
        ('core-unit-build', ['cargo', 'test', '--locked', '-p', 'deadpan-core',
                             '--lib', '--no-run', '--message-format=json-render-diagnostics']),
        ('core-unit-all', None),
        ('core-integration', ['cargo', 'test', '--locked', '-p', 'deadpan-core',
                              '--test', 'semantic_refresh', '--test', 'command_work',
                              '--test', 'audio_reference', '--test', 'semantic_leaf_replay',
                              '--message-format=json-render-diagnostics']),
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
        if name == 'core-unit-all':
            assert unit is not None and sha(unit['retained']) == unit['sha256']
            command = [unit['retained'], '--test-threads=8']
        started = time.monotonic()
        with (S / (name + '.stdout')).open('x') as out, (S / (name + '.stderr')).open('x') as err:
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
        if name == 'core-unit-all':
            assert command[1:] == ['--test-threads=8'], 'run complete unfiltered Core unit binary'
            row['tests'] = test_results(
                name, expected['core-unit-all']['required_test_suffixes'])
            unit['executed'] = unit['retained']
            unit['tests'] = row['tests']
            test_bins[name] = unit
            row['test_binary'] = unit
        if name in ('core-integration', 'store-preparation'):
            target_names = (expected[name]['targets'] if name == 'core-integration'
                            else ['compound_preparation'])
            profile = 'debug' if name == 'core-integration' else 'release'
            records = artifacts(name, target_names, profile)
            cargo_executions(name, records, target_names)
            row['tests'] = test_results(
                name,
                expected[name].get('required_test_suffixes') if name == 'core-integration'
                else expected[name],
                expected_summaries=expected[name]['summaries'] if name == 'core-integration' else 1,
                allow_duplicate_names=name == 'core-integration')
            if name == 'store-preparation':
                assert len(row['tests']['executed_tests']) == 12
                assert row['tests']['passed_per_summary'] == [12]
            if name == 'core-integration':
                assert len(row['tests']['passed_per_summary']) == 4
                assert all(n >= expected[name]['minimum_passed_per_binary']
                           for n in row['tests']['passed_per_summary'])
            test_bins[name] = {target: retain(record, name, target)
                               for target, record in records.items()}
            row['test_binaries'] = test_bins[name]
        row['validation'] = 'passed'
        save('test-binaries.json', test_bins)
        save('build-receipt.json', rows)
    assert [row['name'] for row in rows] == STAGES
    assert all(row['exit'] == 0 and row['validation'] == 'passed' for row in rows)
    helpers = json.loads(HELPER_MANIFEST.read_text())
    binaries = {}
    for name in ['deadpan-app'] + HELPERS:
        if name == 'deadpan-app':
            path = (TARGET / 'release' / name).resolve(strict=True)
        else:
            path = Path(helpers[name]['executed']).resolve(strict=True)
            assert sha(path) == helpers[name]['sha256']
        dest = S / 'bin' / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        assert not dest.exists(), ('refuse to overwrite app/helper', dest)
        shutil.copy2(path, dest)
        assert sha(path) == sha(dest)
        binaries[name] = {'source': str(path), 'executed': str(dest), 'sha256': sha(dest),
                          'freshly_built': name == 'deadpan-app',
                          'qualified_helper_reuse': name != 'deadpan-app'}
    save('binaries.json', binaries)
    shutil.copy2(TARGET / '.rustc_info.json', S / 'cargo-rustc-info.json')
    guard()
    save_exclusive('build-terminal.json', {'status': 'complete', 'steps': len(rows),
                                           'stage_names': STAGES,
                                           'app_sha256': binaries['deadpan-app']['sha256']})
    print(json.dumps({'terminal': True, 'steps': len(rows),
                      'app': binaries['deadpan-app']['sha256']}), flush=True)


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
