"""MP4 Opus from repository PCM, with exact edits and independent WebM PCM references.

Development FFmpeg/libopus encodes the same packet payloads as the retained
Opus/WebM corpus. The private decoder uses the pinned qualified FFmpeg build.
No timestamps are inferred from signal matching. --check is read-only.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).parent


def boxes(data):
    at = 0
    while at < len(data):
        size, tag = struct.unpack_from('>I4s', data, at)
        assert size >= 8 and at + size <= len(data)
        yield tag, data[at + 8:at + size]
        at += size
    assert at == len(data)


def box(tag, data):
    return struct.pack('>I4s', len(data) + 8, tag) + data


def edit(data, *, preskip=None, gain=None, offset=0):
    # All generated files put moov last, so its growth never moves mdat offsets.
    top = list(boxes(data))
    assert top[-1][0] == b'moov'
    movie = top[-1][1]
    mvhd = next(body for tag, body in boxes(movie) if tag == b'mvhd')
    scale = int.from_bytes(mvhd[12:16], 'big')
    def recurse(data, audio=False):
        result = bytearray()
        for tag, body in boxes(data):
            b = bytearray(body)
            if tag == b'trak':
                b = recurse(body, b'soun' in body)
            elif tag in [b'moov', b'mdia', b'minf', b'stbl', b'edts']:
                b = recurse(body, audio)
            elif tag == b'stsd' and audio:
                b = body[:8] + recurse(body[8:], audio)
            elif tag == b'Opus':
                b = body[:28] + recurse(body[28:], audio)
            elif tag == b'dOps':
                if preskip is not None:
                    b[2:4] = preskip.to_bytes(2, 'big')
                if gain is not None:
                    b[8:10] = gain.to_bytes(2, 'big', signed=True)
            elif tag == b'elst' and audio:
                assert body[:8] == struct.pack('>II', 0, 1)
                duration, skip, rate = struct.unpack('>IiI', body[8:])
                if preskip is not None:
                    duration -= (preskip - skip) * scale // 48000
                    skip = preskip
                rows = [(duration, skip, rate)]
                if offset:
                    rows.insert(0, (offset * scale // 48000, -1, 65536))
                b = struct.pack('>II', 0, len(rows)) + b''.join(struct.pack('>IiI', *r) for r in rows)
            elif tag == b'tkhd' and audio:
                duration = int.from_bytes(b[20:24], 'big')
                if preskip is not None:
                    duration -= (preskip - 120) * scale // 48000
                duration += offset * scale // 48000
                b[20:24] = duration.to_bytes(4, 'big')
            elif tag == b'mvhd' and preskip is not None:
                duration = int.from_bytes(b[16:20], 'big') - (preskip - 120) * scale // 48000
                b[16:20] = duration.to_bytes(4, 'big')
            result.extend(box(tag, b))
        return bytes(result)
    return b''.join(box(tag, recurse(body) if tag == b'moov' else body) for tag, body in top)


def packets(path):
    result = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-select_streams',
                        'a:0', '-show_packets', '-show_data_hash', 'sha256', '-of', 'json', str(path)]))
    return [p['data_hash'] for p in result['packets']]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    cases = [('stereo-20', 2, '20'), ('mono-2.5', 1, '2.5'), ('mono-60', 1, '60'),
             ('stereo-120', 2, '120'), ('mono-silk', 1, '20'), ('mono-hybrid', 1, '20'),
             ('mono-gain', 1, '20'), ('mono-preskip', 1, '2.5')]
    cases += [('av-h264', 2, '20'), ('av-av1', 2, '20'), ('av-vp9-offset', 2, '20')]
    manifest = {'producer': __doc__, 'license': 'Repository-authored synthetic signals and pictures', 'files': []}
    with tempfile.TemporaryDirectory(prefix='deadpan-opus-mp4-') as temp:
        for name, channels, duration in cases:
            target = Path(temp) / f'opus-mp4-{name}.mp4'
            inputs = ['-i', str(ROOT / 'audio-fixtures/pcm-stereo-48000.wav')]
            maps = ['-map', '0:a:0']
            video = {'av-h264': 'cfr-bframes.mp4', 'av-av1': 'av1-sdr-8-limited.mp4',
                     'av-vp9-offset': 'vp9-pq.mp4'}.get(name)
            if video:
                inputs += ['-i', str(ROOT / 'fixtures' / video)]
                maps = ['-map', '1:v:0', '-map', '0:a:0', '-c:v', 'copy', '-chroma_sample_location', 'left']
            codec = ['-b:a', '96k']
            if name.endswith('silk'):
                codec = ['-b:a', '12k', '-application', 'voip', '-cutoff', '8000']
            if name.endswith('hybrid'):
                codec = ['-b:a', '24k', '-application', 'voip', '-cutoff', '12000']
            command = ['ffmpeg', '-nostdin', '-v', 'error', '-y', *inputs, *maps,
                       '-c:a', 'libopus', '-ac', str(channels), '-ar', '48000', *codec,
                       '-frame_duration', duration, '-threads', '1', '-fflags', '+bitexact',
                       '-map_metadata', '-1', '-movie_timescale', '240000' if video else '48000', str(target)]
            subprocess.run(command, check=True)
            target.write_bytes(edit(target.read_bytes(), preskip=600 if name == 'mono-preskip' else None,
                                   gain=512 if name == 'mono-gain' else None,
                                   offset=6000 if name == 'av-vp9-offset' else 0))
            reference = 'stereo-20' if video else name
            assert packets(target) == packets(ROOT / f'audio-fixtures/opus-{reference}.webm'), name
            data = target.read_bytes()
            destination = ROOT / 'audio-fixtures' / target.name
            if args.check:
                assert destination.read_bytes() == data, name
            else:
                destination.write_bytes(data)
            manifest['files'].append({'name': target.name, 'bytes': len(data),
                'sha256': hashlib.sha256(data).hexdigest(), 'channels': channels,
                'reference': f'opus-{reference}.f32le', 'valid_samples': 7717 if name == 'mono-preskip' else 8197,
                'offset_samples': 6000 if name == 'av-vp9-offset' else 0,
                'audio_stream': 1 if video else 0, 'video': video, 'packet_duration_ms': duration})
    encoded = json.dumps(manifest, indent=2) + '\n'
    path = ROOT / 'audio-fixtures/opus-mp4-manifest.json'
    if args.check:
        assert path.read_text() == encoded
    else:
        path.write_text(encoded)
    print(f'{len(cases)} exact packet-copy-equivalent fixtures verified')


if __name__ == '__main__':
    main()
