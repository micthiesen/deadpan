#!/usr/bin/env python3
"""Post-build relocated AI admission/workflow checks; never run by prepare.py."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parent
OWNER = Path.home()
OFFLINE = '(version 1)(allow default)(deny network-outbound (remote ip "*:*"))(deny network-inbound (local ip "*:*"))(deny network-bind (local ip "*:*"))'
p = argparse.ArgumentParser()
p.add_argument('--bundle', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--check-args', default='["models","check","ltx-2.3-q4-extension"]', help='JSON CLI argument vector for real WorkerMode::Check')
p.add_argument('--identity', required=True)
p.add_argument('--keychain', type=Path, default=OWNER/'Library/Keychains/dotfiles-signing.keychain-db')
p.add_argument('--owner-signer-script', type=Path, default=OWNER/'.dotfiles/scripts/macos-sign.py')
p.add_argument('--negative', action='store_true')
p.add_argument('--workflow', action='store_true', help='Author scratch fixture, infer one extension, accept/render/Undo')
a = p.parse_args()
if a.negative and not a.workflow:
    p.error('--negative requires --workflow for the actual inference refusal check')
assert (ROOT/'setup.json').is_file(), 'Run prepare.py first'
check_args = json.loads(a.check_args)
assert isinstance(check_args, list) and all(isinstance(x, str) for x in check_args)
a.output.mkdir(parents=True, exist_ok=False)
out = a.output.resolve()
source = a.bundle.resolve(strict=True)
app = out/'Relocated app with spaces'/'Deadpan.app'
app.parent.mkdir()
subprocess.run(['cp', '-cR', str(source), str(app)], check=True)
home = ROOT/'home'
env = {'HOME': str(home), 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'TMPDIR': '/tmp'}
runtime = app/'Contents/Resources/ai-runtime'
inventory = app/'Contents/Resources/ai-runtime-inventory.json'
records = []
tracked_inputs = [source/'Contents/MacOS'/name for name in ['deadpan-cli', 'deadpan-app']]
tracked_inputs += [source/'Contents/Resources/ai-runtime-inventory.json', ROOT/'edge-from-left.mp4']
repo = Path('/Users/michael/Code/deadpan')
tracked_inputs += [repo/path for path in [
    'crates/deadpan-cli/src/bundle.rs', 'crates/deadpan-cli/src/bundle/ai.rs',
    'crates/deadpan-cli/src/bundle/ai_inventory.rs',
    'crates/deadpan-cli/src/generation/runtime/launch.rs',
    'crates/deadpan-cli/src/generation/attempt.rs', 'crates/deadpan-cli/src/models.rs',
    'crates/deadpan-cli/src/lib.rs']]
input_fingerprints = {str(path): {'bytes': path.stat().st_size,
    'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in tracked_inputs}

def final_error_message(result):
    # Only the host's final structured error counts. Progress text or an earlier
    # event cannot turn an unrelated nonzero exit into a successful negative.
    for line in reversed(result.stderr.decode('utf-8', errors='replace').splitlines()):
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and isinstance(value.get('error'), dict):
            message = value['error'].get('message')
            if isinstance(message, str):
                return message
    raise AssertionError('No final structured host error: '+repr(result.stderr[-2000:]))

def require_trust_error(result, case, real_check=False):
    message = final_error_message(result)
    lower = message.lower()
    if case == 'removed-marker':
        accepted = 'deadpanpackaging marker' in lower or 'packaging marker' in lower
    elif real_check:
        accepted = ('ai runtime could not be admitted' in lower
            or 'compiled inventory' in lower
            or 'ai runtime inventory differs from the host' in lower
            or (case == 'identity-fifo' and "bundled ai runtime's identity is missing or unreadable" in lower)
            or (case == 'missing-worker' and 'bundled worker is missing' in lower
                and 'reinstall deadpan' in lower))
    else:
        accepted = ('inventory' in lower or 'compiled pin' in lower
            or ('ai runtime' in lower and any(word in lower for word in
                ['differs', 'special file', 'privately owned', 'symlink', 'bound', 'fixed packaged path'])))
    assert accepted, (case, 'unrelated refusal cannot count as trust evidence', message)
    return message

def run(name, command, success=True, timeout=240, owner=False, offline=False):
    command = [str(x) for x in command]
    if offline:
        command = ['/usr/bin/sandbox-exec', '-p', OFFLINE] + command
    used = dict(env)
    if owner:
        used['HOME'] = str(OWNER)
    start = time.monotonic()
    timed_out = False
    try:
        result = subprocess.run(command, env=used, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        timed_out = True
        result = subprocess.CompletedProcess(command, 124, error.stdout or b'', error.stderr or b'')
    (out/(name+'.stdout')).write_bytes(result.stdout)
    (out/(name+'.stderr')).write_bytes(result.stderr)
    records.append({'name': name, 'command': command, 'exit_code': result.returncode,
                    'seconds': time.monotonic()-start, 'expected_success': success,
                    'timed_out': timed_out, 'environment': used})
    (out/'results.json').write_text(json.dumps({'source_bundle': str(source), 'app': str(app),
        'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'setup': str(ROOT/'setup.json'), 'input_fingerprints': input_fingerprints, 'records': records,
        'offline_scope': 'AI Check/Inference use production deny-network sandbox; outer IP denial only for non-AI commands because macOS rejects nested sandbox-exec.'}, indent=2)+'\n')
    assert not timed_out, (name, 'timeout')
    assert result.returncode == (0 if success else 1), (name, result.returncode, result.stderr[-3000:])
    return result

def cli(name, args, success=True, host='deadpan-cli', **options):
    command = [app/'Contents/MacOS'/host]
    if host == 'deadpan-app':
        command.append('--headless')
    return run(name, command+list(args), success, **options)

def probe(name, success=True, real_check=False, check_timeout=600):
    for host in ['deadpan-cli', 'deadpan-app']:
        result = cli(name+'-'+host, ['ai-runtime-qualify'], success, host=host)
        if success:
            pin = json.loads(result.stdout)['inventory_sha256']
            assert isinstance(pin, str) and re.fullmatch('[0-9a-f]{64}', pin), 'missing compiled AI pin'
        else:
            require_trust_error(result, name)
        if real_check:
            result = cli(name+'-check-'+host, check_args, success, host=host, timeout=check_timeout)
            if not success:
                require_trust_error(result, name, real_check=True)

def refuse_actual_inference(name):
    before = json.loads(cli(name+'-inference-document-before',
        ['project', 'dump', project, '--json']).stdout)
    result = cli(name+'-actual-inference-refused', ['generate-hold', project,
        '--hold', 'ai-trust-hold', '--seed', '10', '--mode', 'extend-left', '--motion', 'moderate'],
        success=False, timeout=240)
    message = require_trust_error(result, name, real_check=True)
    report = json.loads(result.stdout)
    assert report['state'] == 'Failed' and report['ready'] is None, report
    assert all(variant['state'] != 'Ready' for variant in report['variants']), report
    failure = report['failure']
    assert isinstance(failure, str) and failure.startswith('Io: ') and (
        'could not be admitted' in failure.lower()
        or 'compiled inventory' in failure.lower()), (message, failure)
    after = json.loads(cli(name+'-inference-document-after',
        ['project', 'dump', project, '--json']).stdout)
    assert after == before, 'Refused inference must preserve the authored fallback'

def signer_ready(name):
    path = out/'signing-probe'
    if not path.exists():
        shutil.copyfile('/bin/echo', path)
        path.chmod(0o755)
    run(name+'-identity', [sys.executable, a.owner_signer_script,
        'dev.deadpan.signer-probe', path], owner=True)

def sign_image(name, path):
    signer_ready(name)
    run(name+'-remove-signature', ['/usr/bin/codesign', '--remove-signature', path], owner=True)
    run(name+'-sign', ['/usr/bin/codesign', '--sign', a.identity, '--keychain', a.keychain,
        '--timestamp=none', '--options', 'runtime', path], owner=True)

def seal(name):
    signer_ready(name)
    run(name+'-seal', ['/usr/bin/codesign', '--force', '--sign', a.identity,
        '--keychain', a.keychain, '--timestamp=none', '--options', 'runtime', app], owner=True)
    run(name+'-strict', ['/usr/bin/codesign', '--verify', '--deep', '--strict', app], owner=True)

def mutate_uuid(path):
    data = bytearray(path.read_bytes())
    assert data[:4] == b'\xcf\xfa\xed\xfe', 'expected thin arm64 Mach-O'
    cursor = 32
    for _ in range(struct.unpack_from('<I', data, 16)[0]):
        kind, size = struct.unpack_from('<II', data, cursor)
        assert size >= 8 and cursor+size <= len(data)
        if kind == 0x1b:
            data[cursor+8] ^= 1
            path.write_bytes(data)
            return
        cursor += size
    raise AssertionError('no LC_UUID')

def change_file(name, path, change, resigned=False):
    backup = out/(name+'.backup')
    shutil.copy2(path, backup)
    try:
        change(path)
        if resigned:
            sign_image(name, path)
        seal(name)
        probe(name, False, real_check=True)
        if name == 'script':
            refuse_actual_inference(name)
    finally:
        if path.is_symlink() or path.exists():
            path.unlink()
        shutil.copy2(backup, path)
        seal(name+'-restore')

def structural_case(name, create, remove):
    # These shapes may themselves be unsealable. The host must refuse them
    # without following aliases or opening a special file. Keep their exact
    # diagnostic, without treating external codesign rejection as host proof.
    try:
        create()
        probe(name, False)
    finally:
        remove()


project = out/'Workflow.deadpan'
created = json.loads(cli('create-original', ['project', 'create-original', project, ROOT/'edge-from-left.mp4']).stdout)['created']
assert created['presentation_basis']['frame_rate'] == {'numerator': 24, 'denominator': 1}
document = json.loads(cli('original', ['project', 'dump', project, '--json']).stdout)
revision = str(uuid.uuid4())
request = {'protocol': 1, 'project_id': document['project_id'],
    'expected_revision': document['revision_id'], 'new_revision': revision,
    'command': {'command': 'insert', 'parent': document['root'], 'index': 1,
        'subtree': {'root': 'ai-trust-hold', 'nodes': {'ai-trust-hold': {
            'label': 'AI runtime trust regression', 'kind': {'type': 'hold',
            'recipe': {'duration': 12, 'video': {'type': 'freeze',
                'asset': created['asset_id'], 'timestamp': {'ticks': 8,
                'time_base': {'numerator': 1, 'denominator': 24}}},
                'audio': {'type': 'silence'}}}}},
            'overrides': {}, 'gap_overrides': {}}}}
request_path = out/'insert-hold.json'
request_path.write_text(json.dumps(request, indent=2)+'\n')
cli('insert-hold-preview', ['command', project, '--json', request_path, '--dry-run'])
cli('insert-hold', ['command', project, '--json', request_path])
fallback = json.loads(cli('fallback', ['project', 'dump', project, '--json']).stdout)

print(json.dumps({'status':'prepared', 'project':str(project), 'records':len(records)}))
