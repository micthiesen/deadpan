#!/usr/bin/env python3
"""Record real relocated host admission and signed image substitutions."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import struct
import subprocess
import sys
import time

p = argparse.ArgumentParser()
p.add_argument('--bundle', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--source-project', type=Path, required=True)
p.add_argument('--whisper-model', type=Path, required=True)
p.add_argument('--identity', default='-')
p.add_argument('--managed-helpers-root', type=Path)
p.add_argument('--keychain', type=Path)
p.add_argument('--owner-signer-script', type=Path)
a = p.parse_args()
a.output.mkdir(exist_ok=False)
source = a.bundle.resolve(strict=True)
app = a.output.resolve() / 'Deadpan.app'
home = a.output.resolve() / 'home'
home.mkdir()
env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'TMPDIR': '/tmp'}
records = []
script_hash = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def run(name, command, success=True, message=None, owner_environment=False):
    command = [str(v) for v in command]
    # Fixture mutation needs the owner's signing identity. Tested hosts always
    # keep the isolated HOME and scrubbed environment defined above.
    used_environment = dict(env)
    if command[0] == '/usr/bin/codesign' or owner_environment:
        used_environment['HOME'] = str(Path.home())
    if command[0] == '/usr/bin/codesign' and '--sign' in command:
        selected = command[command.index('--sign') + 1]
        if selected != '-' and a.owner_signer_script:
            probe_path = a.output.resolve() / 'signing-probe'
            if not probe_path.exists():
                shutil.copyfile('/bin/echo', probe_path)
                probe_path.chmod(0o755)
            run(name + '-prepare-identity', [sys.executable, a.owner_signer_script,
                'dev.deadpan.signer-probe', probe_path], owner_environment=True)
    start = time.monotonic()
    timed_out = False
    try:
        r = subprocess.run(command, env=used_environment, capture_output=True, timeout=60)
    except subprocess.TimeoutExpired as error:
        timed_out = True
        r = subprocess.CompletedProcess(command, 124, error.stdout or b'', error.stderr or b'')
    (a.output / (name + '.stdout')).write_bytes(r.stdout)
    (a.output / (name + '.stderr')).write_bytes(r.stderr)
    records.append({'name': name, 'command': command, 'exit_code': r.returncode,
                    'seconds': time.monotonic() - start,
                    'expected_success': success, 'expected_message': message,
                    'environment': used_environment, 'timed_out': timed_out})
    (a.output / 'results.json').write_text(json.dumps({
        'source_bundle': str(source), 'environment': env, 'script_sha256': script_hash,
        'records': records, 'scope': 'Admission and local workflow; no concurrent same-user writer containment claim.'
    }, indent=2) + '\n')
    assert not timed_out, (name, 'timed out', r.stderr)
    assert r.returncode == (0 if success else 1), (name, r.returncode, r.stderr)
    if message:
        assert message.encode() in r.stdout + r.stderr, (name, r.stderr)
    return r


def cli(name, args, success=True, message=None, host='deadpan-cli'):
    command = [app / 'Contents/MacOS' / host]
    if host == 'deadpan-app':
        command.append('--headless')
    return run(name, command + args, success, message)


def probe(name, success=True, message=None):
    for host in ['deadpan-cli', 'deadpan-app']:
        r = cli(name + '-' + host, ['native-runtime-qualify'], success, message, host)
        if success:
            v = json.loads(r.stdout)
            assert len(v['content_pins']) == 9, v
            assert all(len(h) == 64 for h in v['content_pins'].values())


def sign_image(name, path, identity=None):
    run(name + '-remove-signature', ['/usr/bin/codesign', '--remove-signature', path])
    selected = identity or a.identity
    keychain = ['--keychain', a.keychain] if a.keychain and selected != '-' else []
    run(name + '-sign', ['/usr/bin/codesign', '--sign', selected,
                         '--timestamp=none', '--options', 'runtime'] + keychain + [path])


def seal(name):
    keychain = ['--keychain', a.keychain] if a.keychain and a.identity != '-' else []
    run(name + '-seal', ['/usr/bin/codesign', '--force', '--sign', a.identity,
                         '--timestamp=none', '--options', 'runtime'] + keychain + [app])
    run(name + '-valid-seal', ['/usr/bin/codesign', '--verify', '--deep', '--strict', app])


def mutate_uuid(path):
    data = bytearray(path.read_bytes())
    assert data[:4] == b'\xcf\xfa\xed\xfe', 'expected thin arm64 Mach-O'
    commands = struct.unpack_from('<I', data, 16)[0]
    cursor = 32
    for _ in range(commands):
        kind, size = struct.unpack_from('<II', data, cursor)
        assert size >= 8 and cursor + size <= len(data)
        if kind == 0x1b:
            assert size == 24
            data[cursor + 8] ^= 1
            path.write_bytes(data)
            return
        cursor += size
    raise AssertionError('no LC_UUID to mutate')


run('copy', ['/bin/cp', '-cR', source, app])
probe('relocated')
cli('alignment-still-qualified', ['refinement-runtime-qualify'])
project = a.output.resolve() / 'Workflow.deadpan'
cli('project-copy', ['project', 'copy-portable', a.source_project, project])
before = cli('document-before', ['project', 'dump', project, '--json']).stdout
model_hash = hashlib.sha256(a.whisper_model.read_bytes()).hexdigest()
transcribe = ['transcribe', project, '--model', a.whisper_model, '--sha256', model_hash]
track = ['track', project, '--from', '0', '--to', '1536', '--region', '0.2,0.2,0.4,0.4', '--through-shots']
regions = ['detect-regions', project, '--at', '0', '--include-faces']
cli('actual-transcription', transcribe)
cli('actual-tracking', track)
cli('actual-regions', regions)
assert before == cli('document-after', ['project', 'dump', project, '--json']).stdout

images = ['MacOS/deadpan-transcribe', 'MacOS/deadpan-track', 'MacOS/deadpan-media-worker']
images += sorted(str(q.relative_to(app / 'Contents')) for q in (app / 'Contents/Frameworks').iterdir()
                 if q.name != 'libonnxruntime.1.dylib')
assert len(images) == 9
for index, relative in enumerate(images):
    name = 'substitution-' + str(index)
    path = app / 'Contents' / relative
    if relative.startswith('MacOS/'):
        shutil.copyfile('/bin/echo', path)
    else:
        mutate_uuid(path)
    sign_image(name, path)
    seal(name)
    probe(name, False, 'compiled content pin')
    if relative == 'MacOS/deadpan-transcribe':
        cli(name + '-actual-transcription', transcribe, False, 'compiled content pin')
    if relative == 'MacOS/deadpan-track':
        cli(name + '-actual-tracking', track, False, 'compiled content pin')
        cli(name + '-actual-regions', regions, False, 'compiled content pin')
    shutil.copyfile(source / 'Contents' / relative, path)
    seal('restored-' + str(index))
    probe('restored-' + str(index))

info = app / 'Contents/Info.plist'
original_info = info.read_bytes()
value = plistlib.loads(original_info)
del value['DeadpanPackaging']
info.write_bytes(plistlib.dumps(value))
seal('removed-marker')
probe('removed-marker', False, 'marker')
cli('removed-marker-downloader', ['downloader', 'status', '--probe'], False, 'marker')
if a.managed_helpers_root:
    cli('removed-marker-explicit-helper-root',
        ['downloader', 'status', '--root', a.managed_helpers_root, '--probe'], False, 'marker')
doctor = json.loads(cli('removed-marker-doctor', ['doctor']).stdout)
assert doctor['runtime']['ai_runtime']['ready'] is False
assert 'damaged packaged app' in doctor['runtime']['ai_runtime']['lookup']
assert any('marker' in line for line in doctor['runtime']['ai_runtime']['missing'])
# A comment containing the old marker cannot substitute for the dictionary key.
text = info.read_text()
info.write_text(text.replace('</plist>', '<!-- <key>DeadpanPackaging</key><string>xtask-bundle-1</string> -->\n</plist>'))
seal('comment-marker')
probe('comment-marker', False, 'marker')
cli('comment-marker-downloader', ['downloader', 'status', '--probe'], False, 'marker')
info.write_bytes(original_info)
seal('restored-marker')
probe('restored-marker')

detached = app.with_name('Detached')
app.rename(detached)
app = detached
probe('outside-application', False, 'application')
cli('outside-application-downloader', ['downloader', 'status', '--probe'], False, 'application')
if a.managed_helpers_root:
    cli('outside-application-explicit-helper-root',
        ['downloader', 'status', '--root', a.managed_helpers_root, '--probe'], False, 'application')
doctor = json.loads(cli('outside-application-doctor', ['doctor']).stdout)
assert doctor['runtime']['ai_runtime']['ready'] is False
assert 'damaged packaged app' in doctor['runtime']['ai_runtime']['lookup']
app.rename(app.with_name('Deadpan.app'))
app = app.with_name('Deadpan.app')
probe('restored-location')

if a.identity != '-':
    path = app / 'Contents/MacOS/deadpan-transcribe'
    sign_image('wrong-signer', path, '-')
    seal('wrong-signer')
    probe('wrong-signer', False, 'specified code requirement')
    shutil.copyfile(source / 'Contents/MacOS/deadpan-transcribe', path)
    seal('restored-signer')
    probe('restored-signer')

assert before == cli('document-final', ['project', 'dump', project, '--json']).stdout
print(json.dumps({'passed': True, 'operations': len(records), 'images': images,
                  'authored_document_unchanged': True}))
