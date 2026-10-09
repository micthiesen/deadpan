"""Repository-authored PCM/WAVE signals and scalar f32 references; no codec or resampling."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

ROOT = Path(__file__).parent / 'audio-fixtures'
COUNT = 8197


def chunk(tag, payload):
    return tag + struct.pack('<I', len(payload)) + payload + bytes(len(payload) % 2)


def signal(n, channel, bits, floating):
    if floating:
        # Quiet detail, signed zero, denormals and unclipped finite excursions.
        special = [-2.125, 1.75, -0.0, 0.0, 2**-149, -(2**-149), 2**-23, -(2**-23)]
        return special[(n + channel) % len(special)] if n < 32 else ((n * 37 + channel * 193) % 2001 - 1000) / 1024
    scale = 1 << (bits - 1)
    special = [-scale, scale - 1, 0, -1, 1, scale // 2, -scale // 2]
    return special[(n + channel) % len(special)] if n < 32 else (n * 104729 + channel * 15485863) % (2 * scale) - scale


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    # name, width, IEEE float, channels, rate, fmt size, speaker mask, fact
    cases = [
        ('u8-mono', 8, False, 1, 8000, 16, 0, False),
        ('u8-stereo', 8, False, 2, 44100, 18, 0, False),
        ('u8-extensible', 8, False, 1, 48000, 40, 4, False),
        ('s16-fmt18', 16, False, 2, 48000, 18, 0, True),
        ('s24-mono', 24, False, 1, 44100, 16, 0, False),
        ('s24-stereo', 24, False, 2, 96000, 40, 3, False),
        ('s24-surround', 24, False, 6, 48000, 40, 0x3f, False),
        ('s24-fmt18', 24, False, 2, 48000, 18, 0, True),
        ('s32-stereo', 32, False, 2, 48000, 16, 0, False),
        ('s32-mono', 32, False, 1, 192000, 40, 4, False),
        ('s32-384000', 32, False, 2, 384000, 40, 3, True),
        ('f32-stereo', 32, True, 2, 48000, 18, 0, True),
        ('f32-mono', 32, True, 1, 44100, 16, 0, False),
        ('f32-surround', 32, True, 6, 48000, 40, 0x3f, True),
    ]
    manifest = {'producer': __doc__, 'license': 'Repository-authored synthetic signals', 'files': []}
    for name, bits, floating, channels, rate, size, mask, fact in cases:
        data, reference = bytearray(), bytearray()
        for n in range(COUNT):
            for channel in range(channels):
                value = signal(n, channel, bits, floating)
                if floating:
                    encoded = struct.pack('<f', value)
                    expected = encoded
                else:
                    encoded = (value + 128).to_bytes(1) if bits == 8 else value.to_bytes(bits // 8, 'little', signed=True)
                    expected = struct.pack('<f', value / (1 << (bits - 1)))
                data.extend(encoded)
                reference.extend(expected)
        subtype = 3 if floating else 1
        align = channels * bits // 8
        fmt = struct.pack('<HHIIHH', 0xfffe if size == 40 else subtype, channels, rate, rate * align, align, bits)
        if size == 18:
            fmt += struct.pack('<H', 0)
        if size == 40:
            fmt += struct.pack('<HHIIHH8s', 22, bits, mask, subtype, 0, 0x10, bytes.fromhex('800000aa00389b71'))
        assert len(fmt) == size
        body = b'WAVE' + chunk(b'fmt ', fmt)
        if fact:
            body += chunk(b'fact', struct.pack('<I', COUNT))
        body += chunk(b'JUNK', b'Deadpan') + chunk(b'data', data)
        wav = b'RIFF' + struct.pack('<I', len(body)) + body
        row = dict(name='wide-' + name + '.wav', codec='pcm_f32le' if floating else 'pcm_u8' if bits == 8 else f'pcm_s{bits}le',
                   sample_rate=rate, channels=channels, samples=COUNT, bits=bits, fmt_size=size, mask=mask, fact=fact,
                   bytes=len(wav), sha256=hashlib.sha256(wav).hexdigest(), reference_sha256=hashlib.sha256(reference).hexdigest())
        for filename, content in [(row['name'], wav), (row['name'].replace('.wav', '.f32le'), reference)]:
            path = ROOT / filename
            if args.check:
                assert path.read_bytes() == content, filename
            else:
                path.write_bytes(content)
        manifest['files'].append(row)
    path = ROOT / 'wide-pcm-manifest.json'
    content = json.dumps(manifest, indent=2) + '\n'
    if args.check:
        assert path.read_text() == content
    else:
        path.write_text(content)
    print(content)


if __name__ == '__main__':
    main()
