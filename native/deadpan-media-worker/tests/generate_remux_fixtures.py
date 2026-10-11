"""Add empty AV1 temporal delimiters to authored MP4 pictures, without encoding.

Reproduces the packet framing observed in YouTube format 399. All coded OBUs,
timing tables, decoder configuration and picture metadata remain identical.
The only external tool is a development ffprobe used to list sample extents.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess

ROOT = Path(__file__).parent
SOURCE = ROOT.parent.parent / 'deadpan-source/tests/fixtures/av1-sdr-8-limited.mp4'


def boxes(data):
    at = 0
    while at < len(data):
        size, tag = struct.unpack_from('>I4s', data, at)
        assert size >= 8 and at + size <= len(data)
        yield tag, data[at + 8:at + size]
        at += size


def box(tag, body):
    return struct.pack('>I4s', len(body) + 8, tag) + body


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    original = SOURCE.read_bytes()
    probe = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-select_streams', 'v:0',
        '-show_packets', '-of', 'json', str(SOURCE)]))
    packets = [original[int(p['pos']):int(p['pos']) + int(p['size'])] for p in probe['packets']]
    assert len(packets) == 12 and all(not p.startswith(b'\x12\0') for p in packets)
    samples = [b'\x12\0' + p for p in packets]
    top = dict(boxes(original))
    ftyp = box(b'ftyp', top[b'ftyp'])
    def rewrite(data):
        result = bytearray()
        for tag, body in boxes(data):
            if tag == b'trak' and b'soun' in body:
                continue
            if tag in [b'moov', b'trak', b'mdia', b'minf', b'stbl']:
                body = rewrite(body)
            elif tag == b'stco':
                body = struct.pack('>III', 0, 1, len(ftyp) + 8)
            elif tag == b'stsc':
                body = struct.pack('>IIIII', 0, 1, 1, len(samples), 1)
            elif tag == b'stsz':
                body = struct.pack('>III', 0, 0, len(samples)) + b''.join(struct.pack('>I', len(p)) for p in samples)
            result.extend(box(tag, body))
        return bytes(result)
    result = ftyp + box(b'mdat', b''.join(samples)) + box(b'moov', rewrite(top[b'moov']))
    manifest = dict(source=str(SOURCE.relative_to(ROOT.parent.parent)), source_sha256=hashlib.sha256(original).hexdigest(),
        fixture_sha256=hashlib.sha256(result).hexdigest(), packets=len(samples), bytes=len(result),
        license='Repository-authored synthetic pictures', operation='Prefix each unchanged coded temporal unit with 12 00')
    for name, data in [('av1-temporal-delimiters.mp4', result),
                       ('av1-temporal-delimiters.json', (json.dumps(manifest, indent=2)+'\n').encode())]:
        path = ROOT/'fixtures/remux'/name
        if args.check:
            assert path.read_bytes() == data, name
        else:
            path.write_bytes(data)
    print(json.dumps(manifest, indent=2))


if __name__ == '__main__':
    main()
