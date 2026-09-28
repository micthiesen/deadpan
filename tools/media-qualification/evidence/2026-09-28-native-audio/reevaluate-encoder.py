"""Re-evaluate retained encoder observations without another encode/decode run."""
import copy
import hashlib
import json
from pathlib import Path
import sys

repo = Path('/Users/michael/Code/deadpan')
sys.path.insert(0, str(repo / 'tools/media-qualification/compatible'))
from qualify_encoder import evaluate_case, source_inventory

scratch = Path(__file__).resolve().parent
source = scratch / 'video-followup.json'
before = source_inventory()
report = json.loads(source.read_text())
result = {'source_report_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
          'source_sha256_at_start': before, 'cases': [], 'changed_existing_checks': []}
for old in report['cases']:
    if 'observations' not in old:
        continue
    for artifact in [*old['artifacts'].values(),
                     *[value['pcm'] for value in old['audio'].values()]]:
        path = Path(artifact['path'])
        assert path.stat().st_size == artifact['bytes'], path
        assert hashlib.sha256(path.read_bytes()).hexdigest() == artifact['sha256'], path
    new = copy.deepcopy(old)
    new['status'] = 'captured, acceptance not evaluated'
    evaluate_case(new)
    old_checks = {check['label']: check for check in old['checks']}
    new_checks = {check['label']: check for check in new['checks']}
    assert len(old_checks) == len(old['checks'])
    assert len(new_checks) == len(new['checks'])
    for label, check in old_checks.items():
        if new_checks.get(label) != check:
            result['changed_existing_checks'].append({'case': old['name'], 'label': label,
                'old': check, 'new': new_checks.get(label)})
    for key in ('scoped_checks_passed', 'unqualified', 'observations'):
        if new.get(key) != old.get(key):
            result['changed_existing_checks'].append({'case': old['name'], 'field': key,
                'old': old.get(key), 'new': new.get(key)})
    result['cases'].append({key: new.get(key) for key in
                           ('name', 'status', 'checks', 'observations', 'unqualified', 'scoped_checks_passed')})
result['source_sha256'] = source_inventory()
result['source_unchanged_during_run'] = result['source_sha256'] == before
result['passed'] = not result['changed_existing_checks'] and result['source_unchanged_during_run']
(scratch / 'native-encoder-regression.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'passed': result['passed'], 'cases': len(result['cases']),
                  'changed_existing_checks': len(result['changed_existing_checks'])}))
raise SystemExit(0 if result['passed'] else 1)
