"""Synthetic AV1 Main 8/10-bit fixtures, with retained planar references.

Development only: FFmpeg/libsvtav1 encodes our authored patches and motion;
libdav1d decodes the reference planes. Tests independently convert these planes
and check authored patches, clocks, thread determinism and reverse seeks.
--check reproduces the bytes without changing the retained fixtures.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile
from generate_aperture_fixtures import boxes

ROOT = Path(__file__).parent / 'fixtures'


def rewrite_configuration(data, transform):
    """Rewrite only av1C, ancestor lengths and following chunk locations."""
    containers = {b'moov': 0, b'trak': 0, b'mdia': 0, b'minf': 0,
                  b'stbl': 0, b'stsd': 8, b'av01': 78}
    found, configs = [], []

    def walk(start, end, parents):
        for tag, at, stop in boxes(data, start, end):
            found.append((tag, at, stop))
            if tag == b'av1C':
                configs.append((at, stop, parents))
            if tag in containers:
                walk(at + 8 + containers[tag], stop, parents + [at])

    walk(0, len(data), [])
    assert len(configs) == 1
    at, stop, parents = configs[0]
    replacement = transform(data[at + 8:stop])
    delta = len(replacement) - (stop - at - 8)
    updated = bytearray(data)
    for parent in parents + [at]:
        struct.pack_into('>I', updated, parent, struct.unpack_from('>I', data, parent)[0] + delta)
    for tag, pos, end in found:
        assert tag != b'co64'
        if tag == b'stco':
            count = struct.unpack_from('>I', data, pos + 12)[0]
            assert end == pos + 16 + 4 * count
            for i in range(count):
                loc = pos + 16 + i * 4
                old = struct.unpack_from('>I', data, loc)[0]
                assert old >= stop
                struct.pack_into('>I', updated, loc, old + delta)
    result = bytes(updated[:at + 8]) + replacement + bytes(updated[stop:])
    before = [data[a:b] for t, a, b in boxes(data, 0, len(data)) if t == b'mdat']
    after = [result[a:b] for t, a, b in boxes(result, 0, len(result)) if t == b'mdat']
    assert before == after
    return result


def static_hdr():
    # AV1's rational units: chromaticity /65536, maximum /256, minimum /16384.
    # These values are exactly representable in the shared ST 2086 contract.
    mastering = struct.pack('>8H2I', 45056, 20480, 16384, 45056, 8192, 4096,
                            20480, 20480, 256000, 16384)
    return b'\x2a\x06\x01' + struct.pack('>2H', 1000, 400) + b'\x80' + b'\x2a\x1a\x02' + mastering + b'\x80'


def planes(frame, depth, full, noise=False):
    scale = 1 << (depth - 8)
    low, high = (0, (1 << depth) - 1) if full else (16 * scale, 235 * scale)
    levels = [low, 81 * scale + frame % 4, 120 * scale + frame % 4, high]
    y = [levels[x // 32] if row < 48 else
         (40 + (x * 2 + row + frame * 7) % 180) * scale
         for row in range(96) for x in range(128)]
    if noise:
        # Deterministic authored spatial/temporal noise for SVT grain estimation.
        y = [max(low, min(high, value + (((at * 214013 + frame * 2531011) >> 5) % 17 - 8) * scale))
             for at, value in enumerate(y)]
    uv = [(105 if plane == 0 else 145) * scale if row < 24 and 32 <= x < 48 else
          128 * scale if row < 24 else
          (100 + (row * 19 + x * 5 + frame * 7 + plane * 17) % 56) * scale
          for plane in range(2) for row in range(48) for x in range(64)]
    values = y + uv
    return bytes(values) if depth == 8 else struct.pack('<' + 'H' * len(values), *values)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--only', help='generate one named case')
    args = parser.parse_args()
    cases = [(f'sdr-{depth}-{r}', depth, r == 'full', '', '', 12)
             for depth in [8, 10] for r in ['limited', 'full']]
    cases += [('anamorphic', 10, False, 'setsar=3/2', '', 12),
              ('topleft', 10, False, '', '', 12),
              ('vfr', 10, False, 'select=not(eq(n\\,4)+eq(n\\,7))', '', 12),
              ('multigop', 10, False, '', '', 36),
              ('grain', 10, False, '', ':film-grain=20', 36),
              ('superres', 10, False, '', ':superres-mode=1:superres-denom=12:superres-kf-denom=12', 12),
              ('pq', 10, False, '', '', 12), ('hlg', 10, False, '', '', 12)]
    if args.only:
        cases = [case for case in cases if case[0] == args.only]
        if not cases:
            parser.error('unknown case')
    audio = ROOT / 'fields-bff.mp4'
    assert hashlib.sha256(audio.read_bytes()).hexdigest() == 'ae843cb5d745c6e7076c46c26404161858df5eedf1644082d88d8f9e11f613aa'
    with tempfile.TemporaryDirectory(prefix='deadpan-av1-fixtures-') as temporary:
        directory = Path(temporary)
        for label, depth, full, filters, options, count in cases:
            name = 'av1-' + label
            pixel = 'yuv420p' if depth == 8 else 'yuv420p10le'
            raw = directory / 'input.yuv'
            raw.write_bytes(b''.join(planes(i, depth, full, label == 'grain') for i in range(count)))
            output = directory / (name + '.mp4')
            hdr = label in ['pq', 'hlg']
            color = ['-color_range', 'pc' if full else 'tv', '-colorspace', 'bt2020nc' if hdr else 'bt709',
                     '-color_trc', 'smpte2084' if label == 'pq' else 'arib-std-b67' if hdr else 'bt709',
                     '-color_primaries', 'bt2020' if hdr else 'bt709', '-chroma_sample_location', 'topleft' if label == 'topleft' else 'left']
            command = ['ffmpeg', '-nostdin', '-v', 'error', '-f', 'rawvideo', '-pixel_format', pixel,
                       '-video_size', '128x96', '-framerate', '30000/1001'] + color + [
                       '-i', str(raw), '-i', str(audio), '-map', '0:v', '-map', '1:a',
                       '-c:v', 'libsvtav1', '-preset', '8', '-crf', '12', '-g', '16',
                       '-svtav1-params', 'lp=1:film-grain=0' + options, '-c:a', 'copy'] + color + [
                       '-fflags', '+bitexact', '-flags:v', '+bitexact', '-video_track_timescale', '60000',
                       '-movflags', '+faststart+write_colr', '-fps_mode', 'passthrough']
            if filters:
                command += ['-vf', filters]
            subprocess.run(command + ['-y', str(output)], check=True)
            webm = directory / (name + '.webm')
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-i', str(output), '-map', '0:v',
                            '-c', 'copy', '-r', '30000/1001', '-fflags', '+bitexact', '-flags:v', '+bitexact',
                            '-y', str(webm)], check=True)
            reference = directory / (name + '.' + pixel)
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-c:v', 'libdav1d', '-threads', '1',
                            '-i', str(output), '-map', '0:v', '-c:v', 'rawvideo', '-pix_fmt', pixel,
                            '-f', 'rawvideo', '-fps_mode', 'passthrough', '-y', str(reference)], check=True)
            paths = [output, webm, reference]
            if label == 'sdr-10-limited':
                bootstrap = directory / 'av1-no-config-sequence.mp4'
                bootstrap.write_bytes(rewrite_configuration(output.read_bytes(), lambda config: config[:4]))
                paths.append(bootstrap)
            if label == 'pq':
                static = directory / 'av1-pq-static-config.mp4'
                static.write_bytes(rewrite_configuration(output.read_bytes(), lambda config: config + static_hdr()))
                paths.append(static)
                # Keep the original encoded picture bytes in an IVF wrapper,
                # adding static metadata after each key's sequence header.
                movie = output.read_bytes()
                packets = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-select_streams', 'v',
                    '-show_packets', '-of', 'json', str(output)], text=True))['packets']
                ivf = struct.pack('<4sHH4sHHIIII', b'DKIF', 0, 32, b'AV01', 128, 96, 30000, 1001, len(packets), 0)
                for ordinal, packet in enumerate(packets):
                    start, size = int(packet['pos']), int(packet['size'])
                    payload = movie[start:start + size]
                    if 'K' in packet['flags']:
                        assert payload[:2] == b'\x0a\x0e'
                        payload = payload[:16] + static_hdr() + payload[16:]
                    ivf += struct.pack('<IQ', len(payload), ordinal) + payload
                ivf_path = directory / 'static.ivf'
                ivf_path.write_bytes(ivf)
                packet_static = directory / 'av1-pq-static-packet.mp4'
                subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-i', str(ivf_path), '-c', 'copy',
                    '-fflags', '+bitexact', '-flags:v', '+bitexact', '-movflags', '+faststart+write_colr',
                    '-video_track_timescale', '60000', '-y', str(packet_static)], check=True)
                paths.append(packet_static)
            for path in paths:
                data = path.read_bytes()
                destination = ROOT / path.name
                if args.check:
                    assert destination.read_bytes() == data, path.name
                else:
                    destination.write_bytes(data)
                print(json.dumps({'name': path.name, 'bytes': len(data),
                                  'sha256': hashlib.sha256(data).hexdigest()}), flush=True)


if __name__ == '__main__':
    main()
