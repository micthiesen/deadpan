from pathlib import Path
import json

root = Path(__file__).parent
driver = Path('/tmp/deadpan-generated-export-20261011/chirp_exports.py').read_text()
exec(driver.split('for name,size,rate,seed in ')[0].replace("root/'chirp-v2-steps.json'", "root/'fixture-steps.json'"))
package = root / 'two-holds.deadpan'
invoke('copy', 'project', 'copy-portable', '/tmp/deadpan-generated-export-20261011/chirp-v2-1080p.deadpan', package)
original = invoke('before', 'project', 'dump', package, '--json')
old_holds = {node for node, value in original['nodes'].items() if value['kind']['type'] == 'hold'}
request = dict(protocol=1, project_id=original['project_id'], expected_revision=original['revision_id'], expected_bank_version=0, operation=dict(type='apply', parent=original['root'], cursor=91, selected_child=None, visual_selection=None, new_revision='two-holds-pause', program=dict(instructions=[dict(type='insert_pause', length=dict(unit='milliseconds', milliseconds=500))])))
request_path = root / 'pause-request.json'
request_path.write_text(json.dumps(request, indent=2) + '\n')
invoke('pause', 'macro', package, '--json', request_path)
fallback = invoke('fallback', 'project', 'dump', package, '--json')
new_holds = {node for node, value in fallback['nodes'].items() if value['kind']['type'] == 'hold'} - old_holds
assert len(new_holds) == 1, new_holds
generation = invoke('generate', 'generate-hold', package, '--hold', new_holds.pop(), '--seed', 815)
assert generation['state'] == 'Ready', generation
invoke('accept', 'accept-hold', package, '--request', generation['request_id'])
accepted = invoke('accepted', 'project', 'dump', package, '--json')
for node in old_holds:
    assert accepted['nodes'][node] == original['nodes'][node], 'First accepted Hold changed'
print('two independently accepted Holds retained', flush=True)
