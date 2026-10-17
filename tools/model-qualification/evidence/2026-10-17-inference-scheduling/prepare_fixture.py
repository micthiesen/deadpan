from pathlib import Path
import json
import subprocess

root = Path(__file__).resolve().parent
package = root / 'fixture.deadpan'
cli = ['/tmp/deadpan-sleep-final-bin-2026-10-17/deadpan-app', '--headless']
source = '/Users/michael/Code/deadpan/tools/model-qualification/evidence/2026-10-08-extension-one-second/fixtures/from_left.mp4'

def run(name, *args):
    result = subprocess.run(cli + list(map(str, args)), capture_output=True, text=True, check=True)
    (root / (name + '.json')).write_text(result.stdout)
    return json.loads(result.stdout)

created = run('created', 'project', 'create-original', package, source)['created']
document = run('original', 'project', 'dump', package, '--json')
request = {'protocol': 1, 'project_id': document['project_id'],
           'expected_revision': document['revision_id'], 'new_revision': 'scheduling-pause',
           'command': {'command': 'insert', 'parent': document['root'], 'index': 1,
             'subtree': {'root': 'extension', 'nodes': {'extension': {
               'label': 'Extension scheduling qualification', 'kind': {'type': 'hold',
                 'recipe': {'duration': 12, 'video': {'type': 'freeze',
                   'asset': created['asset_id'], 'timestamp': {'ticks': 8,
                   'time_base': {'numerator': 1, 'denominator': 24}}},
                   'audio': {'type': 'silence'}}}}}, 'overrides': {}, 'gap_overrides': {}}}}
path = root / 'insert-request.json'
path.write_text(json.dumps(request))
run('inserted', 'command', package, '--json', path)
run('before', 'project', 'dump', package, '--json')
print(package)
