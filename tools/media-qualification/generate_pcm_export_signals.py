"""Write nonperiodic PCM timing probes using the qualified stress fixtures' WAVE headers.

Synthetic repository-authored signals; Python standard library only. Keep the
native stress fixtures unchanged: their complete integer ranges and repeated
patterns prove sample conversion, but cannot always prove encoded timing.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / 'native/deadpan-source/tests/audio-fixtures'
NAMES = ['u8-mono', 's24-stereo', 's24-surround', 's32-mono', 's32-384000', 'f32-stereo']


def knot(index, channel):
    # Defined unsigned integer mixing, independent of Python's salted hash.
    value = ((index + 1) * 0x9e3779b9 + channel * 0x85ebca6b) & 0xffffffff
    value = ((value ^ (value >> 16)) * 0x7feb352d) & 0xffffffff
    value = ((value ^ (value >> 15)) * 0x846ca68b) & 0xffffffff
    return ((value ^ (value >> 16)) / 0xffffffff - 0.5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if not args.check:
        args.output.mkdir(parents=True, exist_ok=True)
        if any(args.output.iterdir()):
            parser.error('output must be empty')
    rows = json.loads((FIXTURES / 'wide-pcm-manifest.json').read_text())['files']
    result = dict(producer=__doc__, script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), files=[])
    for name in NAMES:
        row = next(r for r in rows if r['name'] == f'wide-{name}.wav')
        wav = bytearray((FIXTURES / row['name']).read_bytes())
        cursor = 12
        while wav[cursor:cursor + 4] != b'data':
            size = int.from_bytes(wav[cursor + 4:cursor + 8], 'little')
            cursor += 8 + size + size % 2
        start = cursor + 8
        size = int.from_bytes(wav[cursor + 4:cursor + 8], 'little')
        samples, reference = bytearray(), bytearray()
        bits, rate = row['bits'], row['sample_rate']
        for n in range(row['samples']):
            cell, remainder = divmod(n * 431, rate)
            fraction = remainder / rate
            for channel in range(row['channels']):
                a, b = knot(cell, channel), knot(cell + 1, channel)
                # Interpolation keeps the energy low enough in frequency to
                # survive the existing resampler and lossy export path while
                # retaining a distinct, nonrepeating timing signature.
                amplitude = a + fraction * (b - a)
                if row['codec'] == 'pcm_f32le':
                    encoded = struct.pack('<f', amplitude)
                    expected = encoded
                else:
                    value = round(amplitude * (1 << (bits - 1)))
                    encoded = (value + 128).to_bytes(1, 'little') if bits == 8 else value.to_bytes(bits // 8, 'little', signed=True)
                    expected = struct.pack('<f', value / (1 << (bits - 1)))
                samples.extend(encoded)
                reference.extend(expected)
        assert len(samples) == size
        wav[start:start + size] = samples
        target = f'events-{name}.wav'
        for filename, content in [(target, wav), (target.replace('.wav', '.f32le'), reference)]:
            path = args.output / filename
            if args.check:
                assert path.read_bytes() == content, filename
            else:
                path.write_bytes(content)
        result['files'].append(dict(row, name=target, header_source=row['name'],
            sha256=hashlib.sha256(wav).hexdigest(), reference_sha256=hashlib.sha256(reference).hexdigest()))
    text = json.dumps(result, indent=2) + '\n'
    path = args.output / 'manifest.json'
    if args.check:
        assert path.read_text() == text
    else:
        path.write_text(text)
    print(json.dumps(dict(files=len(result['files']), checked=args.check, output=str(args.output))))


if __name__ == '__main__':
    main()
