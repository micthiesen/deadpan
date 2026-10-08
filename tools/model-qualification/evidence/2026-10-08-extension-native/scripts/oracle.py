"""Independent integer pixel oracle over the retained normal-CLI masters."""
from fractions import Fraction
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import numpy as np

parser = argparse.ArgumentParser()
parser.add_argument('--ffmpeg', required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parent
width, height = 768, 320

def object_path(case, reference):
    content = reference['content']
    assert content['algorithm'] == 'blake3'
    assert len(content['digest']) == 64
    path = case / 'project.deadpan/Media/Generated' / ('blake3-' + content['digest'])
    assert path.is_file() and path.stat().st_size == reference['byte_length']
    return path

def decode(path, count):
    process = subprocess.run([args.ffmpeg, '-v', 'error', '-nostdin', '-i', str(path),
        '-map', '0:v:0', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, timeout=120, check=True)
    assert len(process.stdout) == count * width * height * 3
    return np.frombuffer(process.stdout, dtype=np.uint8).reshape(count, height, width, 3)

reports = []
for item in json.loads((root / 'results.json').read_text()):
    if not item['ready']:
        continue
    case = root / item['case']
    record = json.loads((case / 'generated.json').read_text())
    plan = record['plan']['plan']['sampling']
    k, e, n = [plan[field] for field in ('context_frame_count', 'generated_frame_count', 'output_frame_count')]
    native = decode(object_path(case, record['ready']['native']), k + e)
    sampled = decode(object_path(case, record['ready']['sampled']), n)
    start = k if plan['direction'] == 'from_left' else 0
    positions = []
    for j in range(n):
        position = start + min(max(Fraction((2*j+1)*e-n, 2*n), 0), e-1)
        lower = position.numerator // position.denominator
        upper = -(-position.numerator // position.denominator)
        assert start <= lower <= upper < start+e, 'conditioning picture entered the sampled interval'
        weight = position-lower
        a, b = weight.numerator, weight.denominator
        numerator = (b-a)*native[lower].astype(np.uint32) + a*native[upper].astype(np.uint32)
        expected = ((2*numerator+b)//(2*b)).astype(np.uint8)
        assert np.array_equal(sampled[j], expected), (case.name, j)
        positions.append({'numerator': position.numerator, 'denominator': position.denominator,
                          'lower': lower, 'upper': upper})
    report = {'case': case.name, 'native_frames': k+e, 'generated_frames': e, 'output_frames': n,
              'rgb_channel_comparisons': int(sampled.size), 'context_fetches': 0,
              'sampled_rgb_sha256': hashlib.sha256(sampled.tobytes()).hexdigest(),
              'native_rgb_sha256': hashlib.sha256(native.tobytes()).hexdigest(),
              'positions': positions}
    with (case / 'oracle.json').open('x') as output:
        json.dump(report, output, indent=2)
        output.write('\n')
    reports.append({key: value for key, value in report.items() if key != 'positions'})
    print(json.dumps(reports[-1]), flush=True)
with (root / 'oracle-results.json').open('x') as output:
    json.dump(reports, output, indent=2)
    output.write('\n')
