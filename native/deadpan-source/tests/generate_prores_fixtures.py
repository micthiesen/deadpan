"""Deterministic ProRes 422 Proxy/LT/Standard/HQ QuickTime fixtures.

Each has 12 authored 96x64 ten-bit 422 pictures at 30000/1001 and the retained
mono AAC signal. Extra HQ clips exercise nonsquare pixels, VFR and both field
orders. Raw decoder outputs are retained as independent converter references;
tests also compare authored flat patches and individual moving field rows.
Only the development generator invokes FFmpeg. --check never changes fixtures.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).parent / 'fixtures'


def planes(frame, interlaced=False, width=96, height=64):
    # Flat upper-half patches preserve low bits; a moving lower-half pattern
    # and independently varying chroma rows expose mistaken 4:2:0 conversion.
    y = [([64, 324 + frame % 4, 480 + frame % 4, 940][min(3, x // 24)]
          if row < 32 else 160 + (x * 7 + row * 3 + frame * 29) % 650)
         for row in range(height) for x in range(width)]
    if interlaced:
        y = [200 + 16 * frame + (160 if row % 2 else 0)
             for row in range(height) for _ in range(width)]
    uv = [(420 if c == 0 else 580) if row < 32 and 24 <= x < 36 else
          512 if row < 32 or interlaced else 400 + (row * 19 + x * 5 + frame * 7 + c * 71) % 224
          for c in range(2) for row in range(height) for x in range(width // 2)]
    values = y + uv
    return struct.pack('<' + 'H' * len(values), *values)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--apple', action='store_true', help='generate only the host-dependent VideoToolbox HQ fixture')
    args = parser.parse_args()
    audio = ROOT / 'fields-bff.mp4'
    assert hashlib.sha256(audio.read_bytes()).hexdigest() == 'ae843cb5d745c6e7076c46c26404161858df5eedf1644082d88d8f9e11f613aa'
    cases = [(name, profile, '') for profile, name in enumerate(['proxy', 'lt', 'standard', 'hq'])]
    cases += [('anamorphic', 3, 'setsar=4/3'), ('vfr', 3, 'select=not(eq(n\\,4)+eq(n\\,7))'),
              ('tff', 3, 'setfield=tff'), ('bff', 3, 'setfield=bff'), ('edges', 3, '')]
    if args.apple:
        cases = [('apple-hq', 3, '')]
    with tempfile.TemporaryDirectory(prefix='deadpan-prores-') as temporary:
        directory = Path(temporary)
        for label, profile, filters in cases:
            name = 'prores-' + label
            width, height = (98, 66) if label == 'edges' else (96, 64)
            raw = directory / 'input.yuv'
            raw.write_bytes(b''.join(planes(i, label in ['tff', 'bff'], width, height) for i in range(12)))
            output = directory / (name + '.mov')
            command = ['ffmpeg', '-nostdin', '-v', 'error', '-f', 'rawvideo',
                       '-pixel_format', 'yuv422p10le', '-video_size', f'{width}x{height}',
                       '-framerate', '30000/1001', '-color_range', 'tv',
                       '-colorspace', 'bt709', '-color_trc', 'bt709', '-color_primaries', 'bt709',
                       '-i', str(raw), '-i', str(audio),
                       '-map', '0:v', '-map', '1:a', '-c:v', 'prores_videotoolbox' if args.apple else 'prores_ks', '-profile:v', str(profile),
                       '-threads:v', '1', '-c:a', 'copy', '-color_range', 'tv',
                       '-colorspace', 'bt709', '-color_trc', 'bt709', '-color_primaries', 'bt709',
                       '-video_track_timescale', '60000', '-fflags', '+bitexact',
                       '-flags:v', '+bitexact' + ('+ildct' if label in ['tff', 'bff'] else ''),
                       '-movflags', '+faststart' + ('' if label == 'hq' else '+write_colr'),
                       '-fps_mode', 'passthrough']
            if filters:
                command += ['-vf', filters]
            if args.apple:
                command += ['-allow_sw', '1', '-pix_fmt', 'p210le']
            subprocess.run(command + ['-y', str(output)], check=True)
            reference = directory / (name + '.yuv422p10le')
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-threads', '1',
                            '-i', str(output), '-map', '0:v', '-c:v', 'rawvideo',
                            '-pixel_format', 'yuv422p10le', '-f', 'rawvideo',
                            '-fps_mode', 'passthrough', '-y', str(reference)], check=True)
            for path in [output, reference]:
                data = path.read_bytes()
                destination = ROOT / path.name
                if args.check:
                    assert destination.read_bytes() == data, path.name
                else:
                    destination.write_bytes(data)
                print(json.dumps({'name': path.name, 'bytes': len(data),
                                  'sha256': hashlib.sha256(data).hexdigest()}))


if __name__ == '__main__':
    main()
