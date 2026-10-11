from pathlib import Path
import hashlib
import json
import os
import subprocess
import signal
import time

root = Path(__file__).parent / 'packaged-4k'
root.mkdir(exist_ok=True)
source_bundle = Path('/tmp/deadpan-youtube-network-bundle-20261011/Deadpan.app')
bundle = root / 'Relocated Deadpan.app'
if not bundle.exists():
    subprocess.run(['cp', '-cR', str(source_bundle), str(bundle)], check=True)
for name in ['home', 'tmp', 'cwd']:
    (root / name).mkdir(mode=0o700, exist_ok=True)
env = {'HOME': str(root / 'home'), 'TMPDIR': str(root / 'tmp') + '/',
       'PATH': '/usr/bin:/bin', 'LANG': 'en_US.UTF-8', 'LC_ALL': 'en_US.UTF-8'}
app = bundle / 'Contents/MacOS/deadpan-app'
package = root / 'Charge.deadpan'
rows = json.loads((root / 'results.json').read_text()) if (root / 'results.json').exists() else []

def read_records(path):
    decoder = json.JSONDecoder()
    rest = path.read_text().lstrip()
    records = []
    while rest:
        record, end = decoder.raw_decode(rest)
        records.append(record)
        rest = rest[end:].lstrip()
    return records

def run(name, args):
    previous = next((row for row in rows if row['name'] == name), None)
    if previous:
        assert previous['exit'] == 0, previous
        return read_records(root / (name + '.jsonl'))
    command = [str(app), '--headless', *args]
    started = time.monotonic()
    with (root / (name + '.jsonl')).open('x') as out, (root / (name + '.stderr')).open('x') as err:
        process = subprocess.Popen(command, env=env, cwd=root / 'cwd', stdout=out, stderr=err)
        try:
            code = process.wait(timeout=1200)
        except subprocess.TimeoutExpired:
            process.send_signal(signal.SIGINT)
            code = process.wait(timeout=60)
            raise RuntimeError(f'{name} exceeded the harness deadline and was cancelled (exit {code})')
    rows.append(dict(name=name, command=command, exit=code, seconds=time.monotonic()-started))
    (root / 'results.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(json.dumps(rows[-1]), flush=True)
    assert code == 0, name
    return read_records(root / (name + '.jsonl'))

identity = dict(base=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
                environment=env, executable_sha256=hashlib.sha256(app.read_bytes()).hexdigest(),
                source_url='https://www.youtube.com/watch?v=UXqq0ZvbOnk',
                rights_source='https://commons.wikimedia.org/wiki/File:Charge_-_Blender_Open_Movie-full_movie.webm',
                source_credit='Charge, Blender Studio, CC BY 4.0 as recorded by Wikimedia Commons; YouTube metadata retained below.')
(root / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
run('baseline-probe', ['downloader', 'status', '--probe'])
events = run('create', ['project', 'create-from-url', str(package), identity['source_url']])
metadata = next(row for row in events if row.get('event') == 'metadata')
assert metadata['selection']['video']['width'] >= 3840, metadata
assert metadata['selection']['video']['height'] > 1080, metadata
run('validate', ['project', 'validate', str(package)])
run('view', ['project', 'view', str(package)])
run('dump', ['project', 'dump', str(package), '--json'])
run('doctor', ['doctor', '--project', str(package)])
with (root / 'codesign.log').open('x') as log:
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(bundle)], stdout=log, stderr=subprocess.STDOUT, check=True)
assert not list((root / 'tmp').glob('deadpan-youtube-*')), 'private workspace leaked'
assert not list(root.glob('.*.creating.deadpan')), 'creation staging leaked'
(root / 'source-metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
print('Packaged 4K import, reopen, validation, signature and cleanup passed', flush=True)
