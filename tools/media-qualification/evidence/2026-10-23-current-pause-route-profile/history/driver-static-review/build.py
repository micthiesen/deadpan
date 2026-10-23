from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import time

S = Path('/tmp/deadpan-current-pause-route-profile-2026-10-23')
SOURCE = S / 'source'
TARGET = S / 'target'
ENV = dict(os.environ, CARGO_TARGET_DIR=str(TARGET),
           DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
ENV.pop('DEADPAN_PAUSE_ROUTE_PROFILE', None)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(name, value):
    (S / name).write_text(json.dumps(value, indent=2) + '\n')


def guard():
    expected = json.loads((S / 'frozen-sources.json').read_text())
    actual = {str(p.relative_to(SOURCE)): sha(p) for p in SOURCE.rglob('*.rs')}
    assert actual == expected, 'private Rust sources changed'
    inputs = json.loads((S / 'baseline-inputs.json').read_text())
    assert all(sha(SOURCE / p) == h for p, h in inputs.items()), 'pinned input changed'
    review = json.loads((S / 'source-review.json').read_text())
    assert review['status'] == 'no_findings' and not review['findings']
    assert review['source_map_sha256'] == sha(S / 'frozen-sources.json')


def main():
    assert not TARGET.exists(), 'fresh private target required'
    assert not (S / 'build-receipt.json').exists(), 'do not overwrite an earlier attempt'
    guard()
    save('build-owner.json', {'pid': os.getpid(), 'started_unix': time.time(),
                             'script': str(Path(__file__)), 'fresh_target': True})
    rows = []
    commands = [
        ('rustc', ['rustc', '--version', '--verbose']),
        ('cargo', ['cargo', '--version']),
        ('format', ['cargo', 'fmt', '--all', '--check']),
        ('helper-unit', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-core',
                         '--lib', 'pause_route_profile', '--message-format=json-render-diagnostics']),
        ('store-preparation', ['cargo', 'test', '--locked', '--release', '-p', 'deadpan-store',
                               '--test', 'compound_preparation', '--message-format=json-render-diagnostics']),
        ('release-app', ['cargo', 'build', '--locked', '--release', '-p', 'deadpan-app',
                         '--features', 'ui-harness', '--bin', 'deadpan-app']),
        ('strict', ['cargo', 'clippy', '--locked', '--release', '-p', 'deadpan-app',
                    '--features', 'ui-harness', '--bin', 'deadpan-app', '--', '-D', 'warnings']),
    ]
    test_bins = {}
    for name, command in commands:
        guard()
        started = time.monotonic()
        with (S / (name + '.stdout')).open('w') as out, (S / (name + '.stderr')).open('w') as err:
            result = subprocess.run(command, cwd=SOURCE, env=ENV, stdout=out, stderr=err)
        row = {'name': name, 'command': command, 'cwd': str(SOURCE),
               'exit': result.returncode, 'seconds': time.monotonic() - started}
        rows.append(row)
        save('build-receipt.json', rows)
        print(json.dumps(row), flush=True)
        guard()
        if result.returncode:
            raise SystemExit(result.returncode)
        if name in ('rustc', 'cargo'):
            assert (S / (name + '.stdout')).read_text().startswith(name + ' 1.97.1 ')
        if name in ('helper-unit', 'store-preparation'):
            found = []
            for line in (S / (name + '.stdout')).read_text().splitlines():
                if not line.startswith('{'):
                    continue
                message = json.loads(line)
                if message.get('reason') == 'compiler-artifact' and message.get('profile', {}).get('test') and message.get('executable'):
                    found.append(Path(message['executable']))
            assert len(found) == 1, (name, found)
            p = found[0]
            dest = S / 'test-bin' / name / p.name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(p, dest)
            assert sha(dest) == sha(p)
            test_bins[name] = {'executed': str(p), 'retained': str(dest), 'sha256': sha(p)}
            save('test-binaries.json', test_bins)
    control = json.loads((S / 'baseline.json').read_text())['control']
    assert sha(control['path']) == control['sha256']
    qualified = json.loads(Path('/tmp/deadpan-one-leaf-replay-production-2026-10-23/release-native-binaries.json').read_text())
    binaries = {}
    for name in ['deadpan-app', 'deadpan-media-worker', 'deadpan-transcribe', 'deadpan-track']:
        p = TARGET / 'release' / name if name == 'deadpan-app' else Path(qualified[name]['executed'])
        if name != 'deadpan-app':
            assert sha(p) == qualified[name]['sha256']
        dest = S / 'bin' / name
        dest.parent.mkdir(exist_ok=True)
        shutil.copy2(p, dest)
        assert sha(p) == sha(dest)
        binaries[name] = {'source': str(p), 'executed': str(dest), 'sha256': sha(dest),
                          'freshly_built': name == 'deadpan-app', 'qualified_helper_reuse': name != 'deadpan-app'}
    save('binaries.json', binaries)
    shutil.copy2(TARGET / '.rustc_info.json', S / 'cargo-rustc-info.json')
    guard()
    print(json.dumps({'terminal': True, 'steps': len(rows), 'app': binaries['deadpan-app']['sha256']}), flush=True)


if __name__ == '__main__':
    main()
