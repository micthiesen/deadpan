from pathlib import Path
import json
from check_profile import SCHEMA, PREFIX, EXPECTED, parse_profile

values = {row['name']: 1 for row in EXPECTED}
for row in EXPECTED:
    if row['expected_elapsed_ns'] is not None:
        values[row['name']] = row['expected_elapsed_ns']
for equation in reversed(SCHEMA['containment']):
    values[equation['parent']] = sum(values[name] for name in equation['disjoint_children']) + 5
lines = [PREFIX + ' BEGIN count=1']
for row in EXPECTED:
    lines.append(PREFIX + ' PHASE phase=' + row['name'] + ' kind=' + row['kind'] + ' elapsed_ns=' + str(values[row['name']]) + ' count=' + str(row['expected_count']))
lines.append(PREFIX + ' END count=1')
valid = '\n'.join(lines)
result = parse_profile('unrelated stderr\n' + valid + '\nother stderr')
assert result['profile_lines'] == 35 and len(result['nanoseconds']) == 33
cases = {}
cases['duplicate_block'] = valid + '\n' + valid
cases['missing_row'] = '\n'.join(lines[:5] + lines[6:])
altered = lines.copy()
altered[1] = altered[1].replace('phase=app_service_total', 'phase=unknown')
cases['unknown_phase'] = '\n'.join(altered)
altered = lines.copy()
altered[1] = altered[1].replace('kind=inclusive_total', 'kind=wrong')
cases['wrong_kind'] = '\n'.join(altered)
altered = lines.copy()
altered[1] = altered[1].replace('count=1', 'count=2')
cases['wrong_count'] = '\n'.join(altered)
altered = lines.copy()
altered[-2] = altered[-2].replace('count=0', 'count=1')
cases['debug_count'] = '\n'.join(altered)
altered = lines.copy()
altered[-2] = altered[-2].replace('elapsed_ns=0', 'elapsed_ns=1')
cases['debug_elapsed'] = '\n'.join(altered)
marker_index = next(index for index, line in enumerate(lines) if 'phase=app_refresh_succeeded ' in line)
altered = lines.copy()
altered[marker_index] = altered[marker_index].replace('elapsed_ns=0', 'elapsed_ns=1')
cases['marker_elapsed'] = '\n'.join(altered)
altered = lines.copy()
altered[1] = altered[1].replace('elapsed_ns=' + str(values['app_service_total']), 'elapsed_ns=0')
cases['negative_containment'] = '\n'.join(altered)
altered = lines.copy()
altered[1] = altered[1].replace('elapsed_ns=' + str(values['app_service_total']), 'elapsed_ns=-1')
cases['negative_elapsed'] = '\n'.join(altered)
altered = lines.copy()
altered[1], altered[2] = altered[2], altered[1]
cases['wrong_order'] = '\n'.join(altered)
altered = lines.copy()
altered[0] = 'extra prefix ' + altered[0]
cases['embedded_prefix'] = '\n'.join(altered)
altered = lines.copy()
altered[1] += ' trailing'
cases['trailing_phase_data'] = '\n'.join(altered)
altered = lines.copy()
altered.insert(3, altered[2])
cases['duplicate_row'] = '\n'.join(altered)
altered = lines.copy()
altered[-1] = PREFIX + ' END count=2'
cases['wrong_end'] = '\n'.join(altered)
rejected = []
for name, text in cases.items():
    try:
        parse_profile(text)
    except ValueError:
        rejected.append(name)
    else:
        raise AssertionError('Parser accepted invalid case: ' + name)
assert len(rejected) == len(cases)
record = {'synthetic_only': True, 'valid_shape_passed': True, 'invalid_shapes_rejected': rejected, 'invalid_case_count': len(rejected), 'no_runtime_or_performance_claim': True}
Path(__file__).with_name('parser-tests.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record))
