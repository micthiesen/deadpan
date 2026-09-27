"""Derive paired stages from the retained host run, never unrelated percentiles."""
from pathlib import Path
import hashlib
import gzip
import json
import math
import sys

source = Path(sys.argv[2] if len(sys.argv) > 2 else '/tmp/deadpan-ui-performance-02/report.json')
payload = gzip.decompress(source.read_bytes()) if source.suffix == '.gz' else source.read_bytes()
expected_hash = '95a4994a039c79912b16e2f174872507502fd9cf2f6ed0bbf4df42101426079f'
assert hashlib.sha256(payload).hexdigest() == expected_hash
report = json.loads(payload)
scenario = next(s for s in report['scenarios'] if s['name'] == 'edit-latency')
metrics = {m['name']: [s['elapsed_ms'] for s in m['samples'] if s['outcome'] == 'completed']
           for m in scenario['timings']}
groups = {'repeat': [], 'hold': []}
current = None
for step in scenario['steps']:
    for event in step['semantic']['stages']:
        stage = event['stage']
        if stage == 'command_admitted':
            kind = {'Key Modifiers::NONE R': 'repeat', 'Command hold 11f': 'hold'}.get(step['input'])
            current = dict(kind=kind, admitted=event['wall_ms']) if kind else None
        elif current is not None:
            if stage == 'command_committed':
                current['committed'] = event['wall_ms']
            elif stage == 'picture_requested':
                assert 'ticket' not in current
                current['ticket'] = event['ticket']
                current['requested'] = event['wall_ms']
            elif stage in ('picture_received', 'picture_submitted', 'picture_composed'):
                assert event['ticket'] == current['ticket']
                current[stage] = event['wall_ms']
                if stage == 'picture_composed':
                    groups[current['kind']].append(current)
                    current = None


def distribution(values):
    ordered = sorted(values)
    return {'samples': len(values), 'p50_ms': ordered[math.ceil(len(values) * .5) - 1],
            'p95_ms': ordered[math.ceil(len(values) * .95) - 1], 'max_ms': ordered[-1]}


result = {'source_report': str(source), 'source_report_sha256': expected_hash,
          'source_binary_sha256': report['metadata']['binary_sha256'],
          'scope': 'Paired samples from the prior host release run; no new runtime or physical-display measurement',
          'kinds': {}}
for kind, samples in groups.items():
    assert len(samples) == 44
    samples = samples[4:]
    stem = 'cached_repeat' if kind == 'repeat' else 'hold'
    commit = metrics[stem + '_input_to_commit_ms']
    picture = metrics[('cached_repeat' if kind == 'repeat' else 'hold_fallback') + '_input_to_picture_complete_ms']
    assert len(commit) == len(picture) == len(samples) == 40
    values = []
    for sample, c, p in zip(samples, commit, picture):
        intervals = dict(
            observed_commit_to_picture_ms=p-c,
            commit_to_request_ms=sample['requested']-sample['committed'],
            request_to_received_ms=sample['picture_received']-sample['requested'],
            received_to_submitted_ms=sample['picture_submitted']-sample['picture_received'],
            submitted_to_composed_ms=sample['picture_composed']-sample['picture_submitted'],
        )
        assert all(v >= 0 for v in intervals.values())
        # The trace and the independently recorded metric pair share endpoints.
        assert abs((sample['picture_composed']-sample['committed'])-(p-c)) < .000001
        values.append(dict(ticket=sample['ticket'], **intervals))
    result['kinds'][kind] = dict(
        distributions={name: distribution([s[name] for s in values])
                       for name in values[0] if name != 'ticket'},
        paired_samples=values,
    )
Path(sys.argv[1]).write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({kind: data['distributions'] for kind, data in result['kinds'].items()}, indent=2))
