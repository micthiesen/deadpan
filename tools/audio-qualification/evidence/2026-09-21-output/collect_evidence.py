import datetime
import hashlib
import json
import math
from pathlib import Path
import re
import shutil
import subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-output-20260921')
out = repo / 'tools/audio-qualification/evidence/2026-09-21-output'
out.mkdir(parents=True, exist_ok=True)
if json.loads((scratch / 'gate/gate.json').read_text())['status'] != 'passed':
    raise SystemExit('final gate has not passed')

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

for name in ('hardware-probe-1.json', 'hardware-probe-1.log', 'hardware-probe-2.json', 'hardware-probe-2.log', 'hardware-probe-3.json', 'hardware-probe-3.log', 'gate.py', 'collect_evidence.py'):
    shutil.copyfile(scratch / name, out / name)
for name in ('gate', 'gate-initial', 'gate-before-review-fixes'):
    if (scratch / name).exists():
        shutil.copytree(scratch / name, out / name, dirs_exist_ok=True)
for log in out.rglob('*.log'):
    # Preserve output content while avoiding empty trailing lines in Git diffs.
    raw = log.read_bytes()
    if raw:
        log.write_bytes(raw.rstrip(b'\n') + b'\n')

hardware = json.loads((scratch / 'hardware-before.json').read_text())['SPHardwareDataType'][0]
hardware = {k: v for k, v in hardware.items() if k in ('machine_model', 'machine_name', 'chip_type', 'physical_memory', 'number_processors')}
reports = []
for name in ('hardware-probe-1.json', 'hardware-probe-2.json', 'hardware-probe-3.json'):
    raw = json.loads((out / name).read_text())
    costs = sorted(r.get('render_cost_ns', r.get('callback_cost_ns')) for r in raw['records'])
    reports.append({'file': name, 'sha256': digest(out / name), 'status': raw['status'], 'checks': raw['checks'], 'callback_count': raw['callback_count'], 'device': raw['device'], 'tone': raw['tone'], 'render_cost_ns': {'p50_nearest_rank': costs[math.ceil(len(costs) * .5) - 1], 'p99_nearest_rank': costs[math.ceil(len(costs) * .99) - 1], 'maximum': max(costs)}, 'timestamp_latency_estimates_ns': sorted({r['playback_ns'] - r['callback_ns'] for r in raw['records']})})
tests = [tuple(map(int, match)) for match in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', (out / 'gate/test.log').read_text())]
sources = [repo / 'Cargo.toml', repo / 'Cargo.lock', repo / 'crates/deadpan-cli/src/doctor.rs', *sorted((repo / 'native/deadpan-output').rglob('*.rs')), repo / 'native/deadpan-output/Cargo.toml']
report = {
    'schema_version': 1,
    'collected_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    'hardware': hardware,
    'os': subprocess.check_output(['sw_vers'], text=True).strip(),
    'rustc': subprocess.check_output(['rustc', '-Vv'], cwd=repo, text=True).strip(),
    'reports': reports,
    'tests': dict(zip(('passed', 'failed', 'ignored'), map(sum, zip(*tests)))),
    'source_sha256': {str(p.relative_to(repo)): digest(p) for p in sources},
    'probe_binary_sha256': digest(repo / 'target/release/examples/qualify_output'),
    'probe_load_commands': subprocess.check_output(['otool', '-L', 'target/release/examples/qualify_output'], cwd=repo, text=True).strip(),
    'notes': [
        'Hardware probe 1 predates the pause-on-fault fix and stronger contiguous-coordinate assertions.',
        'Probe 1 callback_cost_ns measured the same partial body as the more accurately named render_cost_ns in later probes; neither includes telemetry publication or upstream/driver work.',
        'Probe 2 adds stronger sample continuity checks and corrected pause-on-fault behavior.',
        'Probe 3 additionally covers full stale queue resume and injected permanent fault visibility, silence and native callback stop. It uses the final production behavior.',
        'Serial numbers, hardware UUIDs and unrelated input-device inventory were omitted from public evidence.',
        'Ordinary CI never opens a device.',
    ],
}
(out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'evidence': str(out), 'tests': report['tests'], 'probes': [(r['status'], r['callback_count']) for r in reports]}, indent=2))
