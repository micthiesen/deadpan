import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time

repo = pathlib.Path('/Users/michael/Code/deadpan')
work = pathlib.Path(tempfile.mkdtemp(prefix='deadpan-original-workflow-'))
pathlib.Path('/tmp/deadpan-original-workflow-current.txt').write_text(str(work))
cli = repo / 'target/debug/deadpan-cli'
app = repo / 'target/debug/deadpan-app'
package = work / 'originals.deadpan'
fixture = repo / 'native/deadpan-source/tests/fixtures/cfr-bframes.mp4'
source = work / 'camera.mp4'
shutil.copyfile(fixture, source)
events = []

def run(*args, error=None, app_headless=False):
    command = [str(app), '--headless'] if app_headless else [str(cli)]
    command.extend(map(str, args))
    start = time.monotonic()
    result = subprocess.run(command, capture_output=True, text=True)
    value = json.loads(result.stderr if error else result.stdout)
    event = {'command': command, 'exit_code': result.returncode,
             'seconds': time.monotonic() - start, 'response': value}
    events.append(event)
    (work / 'events.json').write_text(json.dumps(events, indent=2) + '\n')
    if error:
        assert result.returncode != 0 and value['error']['code'] == error, event
    else:
        assert result.returncode == 0, event
    return value

run('project', 'create', package, '--fps', '30000/1001', '--size', '320x180')
before = run('project', 'dump', package, '--json')
retained = run('project', 'retain-original', package, source, '--linked')
record = retained['retained_original']['record']
digest = record['object']['content']['digest']
assert retained['authored_asset_registered'] is False
assert retained['retained_original']['method'] == 'linked'
moved = work / 'moved.mp4'
source.rename(moved)
run('project', 'verify-original', package, digest, error='OriginalOffline')
wrong = work / 'wrong.mp4'
wrong.write_bytes(b'different complete original')
run('project', 'relink-original', package, digest, wrong, '--expected-version', 1,
    error='OriginalContentMismatch')
run('project', 'relink-original', package, digest, moved, '--expected-version', 1)
run('project', 'relink-original', package, digest, moved, '--expected-version', 1,
    error='OriginalLocationConflict')
managed = run('project', 'retain-original', package, moved)
assert managed['retained_original']['method'] == 'cloned', managed
dedup = run('project', 'retain-original', package, moved)
assert dedup['retained_original']['method'] == 'existing'
moved.unlink()
relocated = work / 'relocated.deadpan'
package.rename(relocated)
run('project', 'verify-original', relocated, digest, app_headless=True)
assert run('project', 'dump', relocated, '--json') == before
run('project', 'originals', relocated, app_headless=True)
run('project', 'validate', relocated)
object_path = relocated / 'Media/Originals' / ('blake3-' + digest)
assert object_path.read_bytes() == fixture.read_bytes()
assert object_path.stat().st_mode & 0o222 == 0
report = {
    'base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
    'scope': 'Complete original ownership and shared headless API; no authored import or audio readiness',
    'hardware': subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip(),
    'os': subprocess.check_output(['sw_vers'], text=True),
    'fixture': str(fixture.relative_to(repo)),
    'fixture_bytes': fixture.stat().st_size,
    'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest(),
    'cli_sha256': hashlib.sha256(cli.read_bytes()).hexdigest(),
    'app_sha256': hashlib.sha256(app.read_bytes()).hexdigest(),
    'retention_method': managed['retained_original']['method'],
    'checks': ['wrong-content rejection', 'offline linked source', 'version-checked relink',
               'stale relink rejection', 'actual APFS clone', 'verified deduplication',
               'source deletion', 'package relocation', 'app --headless readback',
               'full-file byte equality including AAC packets', 'read-only object',
               'unchanged authored snapshot', 'SQLite/history validation'],
    'command_count': len(events),
    'power_cache_state': 'Not controlled; these command durations are smoke observations, not product benchmarks',
}
(work / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'work': str(work), 'commands': len(events), 'method': report['retention_method']}))
