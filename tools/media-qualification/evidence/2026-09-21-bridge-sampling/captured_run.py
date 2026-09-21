import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--worker', type=Path, required=True)
parser.add_argument('--label', required=True)
args = parser.parse_args()
repo = Path('/Users/michael/Code/deadpan')
work = Path(tempfile.mkdtemp(prefix=f'deadpan-bridge-{args.label}-', dir='/tmp'))
source = Path('/tmp/deadpan-supervised-mlx-20260921-attributed/worker/outputs/native.mp4')
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
worker = work / 'deadpan-media-worker'
host = work / 'convert_generated'
shutil.copy2(args.worker, worker)
shutil.copy2(repo / 'target/debug/examples/convert_generated', host)
request = {
    'protocol': 2, 'operation': 'sample_bridge',
    'native': {'width': 768, 'height': 320, 'frames': 25, 'rate_num': 24, 'rate_den': 1},
    'sampling': {'schema_version': 1, 'project_rate': {'numerator': 30000, 'denominator': 1001},
                 'native_rate': {'numerator': 24, 'denominator': 1},
                 'native_frame_count': 25, 'output_frame_count': 30,
                 'interpolation': 'encoded_srgb_rgb8_linear_half_up'},
    'input_byte_length': source.stat().st_size,
    'limits': {'max_input_bytes': 32 * 1024 * 1024, 'max_output_bytes': 32 * 1024 * 1024,
               'max_scratch_bytes': 768 * 320 * 3 * 25, 'timeout_ms': 120000},
}
request_path = work / 'request.json'
request_path.write_text(json.dumps(request) + '\n')
command = ['convert_generated', str(worker), str(source), str(request_path), sha(source),
           str(work / 'sampled.mkv'), str(work / 'native.mkv')]
sources = ['crates/deadpan-media/src/conversion.rs', 'crates/deadpan-media/src/protocol.rs',
           'crates/deadpan-media/src/lib.rs', 'crates/deadpan-media/examples/convert_generated.rs',
           'native/deadpan-media-worker/src/converter.c', 'native/deadpan-media-worker/src/converter.h',
           'native/deadpan-media-worker/src/main.rs', 'native/deadpan-media-worker/build.rs', 'Cargo.lock']
report = {
    'scope': 'captured native model sequence to canonical native and host-sampled FFV1 pair; no candidate admission',
    'label': args.label, 'work': str(work), 'command': command,
    'host': platform.platform(), 'machine': platform.machine(),
    'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
    'sources_sha256': {p: sha(repo / p) for p in sources},
    'worker_sha256': sha(worker), 'host_sha256': sha(host), 'source_sha256': sha(source),
    'request': request,
    'libraries_sha256': {str(p): sha(p) for p in Path('/tmp/deadpan-media-compatible-xyhilms4/prefix/lib').glob('*.dylib')},
    'expected_native_rgb_sha256': 'be4ffce12b048457bcab1779eefe8683f41c242d013be7f488c25416491efe1a',
    'expected_sampled_rgb_sha256': 'eda362044e57353435b56756be4143419a3bb6ed52bb2f849102b3eff962c71a',
}
start = time.monotonic()
env = dict(os.environ, PATH=str(work) + os.pathsep + os.environ['PATH'])
result = subprocess.run(command, cwd=repo, env=env, capture_output=True, timeout=150)
(work / 'stdout.json').write_bytes(result.stdout)
(work / 'stderr.log').write_bytes(result.stderr)
report.update(exit_code=result.returncode, elapsed_seconds=time.monotonic()-start)
try:
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors='replace'))
    value = json.loads(result.stdout)
    report['conversion'] = value
    assert value['native']['report']['input_rgb_sha256'] == report['expected_native_rgb_sha256']
    assert value['native']['report']['output_rgb_sha256'] == report['expected_native_rgb_sha256']
    assert value['report']['input_rgb_sha256'] == report['expected_native_rgb_sha256']
    assert value['report']['output_rgb_sha256'] == report['expected_sampled_rgb_sha256']
    report['files'] = {p.name: {'bytes': p.stat().st_size, 'sha256': sha(p)} for p in [work/'native.mkv', work/'sampled.mkv']}
    report['status'] = 'passed'
except Exception as error:
    report.update(status='failed', failure=str(error))
(work / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'work': str(work), 'status': report['status'], 'seconds': report['elapsed_seconds'],
                  'failure': report.get('failure')}, indent=2), flush=True)
raise SystemExit(0 if report['status'] == 'passed' else 1)
