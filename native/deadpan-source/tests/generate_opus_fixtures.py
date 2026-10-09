"""Deterministic Opus/WebM fixtures from pinned repository PCM and VP9.

Development FFmpeg/libopus encodes and independently decodes the reference PCM;
Deadpan uses its separately built, pinned FFmpeg/libopus decoder. --check is read-only.
"""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).parent


def run(args):
    subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-y', *args], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    pcm = ROOT / 'audio-fixtures/pcm-stereo-48000.wav'
    video = ROOT / 'fixtures/vp9-sdr-8-limited.mp4'
    assert hashlib.sha256(pcm.read_bytes()).hexdigest() == 'e79aba90eab42dc057171be52be9ecb794c3e2790c5731783270ece73c24d2ee'
    assert hashlib.sha256(video.read_bytes()).hexdigest() == next(
        f['sha256'] for f in json.loads((ROOT / 'fixtures/manifest.json').read_text())['files']
        if f['name'] == video.name)
    cases = [('stereo-20', 2, '20', None), ('mono-2.5', 1, '2.5', None),
             ('mono-60', 1, '60', None), ('stereo-120', 2, '120', None),
             ('av', 2, '20', '0'), ('av-offset', 2, '20', '0.125')]
    cases += [('av-audio-first', 2, '20', '0'), ('av-multi', 2, '20', '0')]
    cases += [('mono-silk', 1, '20', None), ('mono-hybrid', 1, '20', None)]
    cases += [('mono-preskip', 1, '2.5', None)]
    cases += [('mono-gain', 1, '20', None)]
    manifest = {'producer': __doc__, 'license': 'Repository-authored synthetic signals and pictures', 'files': []}
    with tempfile.TemporaryDirectory(prefix='deadpan-opus-') as tmp:
        for name, channels, duration, offset in cases:
            name = 'opus-' + name
            target = Path(tmp) / (name + '.webm')
            inputs = ['-i', str(pcm)] if offset is None else [
                '-i', str(video), '-itsoffset', offset, '-i', str(pcm)]
            maps = ['-map', '0:a:0'] if offset is None else [
                '-map', '0:v:0', '-map', '1:a:0', '-c:v', 'copy', '-chroma_sample_location', 'left']
            if name == 'opus-av-audio-first':
                maps = ['-map', '1:a:0', '-map', '0:v:0', '-c:v', 'copy', '-chroma_sample_location', 'left']
            if name == 'opus-av-multi':
                maps += ['-map', '1:a:0']
            codec = ['-b:a', '96k']
            if name.endswith('-silk'):
                codec = ['-b:a', '12k', '-application', 'voip', '-cutoff', '8000']
            if name.endswith('-hybrid'):
                codec = ['-b:a', '24k', '-application', 'voip', '-cutoff', '12000']
            run([*inputs, *maps, '-c:a', 'libopus', '-ac', str(channels), '-ar', '48000',
                 *codec, '-frame_duration', duration, '-threads', '1',
                 '-fflags', '+bitexact', '-map_metadata', '-1', str(target)])
            available_samples = 8197
            gain_reference = None
            if name == 'opus-mono-gain':
                gain_reference = Path(tmp) / 'ungained.f32le'
                run(['-c:a', 'libopus', '-request_sample_fmt', 'flt', '-i', str(target), '-map', '0:a:0', '-c:a', 'pcm_f32le', '-f', 'f32le', str(gain_reference)])
                data = bytearray(target.read_bytes())
                assert data.count(b'OpusHead') == 1
                at = data.index(b'OpusHead')
                assert data[at + 16:at + 18] == b'\0\0'
                data[at + 16:at + 18] = (512).to_bytes(2, 'little', signed=True)
                target.write_bytes(data)
            if name == 'opus-mono-preskip':
                data = target.read_bytes()
                before = b'OpusHead\x01\x01' + (120).to_bytes(2, 'little')
                after = b'OpusHead\x01\x01' + (600).to_bytes(2, 'little')
                delay = b'\x56\xaa\x83' + (2_500_000).to_bytes(3, 'big')
                changed_delay = b'\x56\xaa\x83' + (12_500_000).to_bytes(3, 'big')
                assert data.count(before) == data.count(delay) == 1
                target.write_bytes(data.replace(before, after).replace(delay, changed_delay))
                available_samples -= 480
            probe = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-select_streams', 'a:0', '-show_packets', '-show_data', '-of', 'json', str(target)]))
            configurations = sorted({int(re.search(r'00000000: ([0-9a-f]{2})', p['data'])[1], 16) >> 3 for p in probe['packets']})
            reference = Path(tmp) / (name + '.f32le')
            run(['-c:a', 'libopus', '-request_sample_fmt', 'flt', '-i', str(target), '-map', '0:a:0', '-c:a', 'pcm_f32le', '-f', 'f32le', str(reference)])
            if gain_reference is not None:
                expected = struct.unpack('<8197f', gain_reference.read_bytes())
                measured = struct.unpack('<8197f', reference.read_bytes())
                assert max(abs(a * 10 ** (2 / 20) - b) for a, b in zip(expected, measured)) < 0.000002
            for path in [target, reference]:
                data = path.read_bytes()
                destination = ROOT / 'audio-fixtures' / path.name
                if args.check:
                    assert destination.read_bytes() == data, path.name
                else:
                    destination.write_bytes(data)
                manifest['files'].append({'name': path.name, 'bytes': len(data),
                    'sha256': hashlib.sha256(data).hexdigest(), 'channels': channels,
                    'available_samples': available_samples, 'packet_duration_ms': duration, 'audio_offset': offset,
                    'opus_configurations': configurations,
                    'header_gain_q8_db': 512 if name == 'opus-mono-gain' else 0})
    encoded = json.dumps(manifest, indent=2) + '\n'
    path = ROOT / 'audio-fixtures/opus-manifest.json'
    if args.check:
        assert path.read_text() == encoded
    else:
        path.write_text(encoded)
    print(encoded)


if __name__ == '__main__':
    main()
