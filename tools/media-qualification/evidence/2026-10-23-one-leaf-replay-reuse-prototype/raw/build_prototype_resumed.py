from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import time

ROOT = Path('/tmp/deadpan-one-leaf-replay-reuse-prototype-2026-10-23')
SOURCE = ROOT / 'source'
TARGET = ROOT / 'target'
QUALIFIED = Path('/tmp/deadpan-semantic-refresh-duration-reuse-qualification-2026-10-23')
ENV = dict(os.environ,
    CARGO_TARGET_DIR=str(TARGET),
    DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')

def sha(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()

def load(path):
    return json.loads(Path(path).read_text())

def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')

def main():
    frozen = load(ROOT / 'formatted-source-identities.json')
    inputs = load(ROOT / 'input-identities.json')
    assert TARGET.is_dir()  # Same private target after assertion-only test correction.
    rows = []
    test_binaries = {}

    def guard():
        paths = subprocess.check_output(['rg', '--files', '-g', '*.rs'], cwd=SOURCE, text=True).splitlines()
        assert {p: sha(SOURCE / p) for p in paths} == frozen
        assert {p: sha(SOURCE / p) for p in inputs} == inputs
        review = load(ROOT / 'independent-review.json')
        assert review['status'] == 'no_findings' and review.get('findings') == []
        for artifacts in test_binaries.values():
            for row in artifacts:
                assert sha(row['executed']) == row['sha256'] == sha(row['retained'])

    def run(name, command, is_test=False):
        guard()
        started = time.monotonic()
        stdout = ROOT / (name + '.stdout')
        stderr = ROOT / (name + '.stderr')
        with stdout.open('w') as out, stderr.open('w') as err:
            result = subprocess.run(command, cwd=SOURCE, env=ENV, stdout=out, stderr=err)
        row = {'name': name, 'command': command, 'cwd': str(SOURCE),
               'exit': result.returncode, 'seconds': time.monotonic() - started}
        rows.append(row)
        save(ROOT / 'build-receipt.json', rows)
        print(json.dumps(row), flush=True)
        guard()
        if result.returncode:
            raise SystemExit(result.returncode)
        if is_test:
            artifacts = {}
            for line in stdout.read_text().splitlines():
                if not line.startswith('{'):
                    continue
                try:
                    message = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if (message.get('reason') == 'compiler-artifact'
                        and message.get('profile', {}).get('test') is True
                        and message.get('executable')):
                    executable = Path(message['executable'])
                    artifacts[str(executable)] = executable
            assert len(artifacts) == 1, (name, sorted(artifacts))
            retained_rows = []
            for executable in artifacts.values():
                retained = ROOT / 'test-bin' / name / executable.name
                retained.parent.mkdir(parents=True, exist_ok=True)
                digest = sha(executable)
                shutil.copy2(executable, retained)
                assert sha(retained) == digest
                retained_rows.append({'executed': str(executable), 'retained': str(retained),
                                      'sha256': digest, 'bytes': retained.stat().st_size})
            test_binaries[name] = retained_rows
            save(ROOT / 'test-binaries.json', test_binaries)
            guard()

    guard()
    save(ROOT / 'build-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
                                    'script': str(Path(__file__)), 'fresh_target': True})
    run('rustc-toolchain', ['rustc', '--version', '--verbose'])
    assert (ROOT / 'rustc-toolchain.stdout').read_text().startswith('rustc 1.97.1 ')
    run('cargo-toolchain', ['cargo', '--version'])
    assert (ROOT / 'cargo-toolchain.stdout').read_text().startswith('cargo 1.97.1 ')
    run('format-check', ['cargo', 'fmt', '--all', '--check'])
    prefix = ['cargo', 'test', '--locked', '--release', '--message-format=json-render-diagnostics']
    run('core-refutations', prefix + ['-p', 'deadpan-core', '--test', 'semantic_leaf_replay'], True)
    run('core-boundary-unit', prefix + ['-p', 'deadpan-core', '--lib', 'one_leaf_replay'], True)
    run('store-atomic', prefix + ['-p', 'deadpan-store', '--test', 'compound_preparation'], True)
    run('core-reference', prefix + ['-p', 'deadpan-core', '--test', 'command_work'], True)
    run('core-refresh', prefix + ['-p', 'deadpan-core', '--test', 'semantic_refresh'], True)
    run('core-trim', prefix + ['-p', 'deadpan-core', '--test', 'semantic_trim'], True)
    run('release-app', ['cargo', 'build', '--locked', '--release', '-p', 'deadpan-app',
                        '--features', 'ui-harness', '--bin', 'deadpan-app'])
    qualified = load(QUALIFIED / 'release-native-binaries.json')
    binaries = {}
    for name in ['deadpan-app', 'deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track']:
        source = TARGET / 'release' / name if name == 'deadpan-app' else Path(qualified[name]['executed'])
        if name != 'deadpan-app':
            assert sha(source) == qualified[name]['sha256']
        executed = ROOT / 'bin' / name
        executed.parent.mkdir(exist_ok=True)
        shutil.copy2(source, executed)
        binaries[name] = {'source': str(source), 'executed': str(executed), 'sha256': sha(executed),
                          'freshly_built': name == 'deadpan-app', 'qualified_helper_reuse': name != 'deadpan-app'}
    save(ROOT / 'binaries.json', binaries)
    guard()
    cache = TARGET / '.rustc_info.json'
    assert cache.is_file()
    shutil.copy2(cache, ROOT / 'cargo-rustc-info.json')
    print(json.dumps({'done': True, 'app_sha256': binaries['deadpan-app']['sha256'],
                      'test_phases': len(test_binaries), 'steps': len(rows)}), flush=True)

if __name__ == '__main__':
    main()
