"""Reproducible VP9 profile-2 PQ/HLG patches and static metadata.

Development-only FFmpeg/libvpx encodes independently authored ten-bit planes.
MP4 retains pinned mono AAC; WebM remuxes the identical picture packets.
--check compares bytes without writing the retained fixtures.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile
from generate_aperture_fixtures import boxes
from generate_sdr_hevc_fixtures import planes, ROOT


def add_metadata(data, metadata):
    containers = {b'moov': 0, b'trak': 0, b'mdia': 0, b'minf': 0,
                  b'stbl': 0, b'stsd': 8, b'vp09': 78}
    found, entries = [], []

    def walk(start, end, parents):
        for tag, at, stop in boxes(data, start, end):
            found.append((tag, at, stop))
            if tag == b'vp09':
                entries.append((stop, parents + [at]))
            if tag in containers:
                walk(at + 8 + containers[tag], stop, parents + [at])

    walk(0, len(data), [])
    assert len(entries) == 1
    insert, parents = entries[0]
    updated = bytearray(data)
    for parent in parents:
        struct.pack_into('>I', updated, parent, struct.unpack_from('>I', data, parent)[0] + len(metadata))
    for tag, at, stop in found:
        assert tag != b'co64'
        if tag == b'stco':
            count = struct.unpack_from('>I', data, at + 12)[0]
            assert stop == at + 16 + 4 * count
            for i in range(count):
                offset = at + 16 + 4 * i
                old = struct.unpack_from('>I', data, offset)[0]
                assert old >= insert
                struct.pack_into('>I', updated, offset, old + len(metadata))
    result = bytes(updated[:insert]) + metadata + bytes(updated[insert:])
    assert [data[a:b] for t, a, b in boxes(data, 0, len(data)) if t == b'mdat'] == [
        result[a:b] for t, a, b in boxes(result, 0, len(result)) if t == b'mdat']
    return result


def static_metadata(standard=False):
    # Exactly representable in both the VP9 binding and shared ST 2086 units.
    if standard:
        mastering = struct.pack('>8H2I', 12500, 34375, 6250, 3125, 34375, 15625,
                                15625, 15625, 10000000, 10000)
        light = struct.pack('>2H', 1000, 400)
        tags = (b'mdcv', b'clli')
    else:
        mastering = bytes(4) + struct.pack('>8H2I', 45056, 20480, 16384, 45056,
                                          8192, 4096, 20480, 20480, 256000, 16384)
        light = bytes(4) + struct.pack('>2H', 1000, 400)
        tags = (b'SmDm', b'CoLL')
    return b''.join(struct.pack('>I4s', len(body) + 8, tag) + body
                    for tag, body in zip(tags, [mastering, light]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    audio = ROOT / 'fields-bff.mp4'
    assert hashlib.sha256(audio.read_bytes()).hexdigest() == 'ae843cb5d745c6e7076c46c26404161858df5eedf1644082d88d8f9e11f613aa'
    with tempfile.TemporaryDirectory(prefix='deadpan-vp9-hdr-fixtures-') as temporary:
        directory = Path(temporary)
        raw = directory / 'input.yuv'
        raw.write_bytes(b''.join(planes(10, False, i) for i in range(12)))
        paths = []
        for label in ['pq', 'hlg', 'pq-topleft', 'hlg-anamorphic', 'pq-vfr']:
            output = directory / ('vp9-' + label + '.mp4')
            color = ['-color_range', 'tv', '-colorspace', 'bt2020nc', '-color_primaries', 'bt2020',
                     '-color_trc', 'smpte2084' if label.startswith('pq') else 'arib-std-b67',
                     '-chroma_sample_location', 'topleft' if label.endswith('topleft') else 'left']
            command = ['ffmpeg', '-nostdin', '-v', 'error', '-f', 'rawvideo', '-pixel_format', 'yuv420p10le',
                       '-video_size', '96x64', '-framerate', '30000/1001'] + color + [
                       '-i', str(raw), '-i', str(audio), '-map', '0:v', '-map', '1:a', '-c:a', 'copy',
                       '-c:v', 'libvpx-vp9', '-threads', '1', '-row-mt', '0', '-deadline', 'good',
                       '-cpu-used', '0', '-g', '4', '-lossless', '1', '-auto-alt-ref', '0', '-lag-in-frames', '0'] + color + [
                       '-fflags', '+bitexact', '-flags:v', '+bitexact', '-video_track_timescale', '60000',
                       '-movflags', '+faststart+write_colr', '-fps_mode', 'passthrough']
            if label.endswith('anamorphic'):
                command += ['-vf', 'setsar=3/2']
            if label.endswith('vfr'):
                command += ['-vf', 'select=not(eq(n\\,4)+eq(n\\,7))']
            subprocess.run(command + ['-y', str(output)], check=True)
            paths.append(output)
            if label == 'pq':
                for standard in [False, True]:
                    static = directory / ('vp9-pq-' + ('mdcv' if standard else 'static') + '.mp4')
                    static.write_bytes(add_metadata(output.read_bytes(), static_metadata(standard)))
                    paths.append(static)
        for source in list(paths):
            # The alternative MP4 metadata boxes mean the same thing; one WebM
            # static variant is sufficient to retain the floating-point binding.
            if source.stem == 'vp9-pq-mdcv':
                continue
            webm = source.with_suffix('.webm')
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-i', str(source), '-map', '0:v',
                            '-c', 'copy', '-chroma_sample_location', 'topleft' if 'topleft' in source.stem else 'left',
                            '-r', '30000/1001', '-fflags', '+bitexact', '-flags:v', '+bitexact',
                            '-y', str(webm)], check=True)
            paths.append(webm)
        for path in paths:
            data = path.read_bytes()
            if args.check:
                assert (ROOT / path.name).read_bytes() == data, path.name
            else:
                (ROOT / path.name).write_bytes(data)
            print(json.dumps(dict(name=path.name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest())), flush=True)


if __name__ == '__main__':
    main()
