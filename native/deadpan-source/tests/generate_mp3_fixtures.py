"""Reproducible synthetic MP3 signals, independent development decoder PCM, and framing evidence."""
import argparse
import binascii
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import zlib

from generate_audio_fixtures import encode, mono_sample, stereo_sample

ROOT = Path(__file__).parent / 'audio-fixtures'


def run(args):
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL)


def syncsize(size):
    return bytes((size >> shift) & 127 for shift in (21, 14, 7, 0))


def tagframe(name, data, version):
    return name + (syncsize(len(data)) if version == 4 else struct.pack('>I', len(data))) + b'\0\0' + data


def png():
    def chunk(name, data):
        return struct.pack('>I', len(data)) + name + data + struct.pack('>I', binascii.crc32(name + data))
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 2, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress((b'\0' + b'\xf0\x10\x40' * 2) * 2)) + chunk(b'IEND', b'')


def crc16(data):
    crc = 0
    for b in data:
        crc ^= b
        for _ in range(8):
            crc = (crc >> 1) ^ (0xa001 if crc & 1 else 0)
    return crc


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    cases = [
        ('stereo-cbr', 48000, 2, ['-b:a', '192k']),
        ('mono-vbr', 44100, 1, ['-q:a', '2']),
        ('stereo-32000', 32000, 2, ['-q:a', '3']),
        ('mono-mpeg2', 24000, 1, ['-b:a', '64k']),
        ('stereo-22050', 22050, 2, ['-q:a', '2']),
        ('mono-16000', 16000, 1, ['-b:a', '32k']),
        ('stereo-12000', 12000, 2, ['-b:a', '32k']),
        ('mono-11025', 11025, 1, ['-b:a', '24k']),
        ('mono-mpeg25', 8000, 1, ['-b:a', '16k']),
        ('stereo-untagged', 48000, 2, ['-b:a', '192k', '-write_xing', '0']),
        ('mono-short', 24000, 1, ['-b:a', '64k']),
        ('stereo-crc', 48000, 2, []),
        ('stereo-id3v3', 48000, 2, ['-b:a', '192k']),
        ('stereo-id3v4', 48000, 2, ['-b:a', '192k']),
        ('stereo-spanning', 48000, 2, ['-b:a', '192k']),
        ('stereo-unknown', 48000, 2, ['-b:a', '192k']),
    ]
    manifest = {'producer': __doc__, 'license': 'Repository-authored synthetic signals and cover picture',
                'ffmpeg': subprocess.check_output(['ffmpeg', '-version'], text=True).splitlines()[0],
                'lame': subprocess.check_output(['lame', '--version'], text=True).splitlines()[0], 'files': []}
    with tempfile.TemporaryDirectory(prefix='deadpan-mp3-fixtures-') as tmp:
        for name, rate, channels, codec in cases:
            source = Path(tmp) / 'source.wav'
            count = 97 if name == 'mono-short' else 8197
            wav, _ = encode(rate, channels, count, stereo_sample if channels == 2 else mono_sample)
            source.write_bytes(wav)
            target = Path(tmp) / ('mp3-' + name + '.mp3')
            if name == 'stereo-crc':
                run(['lame', '--silent', '-p', '-b', '192', str(source), str(target)])
            else:
                run(['ffmpeg', '-nostdin', '-v', 'error', '-y', '-i', str(source), '-c:a', 'libmp3lame',
                     *codec, '-id3v2_version', '0', '-threads', '1', '-map_metadata', '-1', '-fflags', '+bitexact', str(target)])
            if name.startswith('stereo-id3'):
                version = int(name[-1])
                title = tagframe(b'TIT2', b'\0Deadpan fixture', version)
                art = tagframe(b'APIC', b'\0image/png\0\x03\0' + png(), version)
                body = title + art + b'\0' * 8
                target.write_bytes(b'ID3' + bytes([version, 0, 0]) + syncsize(len(body)) + body + target.read_bytes() + b'TAG' + bytes(125))
            if name == 'stereo-spanning':
                data = bytearray(target.read_bytes())
                xing = data.index(b'Info')
                encoder = xing + 120
                assert data[encoder:encoder + 4] == b'Lavf'
                data[encoder + 21:encoder + 24] = ((2500 << 12) | 3000).to_bytes(3, 'big')
                data[encoder + 34:encoder + 36] = crc16(data[:encoder + 34]).to_bytes(2, 'big')
                target.write_bytes(data)
            if name == 'stereo-unknown':
                data = bytearray(target.read_bytes())
                encoder = data.index(b'Info') + 120
                data[encoder:encoder + 9] = b'Unknown00'
                target.write_bytes(data)
            reference = target.with_suffix('.f32le')
            run(['ffmpeg', '-nostdin', '-v', 'error', '-y', '-c:a', 'mp3float', '-i', str(target), '-map', '0:a:0', '-f', 'f32le', str(reference)])
            probe = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-flags2', '+skip_manual', '-select_streams', 'a:0', '-show_frames', '-of', 'json', str(target)]))
            frames = probe['frames']
            available = reference.stat().st_size // (4 * channels)
            if not any(kind in name for kind in ['untagged', 'spanning', 'unknown']):
                assert available == count, (name, available)
            for path in [target, reference]:
                data = path.read_bytes()
                destination = ROOT / path.name
                if args.check:
                    assert destination.read_bytes() == data, path.name
                else:
                    destination.write_bytes(data)
                manifest['files'].append({'name': path.name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(),
                    'sample_rate': rate, 'channels': channels, 'available_samples': available,
                    'physical_frames': len(frames), 'samples_per_frame': frames[0]['nb_samples'],
                    'first_frame': frames[0], 'last_frame': frames[-1], 'source_sha256': hashlib.sha256(wav).hexdigest()})
    data = json.dumps(manifest, indent=2) + '\n'
    destination = ROOT / 'mp3-manifest.json'
    if args.check:
        assert destination.read_text() == data
    else:
        destination.write_text(data)
    print(data)


if __name__ == '__main__':
    main()
