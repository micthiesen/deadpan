#!/usr/bin/env python3
"""Exercise the real relocated hosts, then independently re-signed substitutions."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--bundle', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--project', type=Path, required=True)
parser.add_argument('--pack-source', type=Path, required=True)
parser.add_argument('--unpinned-cli', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(exist_ok=False)
source = args.bundle.resolve(strict=True)
app = args.output.resolve() / 'Deadpan.app'
environment = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin',
               'HOME': str(args.output.resolve() / 'home'), 'TMPDIR': '/tmp'}
Path(environment['HOME']).mkdir()
records = []


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(name, command, expected=True, message=None):
    started = time.monotonic()
    result = subprocess.run([str(v) for v in command], env=environment,
                            capture_output=True, timeout=180)
    (args.output / f'{name}.stdout').write_bytes(result.stdout)
    (args.output / f'{name}.stderr').write_bytes(result.stderr)
    record = {'name': name, 'command': [str(v) for v in command],
              'exit_code': result.returncode, 'seconds': time.monotonic() - started,
              'expected_success': expected, 'expected_message': message}
    records.append(record)
    (args.output / 'results.json').write_text(json.dumps({
        'source_bundle': str(source), 'environment': environment, 'records': records,
        'scope': 'Relocated real inference and re-signed admission failures; no same-user concurrent-writer containment claim.'
    }, indent=2) + '\n')
    assert (result.returncode == 0) == expected, (name, result.returncode, result.stderr)
    if message is not None:
        assert message.encode() in result.stderr + result.stdout, (name, result.stderr)
    return result


def probe(name, expected=True, message=None, both=False):
    for host in (['deadpan-cli', 'deadpan-app'] if both else ['deadpan-cli']):
        command = [app / 'Contents/MacOS' / host]
        if host == 'deadpan-app':
            command.append('--headless')
        command.append('refinement-runtime-qualify')
        result = run(f'{name}-{host}', command, expected, message)
        if expected:
            value = json.loads(result.stdout)
            assert len(value['worker_content_sha256']) == 64
            assert len(value['ort_content_sha256']) == 64


def sign(name):
    # Clean the two images, then seal the parent without recursively forcing
    # another signature over them. Forced re-signing can retain stale bytes.
    for image, relative in [('worker', 'Contents/MacOS/deadpan-align'),
                            ('ort', 'Contents/Frameworks/libonnxruntime.1.dylib')]:
        path = app / relative
        run(f'{name}-{image}-remove-signature', ['/usr/bin/codesign', '--remove-signature', path])
        run(f'{name}-{image}-sign', ['/usr/bin/codesign', '--force', '--sign', '-',
                                     '--options', 'runtime', path])
    run(f'{name}-sign', ['/usr/bin/codesign', '--force', '--sign', '-',
                         '--options', 'runtime', app])
    run(f'{name}-valid-seal', ['/usr/bin/codesign', '--verify', '--deep', '--strict', app])


run('copy', ['/bin/cp', '-cR', source, app])
images = ['Contents/MacOS/deadpan-cli', 'Contents/MacOS/deadpan-app',
          'Contents/MacOS/deadpan-align', 'Contents/Frameworks/libonnxruntime.1.dylib']
(args.output / 'source-images.json').write_text(json.dumps(
    {name: {'bytes': (source / name).stat().st_size, 'sha256': sha(source / name)}
     for name in images}, indent=2) + '\n')
probe('relocated', both=True)
models = args.output.resolve() / 'models'
run('offline-model-import', [app / 'Contents/MacOS/deadpan-cli', 'models', 'import',
                             'phoneme-ctc-en', args.pack_source, '--root', models],
    message='"event":"smoke_test"')
run('actual-refinement', [app / 'Contents/MacOS/deadpan-cli', 'refine-words',
                         args.project, '--word', '0', '--models-root', models])

for name, relative in [('worker', 'Contents/MacOS/deadpan-align'),
                       ('ort', 'Contents/Frameworks/libonnxruntime.1.dylib')]:
    shutil.copy2('/bin/echo', app / relative)
    sign(f'substituted-{name}')
    probe(f'substituted-{name}', False, 'compiled content pin', both=True)
    shutil.copy2(source / relative, app / relative)
    sign(f'restored-{name}')
    probe(f'restored-{name}')

info = app / 'Contents/Info.plist'
original_info = info.read_bytes()
value = plistlib.loads(original_info)
del value['DeadpanPackaging']
info.write_bytes(plistlib.dumps(value))
sign('removed-marker')
probe('removed-marker', False, 'lost its DeadpanPackaging marker', both=True)
info.write_bytes(original_info)
sign('restored-marker')
probe('restored-marker')

cli = app / 'Contents/MacOS/deadpan-cli'
shutil.copy2(args.unpinned_cli, cli)
sign('unpinned-host')
probe('unpinned-host', False, 'requires both compiled content pins')
shutil.copy2(source / 'Contents/MacOS/deadpan-cli', cli)
sign('restored-host')
probe('restored-host', both=True)
assert app.parent == args.output.resolve()
shutil.rmtree(app)
print(json.dumps({'passed': True, 'checks': len(records), 'copy_removed': True}))
