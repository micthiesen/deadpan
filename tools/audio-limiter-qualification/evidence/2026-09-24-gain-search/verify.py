"""Read-only verification of this retained research archive. No DSP is run."""
import ast
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import struct
import tarfile

ROOT = Path(__file__).resolve().parent
CEILING = 10 ** (-1 / 20)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def wave(data):
    fields = struct.unpack('<4sI4s4sIHHIIHH4sI', data[:44])
    assert fields == (b'RIFF', len(data) - 8, b'WAVE', b'fmt ', 16, 3, 2,
                      48000, 384000, 8, 32, b'data', len(data) - 44)
    assert (len(data) - 44) % 8 == 0
    samples = list(struct.iter_unpack('<ff', data[44:]))
    assert all(math.isfinite(value) and abs(value) <= 16
               for pair in samples for value in pair)
    return samples


def main():
    manifest = json.loads((ROOT / 'retention.json').read_text())
    path = ROOT / 'experiments.tar.gz'
    assert sha(path.read_bytes()) == manifest['archive_sha256']
    files = {}
    with tarfile.open(path, 'r:gz') as archive:
        for entry in archive:
            name = PurePosixPath(entry.name)
            assert entry.isfile() and not name.is_absolute() and '..' not in name.parts
            assert entry.name not in files and entry.name in manifest['files']
            expected = manifest['files'][entry.name]
            assert entry.size == expected['bytes']
            data = archive.extractfile(entry).read()
            assert sha(data) == expected['sha256'], entry.name
            if name.suffix == '.py':
                ast.parse(data, filename=entry.name)
            elif name.suffix == '.json':
                json.loads(data)
            elif name.suffix == '.wav':
                wave(data)
            files[entry.name] = data
    assert set(files) == set(manifest['files'])
    hashes = {sha(data) for data in files.values()}

    def report(name):
        return json.loads(files[name])

    for name, data in files.items():
        if name.endswith('/environment.json'):
            for key, value in json.loads(data).items():
                if key.endswith('_sha256'):
                    assert value in hashes, (name, key)

    joined = 0
    observations = {}
    for base in ['producer', 'producer-v2', 'producer-v3', 'producer-v4',
                 'producer-v4-full', 'producer-v5', 'producer-v5-full',
                 'finite-producer', 'finite-producer/wide-r4096',
                 'finite-producer/wide-r8192', 'finite-producer/wide-r8192-long']:
        results = report(base + '/results.json')['results']
        names = [row['name'] for row in results]
        assert len(names) == len(set(names)), base
        observations[base] = {}
        for row in results:
            if 'error' in row:
                assert not row['passes_both']
                continue
            name = row['name']
            source = files[f'{base}/inputs/{name}.wav']
            output = files[f'{base}/outputs/{name}.wav']
            assert sha(source) == row['input_sha256']
            assert sha(output) == row['output_sha256']
            assert report(f'{base}/reports/{name}.json') == row
            left, right = wave(source), wave(output)
            assert len(left) == len(right)
            assert all(y == 0 for a, b in zip(left, right) for x, y in zip(a, b) if x == 0)
            active_input = sum(any(value != 0 for value in pair) for pair in left)
            active_output = sum(any(value != 0 for value in pair) for pair in right)
            identical = source == output
            for key, actual in [('nonzero_input_frames', active_input),
                                ('nonzero_output_frames', active_output),
                                ('input_output_identical', identical),
                                ('byte_identical', identical)]:
                if key in row:
                    assert row[key] == actual, (base, name, key)
            observations[base][name] = (active_input, active_output, identical)
            if 'gain_knots' in row:
                assert len(left) == 8192
                knots = list(range(0, 8192, 64)) + [8191]
                controls = row['gain_knots']
                gain = []
                for n in range(8192):
                    at = min(n // 64, len(knots) - 2)
                    part = (n - knots[at]) / (knots[at + 1] - knots[at])
                    gain.append((1 - part) * controls[at] + part * controls[at + 1])
            else:
                gain = [value for value, in struct.iter_unpack('<d', files[f'{base}/outputs/{name}.gain.f64le'])]
            assert len(gain) == len(left) and all(0 <= value <= 1 for value in gain)
            rebuilt = b''.join(struct.pack('<ff', a * g, b * g) for (a, b), g in zip(left, gain))
            assert rebuilt == output[44:], (base, name)
            joined += 1

    for version in ['v3', 'v5']:
        producer = 'producer-v3' if version == 'v3' else 'producer-v5-full'
        summary = report(f'native-meter-{version}/summary.json')
        assert summary['meter_sha256'] == '97216bd091c1f54b769ccfa0c709590fd03ae7e095b94e7fd12a2dc3ba78eaae'
        assert len(summary['results']) == 16
        names = [row['name'] for row in summary['results']]
        assert len(names) == len(set(names)) and set(names) == set(observations[producer])
        for row in summary['results']:
            name = row['name']
            data = files[f'native-meter-{version}/{name}.json']
            assert sha(data) == row['report_sha256']
            meter = json.loads(data)
            assert sha(files[f'{producer}/outputs/{name}.wav'][44:]) == row['raw_sha256'] == meter['input_sha256']
            assert meter['peaks']['measured_frames'] == 8192
            assert max(meter['peaks']['true_peak']) <= CEILING

    # Make retention failures visible, not just generic hash successes.
    for base in ['producer', 'producer-v2']:
        failure = report(f'{base}/failure.json')
        assert failure['case'] == 'rapid-mask-16.0'
        assert 'budget' in failure['reason'] or 'exhausted' in failure['reason']
    v3 = observations['producer-v3']['near-nyquist']
    assert v3[0] - v3[1] == 1090
    v5 = report('producer-v5-full/results.json')['results']
    assert len(v5) == 16 and all(row['passes_both'] for row in v5)
    assert all(left == right for left, right, _ in observations['producer-v5-full'].values())
    assert sum(identical for _, _, identical in observations['producer-v5-full'].values()) == 8
    long = report('finite-producer/wide-r8192-long/reports/long-rapid-mask-16.json')
    assert long['passes_finite'] and long['passes_bs'] and not long['passes_sinc_diagnostic']
    assert max(abs(row['refined_signed_value']) for row in long['oracle']) > 1
    print(json.dumps({'archive_files': len(files), 'case_identity_and_gain_joins': joined,
                      'compiled_meter_joins': 32, 'v5_active_frames_preserved_cases': 16,
                      'v5_unchanged_controls': 8, 'negative_evidence_verified': True,
                      'scope': 'Artifact integrity and stored output relationships, not new limiter qualification'}, indent=2))


if __name__ == '__main__':
    main()
