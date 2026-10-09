"""Deterministic HEVC Main/Main10 and H.264 High10 SDR inputs.

96x64, 12 pictures at 30000/1001, 4:2:0, IDR every four pictures,
two B frames, BT.709 and explicit left chroma siting. Upper-half patches are
black, gray, colored and white; lower-half motion retains ten-bit low bits.
The pinned fields-bff.mp4 supplies unchanged mono AAC through sample 19219.
HEVC is lossless; H.264 High10 uses QP 1. Development libx265/libx264 are not
linked into Deadpan. --check reproduces the retained bytes without writes.
"""
from pathlib import Path
import argparse
import hashlib
import json
import struct
import subprocess
import tempfile

ROOT = Path(__file__).parent / 'fixtures'

def planes(bits, full, frame):
    scale = 1 << (bits - 8)
    black, white = (0, (1 << bits) - 1) if full else (16 * scale, 235 * scale)
    patches = [black, 81 * scale + frame % 4, 120 * scale + frame % 4, white]
    result = [patches[x // 24] if y < 32 else
              (40 + (2*x + y + 7*frame) % 180) * scale + frame % scale
              for y in range(64) for x in range(96)]
    for component in range(2):
        result.extend((105 if component == 0 else 145) * scale
                      if y < 16 and 24 <= x < 36 else 128 * scale
                      for y in range(32) for x in range(48))
    return bytes(result) if bits == 8 else struct.pack('<' + 'H' * len(result), *result)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    audio = ROOT / 'fields-bff.mp4'
    assert hashlib.sha256(audio.read_bytes()).hexdigest() == 'ae843cb5d745c6e7076c46c26404161858df5eedf1644082d88d8f9e11f613aa'
    with tempfile.TemporaryDirectory(prefix='deadpan-sdr-hevc-') as directory:
        directory = Path(directory)
        for codec, bits in [('hevc', 8), ('hevc', 10), ('h264', 10)]:
            for full in [False, True]:
                name = f'{codec}-sdr-{bits}-' + ('full' if full else 'limited') + '.mp4'
                raw = directory / 'input.yuv'
                raw.write_bytes(b''.join(planes(bits, full, frame) for frame in range(12)))
                output = directory / name
                parameters = ('pools=none:frame-threads=1:lookahead-threads=0:info=0:log-level=error:'
                    'repeat-headers=0:lossless=1:keyint=4:min-keyint=4:scenecut=0:'
                    'open-gop=0:bframes=2:chromaloc=0:colorprim=bt709:transfer=bt709:'
                    'colormatrix=bt709:range=' + ('full' if full else 'limited'))
                encoder = ['-c:v', 'libx265', '-x265-params', parameters, '-tag:v', 'hvc1']
                if codec == 'h264':
                    encoder = ['-c:v', 'libx264', '-profile:v', 'high10', '-qp', '1',
                        '-x264-params', 'keyint=4:min-keyint=4:scenecut=0:bframes=2:colorprim=bt709:transfer=bt709:colormatrix=bt709']
                command = ['ffmpeg', '-nostdin', '-v', 'error', '-f', 'rawvideo',
                    '-pixel_format', 'yuv420p' if bits == 8 else 'yuv420p10le',
                    '-video_size', '96x64', '-framerate', '30000/1001',
                    '-color_range', 'pc' if full else 'tv', '-colorspace', 'bt709',
                    '-color_trc', 'bt709', '-color_primaries', 'bt709',
                    '-chroma_sample_location', 'left', '-i', str(raw),
                    '-i', str(audio), '-map', '0:v', '-map', '1:a', *encoder,
                    '-color_range', 'pc' if full else 'tv',
                    '-colorspace', 'bt709', '-color_trc', 'bt709', '-color_primaries', 'bt709',
                    '-chroma_sample_location', 'left', '-video_track_timescale', '60000',
                    '-c:a', 'copy', '-flags:v', '+bitexact', '-fflags', '+bitexact',
                    '-movflags', '+faststart', '-y', str(output)]
                subprocess.run(command, check=True)
                data = output.read_bytes()
                destination = ROOT / name
                if args.check:
                    assert destination.read_bytes() == data, name
                else:
                    destination.write_bytes(data)
                print(json.dumps({'name': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}))

if __name__ == '__main__':
    main()
