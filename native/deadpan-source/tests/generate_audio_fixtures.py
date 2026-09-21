"""Generate deterministic PCM originals for native audio qualification.

Development-only Python standard library. These signals are synthetic and contain
no external media. Their full-scale peaks, quiet intervals, channel asymmetry and
non-block-aligned lengths expose normalization, channel and terminal errors.
"""

import argparse
import hashlib
import io
import json
from pathlib import Path
import struct
import wave


def stereo_sample(index):
    phase = index % 2048
    if phase == 0:
        left = 24576
    elif phase == 1:
        left = -24576
    elif 512 <= phase < 1024:
        left = (index * 97) % 16384 - 8192
    else:
        left = 0
    right = -32768 if index % 257 == 0 else 32767 if index % 257 == 1 else (index % 97 - 48) * 3
    return left, right


def mono_sample(index):
    return ((index * 73) % 65536 - 32768,)


def encode(rate, channels, frames, sample):
    pcm = b''.join(struct.pack('<' + 'h' * channels, *sample(index)) for index in range(frames))
    output = io.BytesIO()
    with wave.open(output, 'wb') as writer:
        writer.setnchannels(channels)
        writer.setsampwidth(2)
        writer.setframerate(rate)
        writer.writeframes(pcm)
    return output.getvalue(), pcm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=Path(__file__).parent / 'audio-fixtures')
    parser.add_argument('--verify', action='store_true', help='compare existing files without changing them')
    args = parser.parse_args()
    if not args.verify:
        args.output.mkdir(parents=True, exist_ok=True)
    files = []
    for name, rate, channels, frames, sample in [
        ('pcm-stereo-48000.wav', 48000, 2, 8197, stereo_sample),
        ('pcm-mono-44100.wav', 44100, 1, 44117, mono_sample),
    ]:
        encoded, pcm = encode(rate, channels, frames, sample)
        path = args.output / name
        if args.verify:
            assert path.read_bytes() == encoded, f'{path} differs from deterministic source'
        else:
            path.write_bytes(encoded)
        files.append({
            'name': name, 'bytes': len(encoded), 'sha256': hashlib.sha256(encoded).hexdigest(),
            'sample_rate': rate, 'channels': channels, 'sample_frames': frames,
            'sample_format': 'signed little-endian 16-bit PCM',
            'pcm_sha256': hashlib.sha256(pcm).hexdigest(),
            'expected_source_interval': {'start': 0, 'end': frames, 'time_base': [1, rate]},
            'first_four_sample_frames': [sample(index) for index in range(4)],
        })
    manifest = json.dumps({
        'producer': 'native/deadpan-source/tests/generate_audio_fixtures.py; Python standard-library wave/struct; no encoder or resampling',
        'license': 'Synthetic repository-authored signals under the repository license',
        'files': files,
    }, indent=2) + '\n'
    path = args.output / 'manifest.json'
    if args.verify:
        assert path.read_text() == manifest, 'audio fixture manifest differs'
    else:
        path.write_text(manifest)
    print(json.dumps({'files': len(files), 'verified': args.verify, 'output': str(args.output)}))


if __name__ == '__main__':
    main()
