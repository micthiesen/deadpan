"""Reproduce H.264 picture-timing SEI telecine fixtures with exact container time.

The retained TFF/BFF inputs keep every slice and audio packet byte. Only their
one-byte picture-timing payload and MP4 timing tables change. A progressive
input uses development libx264 with pic-struct=1 and the same authored bars.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

from generate_interlace_fixtures import frames
from generate_interlace_timing_fixture import generate

ROOT = Path(__file__).parent / 'fixtures'
MANIFEST = json.loads((ROOT / 'manifest.json').read_text())

def repeated(data, path, structs):
    packets = json.loads(subprocess.check_output([
        'ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_packets',
        '-show_entries', 'packet=pos,size', '-of', 'json', str(path)]))['packets']
    assert len(packets) == len(structs)
    output = bytearray(data)
    for packet, pic_struct in zip(packets, structs):
        at = int(packet['pos'])
        end = at + int(packet['size'])
        changed = 0
        while at < end:
            size = int.from_bytes(data[at:at+4], 'big')
            at += 4
            assert size > 0 and at + size <= end
            if data[at] & 31 == 6 and data[at:at+3] == bytes([6, 1, 1]):
                assert size == 5 and data[at+4] == 0x80
                assert data[at+3] in (0x04, 0x32, 0x42)
                # Zero clock_timestamp flags and a payload alignment stop bit.
                clocks = {0: 1, 3: 2, 4: 2, 5: 3, 6: 3, 7: 2, 8: 3}[pic_struct]
                output[at+3] = (pic_struct << 4) | (1 << (3 - clocks))
                changed += 1
            at += size
        assert at == end and changed == 1
    return bytes(output)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='deadpan-telecine-') as scratch:
        scratch = Path(scratch)
        raw = scratch / 'progressive.yuv'
        raw.write_bytes(b''.join(frames(12)))
        progressive = scratch / 'progressive.mp4'
        command = ['ffmpeg', '-nostdin', '-v', 'error', '-f', 'rawvideo',
            '-pixel_format', 'yuv420p', '-video_size', '96x64', '-framerate', '25',
            '-i', str(raw), '-i', str(ROOT / 'fields-tff.mp4'), '-map', '0:v', '-map', '1:a',
            '-c:v', 'libx264', '-qp', '12', '-flags:v', '+bitexact', '-fflags', '+bitexact',
            '-x264-params', 'pic-struct=1:keyint=3:min-keyint=3:scenecut=0:bframes=0:colorprim=bt709:transfer=bt709:colormatrix=bt709',
            '-color_range', 'tv', '-colorspace', 'bt709', '-color_trc', 'bt709',
            '-color_primaries', 'bt709', '-chroma_sample_location', 'left',
            '-video_track_timescale', '60000', '-c:a', 'copy', '-movflags', '+faststart',
            '-y', str(progressive)]
        subprocess.run(command, check=True)
        for name, source, structs, intervals, old_interval in [
            ('telecine-tff', ROOT/'fields-tff.mp4', [3,5]*6, [2002,3003]*6, 2400),
            ('telecine-bff', ROOT/'fields-bff.mp4', [4,6]*6, [2002,3003]*6, 2002),
            ('telecine-progressive', progressive, [3,5]*6, [2002,3003]*6, 2400),
            ('telecine-variable', ROOT/'fields-tff.mp4', [3,5]*6, [2003,3001]*6, 2400),
            ('telecine-single', ROOT/'fields-single.mp4', [5], [3003], 2400),
            ('telecine-bframes', ROOT/'fields-tff-bframes.mp4', [5]*12, None, 2400),
            ('progressive-repeats', progressive, [0,7,8]*4, [2400,4800,7200]*4, 2400),
        ]:
            original = source.read_bytes()
            if source.parent == ROOT:
                pin = next(row['sha256'] for row in MANIFEST['files'] if row['name'] == source.name)
                assert hashlib.sha256(original).hexdigest() == pin
            patched = repeated(original, source, structs)
            output = generate(patched, intervals, old_interval, hashlib.sha256(patched).hexdigest()) if intervals else patched
            destination = ROOT / (name + '.mp4')
            if args.check:
                assert destination.read_bytes() == output, name
            else:
                destination.write_bytes(output)
            print(json.dumps({'file': destination.name, 'bytes': len(output),
                              'sha256': hashlib.sha256(output).hexdigest(),
                              'pic_struct': structs, 'coded_intervals': intervals}))
        # Progressive picture timing with real HRD delay fields. This must pass
        # strict output decoding without any source-field reinterpretation.
        hrd = scratch / 'progressive-timing-hrd.mp4'
        index = command.index('-qp')
        command[index:index+2] = ['-b:v', '1000k', '-maxrate', '1000k', '-bufsize', '1000k']
        index = command.index('-x264-params') + 1
        command[index] += ':nal-hrd=cbr'
        command[-1] = str(hrd)
        subprocess.run(command, check=True)
        output = hrd.read_bytes()
        destination = ROOT / hrd.name
        if args.check:
            assert destination.read_bytes() == output, hrd.name
        else:
            destination.write_bytes(output)
        print(json.dumps({'file': hrd.name, 'bytes': len(output),
                          'sha256': hashlib.sha256(output).hexdigest()}))

if __name__ == '__main__':
    main()
