"""Collect completed production measurements without running media or models."""
from collections import Counter
from pathlib import Path
import json
import re

root = Path(__file__).resolve().parent

def read(path):
    return json.loads(path.read_text())

def swap_used(sample):
    value = sample.get('swap', {}).get('stdout', '')
    match = re.search(r'used\s*=\s*([0-9.]+)([KMG])', value)
    return float(match[1]) * {'K': 1024, 'M': 1024**2, 'G': 1024**3}[match[2]] if match else None

rows = []
for result in read(root / 'results.json'):
    row = dict(result)
    case = root / result['case']
    if not result['ready']:
        rows.append(row)
        continue
    generated = read(case / 'generated.json')
    provenance = generated['ready']['provenance']
    envelope = read(case / 'project.deadpan/Media/Generated' / ('blake3-' + provenance['content']['digest']))
    worker = json.loads(envelope['worker_provenance_utf8'])
    samples = read(case / 'generated.system.json')
    swaps = [swap_used(sample) for sample in samples]
    available = [value for value in swaps if value is not None]
    pressure = []
    for sample in samples:
        match = re.search(r'free percentage:\s*(\d+)%', sample.get('memory_pressure', {}).get('stdout', ''))
        if match:
            pressure.append(int(match[1]))
    row.update({
        'wall_seconds': read(case / 'generated.execution.json')['seconds'],
        'conditioning_ms': generated['conditioning_ms'],
        'plan': generated['plan'],
        'provider': envelope['binding']['provider'],
        'objects': {key: generated['ready'][key] for key in ['native', 'sampled', 'provenance']},
        'backend_seconds': worker['backend_seconds'],
        'worker_reported_peak_rss_bytes': worker['process_peak_rss_bytes'],
        'mlx_counter_at_end_bytes': worker['mlx_counter_at_end_bytes'],
        'source_latent_preservation': worker['source_latent_preservation'],
        'timing': worker['timing'],
        'motion_coverage': dict(Counter(item['motion']['status'] for item in envelope['pixels']['motion']['transitions'])),
        'entry': envelope['pixels']['endpoints']['entry'],
        'exit': envelope['pixels']['endpoints']['exit'],
        'geometry': envelope['geometry']['geometry'],
        'region_unavailable_reason': envelope['geometry']['region_unavailable_reason'],
        'unchanged_after_ready': read(case / 'before.json') == read(case / 'after-ready.json'),
        'system': {
            'sample_count': len(samples),
            'swap_start_bytes': swaps[0], 'swap_end_bytes': swaps[-1],
            'swap_max_bytes': max(available) if available else None,
            'minimum_system_free_percentage': min(pressure) if pressure else None,
            'thermal_reports': sorted(set(sample.get('thermal', {}).get('stdout', '') for sample in samples)),
            'unavailable': [{key: value for key, value in sample.items() if isinstance(value, dict) and ('unavailable' in value or value.get('exit_code') != 0)} for sample in samples],
        },
    })
    rows.append(row)
summary = {
    'executed_binary': read(root / 'executed-binary.json'),
    'complete': len(rows) == len(read(root / 'cases.json')),
    'ready_count': sum(row['ready'] for row in rows),
    'limits': ['One attempt per duration/direction; no warm-model distribution.',
               'Synthetic footage; human acceptability and real-person identity are unmeasured.',
               'Separate cold worker processes on a shared Mac; OS file cache uncontrolled.',
               'Process peak RSS and sampled system state do not establish peak unified-memory pressure.',
               'No interactive editing workload was run during this matrix.'],
    'cases': rows,
}
(root / 'measurement-summary.json').write_text(json.dumps(summary, indent=2)+'\n')
for row in rows:
    print(row['case'], 'Ready' if row['ready'] else 'Failed', row.get('wall_seconds'), row.get('worker_reported_peak_rss_bytes'))
