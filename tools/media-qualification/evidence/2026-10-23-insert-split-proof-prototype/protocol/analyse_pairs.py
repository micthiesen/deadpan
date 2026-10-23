from pathlib import Path
import hashlib
import json
import statistics

S = Path('/tmp/deadpan-insert-split-proof-prototype-v3-2026-10-23')
trials = json.loads((S / 'trials.json').read_text())
assert len(trials) == 18
assert json.loads((S / 'measurement-terminal.json').read_text())['status'] == 'complete'
metrics = {
    'input_to_commit_ms': 'large_first_pause_input_to_commit_ms',
    'input_to_offscreen_composition_ms': 'large_first_pause_input_to_offscreen_background_composition_ms',
}
pairs = []
for rotation in range(1, 7):
    group = {r['kind']: r for r in trials if r['rotation'] == rotation}
    assert set(group) == {'control', 'baseline', 'candidate'}
    values = {}
    for kind in ['baseline', 'candidate']:
        row = group[kind]
        assert row['checks'] == 7 and row['kestrel_checks'] == 1
        assert row['authored_input_unchanged'] and row['wal_empty']
        block = row['profile']['selected']
        phases = {r['name']: r['elapsed_ns'] for r in block['phases']}
        assert len(phases) == 35 and sum(phases.values()) == block['elapsed_ns']
        values[kind] = {name: row['metrics'][key] for name, key in metrics.items()}
        values[kind]['worker_ms'] = block['elapsed_ns'] / 1e6
        values[kind]['legacy_insertion_including_input_ms'] = (
            phases['leaf_insert_known'] + phases['leaf_input_durations']) / 1e6
        values[kind]['pure_leaf_region_ms'] = sum(phases[name] for name in [
            'semantic_leaf_apply', 'leaf_insert_known',
            'leaf_input_durations', 'leaf_net_construction']) / 1e6
    pairs.append({
        'rotation': rotation, 'values': values,
        'baseline_minus_candidate_ms': {
            name: values['baseline'][name] - values['candidate'][name]
            for name in values['baseline']},
        'relative_reduction_percent': {
            name: 100 * (values['baseline'][name] - values['candidate'][name]) / values['baseline'][name]
            for name in values['baseline']},
    })

def stats(values):
    return {'minimum': min(values), 'median': statistics.median(values), 'maximum': max(values)}

result = {
    'diagnostic_only': True, 'n_pairs': 6,
    'method': 'same-rotation baseline minus candidate, independent fresh processes; no percentile, target or calibrated overhead claim',
    'trials_sha256': hashlib.sha256((S / 'trials.json').read_bytes()).hexdigest(),
    'pairs': pairs,
    'paired_difference_ms': {
        name: stats([r['baseline_minus_candidate_ms'][name] for r in pairs])
        for name in pairs[0]['baseline_minus_candidate_ms']},
    'paired_relative_reduction_percent': {
        name: stats([r['relative_reduction_percent'][name] for r in pairs])
        for name in pairs[0]['relative_reduction_percent']},
}
with (S / 'paired-analysis.json').open('x') as output:
    output.write(json.dumps(result, indent=2) + '\n')
print(json.dumps(result['paired_difference_ms'], indent=2))
