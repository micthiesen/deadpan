"""Retain all compressed field-motion/audio samples, vary only video timing.

Odd coded-picture intervals require exact half ticks after deinterlacing. The
last interval differs from the preceding one so extrapolated EOF timing fails.
Reproduce fields-variable.mp4; --check verifies without writing.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct

from generate_vfr_terminal_fixture import boxes, one, require, u32

ROOT = Path(__file__).parent / 'fixtures'
INTERVALS = [2401, 4801, 2401, 2401, 7201, 2401, 2401, 4801, 2401, 2401, 2401, 6001]
CONTAINERS = {b'moov', b'trak', b'mdia', b'minf', b'stbl', b'edts'}

def generate(data):
    require(hashlib.sha256(data).hexdigest() ==
            'b51ac86c5cb35827c34dfc90b37aec11379cc808efc3637b68f6d8940807e42b', 'pinned input')
    moov = one(data, (0, len(data)), b'moov')
    mdat = one(data, (0, len(data)), b'mdat')
    require(moov[1] < mdat[0], 'faststart source')
    delta = (len(INTERVALS) - 1) * 8
    terminal = sum(INTERVALS)
    movie_terminal = (terminal * 4 + 4) // 5

    def rewrite(start, end, video=False):
        output = bytearray()
        for kind, at, limit in boxes(data, start, end):
            body = bytearray(data[at:limit])
            is_video = video
            if kind == b'trak':
                media = one(data, (at, limit), b'mdia')
                handler, _ = one(data, media, b'hdlr')
                is_video = data[handler + 8:handler + 12] == b'vide'
            if kind in CONTAINERS:
                body = rewrite(at, limit, is_video)
            elif kind == b'stts' and video:
                require(body == struct.pack('>IIII', 0, 1, 12, 2400), 'original CFR runs')
                body = bytearray(struct.pack('>II', 0, len(INTERVALS)))
                for duration in INTERVALS:
                    body.extend(struct.pack('>II', 1, duration))
            elif kind == b'stco':
                count = u32(body, 4)
                require(len(body) == 8 + 4 * count and count < 128, 'bounded chunk table')
                for index in range(count):
                    offset = u32(body, 8 + index * 4)
                    require(mdat[0] <= offset < mdat[1], 'chunk inside retained payload')
                    struct.pack_into('>I', body, 8 + index * 4, offset + delta)
            elif kind == b'mvhd':
                require(u32(body, 0) == 0 and u32(body, 12) == 48000, 'movie clock')
                struct.pack_into('>I', body, 16, movie_terminal)
            elif video and kind == b'mdhd':
                require(u32(body, 0) == 0 and u32(body, 12) == 60000, 'video clock')
                struct.pack_into('>I', body, 16, terminal)
            elif video and kind == b'tkhd':
                require(body[0] == 0, 'version zero track')
                struct.pack_into('>I', body, 20, movie_terminal)
            elif video and kind == b'elst':
                require(len(body) == 20 and u32(body, 4) == 1 and u32(body, 12) == 0, 'unshifted video edit')
                struct.pack_into('>I', body, 8, movie_terminal)
            output.extend(struct.pack('>I4s', len(body) + 8, kind))
            output.extend(body)
        return output

    output = bytes(rewrite(0, len(data)))
    require(len(output) == len(data) + delta, 'only timing table grows')
    out_mdat = one(output, (0, len(output)), b'mdat')
    require(output[slice(*out_mdat)] == data[slice(*mdat)], 'unchanged compressed video and audio')
    return output

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = generate((ROOT / 'fields-tff.mp4').read_bytes())
    destination = ROOT / 'fields-variable.mp4'
    if args.check:
        require(destination.read_bytes() == output, 'checked-in fixture')
    else:
        destination.write_bytes(output)
    print(json.dumps({'name':destination.name, 'bytes':len(output),
                      'sha256':hashlib.sha256(output).hexdigest(),
                      'coded_intervals':INTERVALS, 'coded_time_base':[1,60000]}))

if __name__ == '__main__':
    main()
