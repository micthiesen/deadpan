"""Add one clean-aperture box to pinned SDR and HDR fixtures, without encoding.

Only clap, enclosing box sizes and chunk offsets change. Compressed payload,
timing tables, audio and color declarations remain byte-identical. --check
reproduces and compares the committed files without writing.
"""

import argparse
import hashlib
import json
from pathlib import Path
import struct


ROOT = Path(__file__).parent / "fixtures"
CASES = [
    ("cfr-bframes.mp4", "aperture.mp4",
     "5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918",
     [300, 1, 160, 1, 3, 1, -1, 1]),  # left 13, top 9; odd chroma phase
    ("hevc-pq.mp4", "aperture-hdr.mp4",
     "d59a3ca17af6f88748c87aa72659e700971f4da42012020b1bb2ea82d062d7b0",
     [48, 1, 28, 1, -2, 1, -2, 1]),  # left 6, top 2; planar-aligned
    ("cfr-bframes.mp4", "aperture-fractional.mp4",
     "5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918",
     [599, 2, 319, 2, 3, 1, -1, 1]),  # [13.25,9.25,299.5,159.5]
    ("hevc-pq.mp4", "aperture-fractional-hdr.mp4",
     "d59a3ca17af6f88748c87aa72659e700971f4da42012020b1bb2ea82d062d7b0",
     [95, 2, 55, 2, -2, 1, -2, 1]),  # [6.25,2.25,47.5,27.5]
    ("../../../deadpan-media-worker/tests/fixtures/proxy/uhd-bt709.mp4", "aperture-fractional-uhd.mp4",
     "a04f59c16b66eaac02605093f5565efb0edaa762acdb0d8877c1afb5a891524b",
     [5601, 2, 3201, 2, 0, 1, 0, 1]),  # [519.75,279.75,2800.5,1600.5]
]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def boxes(data, start, end):
    found = []
    while start < end:
        require(end - start >= 8, "complete box header")
        size = struct.unpack_from(">I", data, start)[0]
        require(8 <= size <= end - start, "bounded ordinary box")
        found.append((data[start + 4:start + 8], start, start + size))
        start += size
    return found


def generate(data, words):
    containers = {b"moov": 0, b"trak": 0, b"mdia": 0, b"minf": 0,
                  b"stbl": 0, b"stsd": 8, b"avc1": 78, b"hvc1": 78}
    all_boxes = []
    video = []

    def walk(start, end, parents):
        for tag, at, stop in boxes(data, start, end):
            all_boxes.append((tag, at, stop))
            if tag in (b"avc1", b"hvc1"):
                video.append((at, stop, parents))
            if tag in containers:
                walk(at + 8 + containers[tag], stop, [*parents, at])

    walk(0, len(data), [])
    require(len(video) == 1, "one video sample entry")
    at, insert, parents = video[0]
    require(not any(tag == b"clap" for tag, _, _ in all_boxes), "no existing aperture")
    clap = struct.pack(">I4s8i", 40, b"clap", *words)
    updated = bytearray(data)
    changed = set()

    def replace(offset, value):
        struct.pack_into(">I", updated, offset, value)
        changed.update(range(offset, offset + 4))

    for parent in [*parents, at]:
        replace(parent, struct.unpack_from(">I", data, parent)[0] + len(clap))
    for tag, start, end in all_boxes:
        require(tag != b"co64", "fixture uses narrow chunk offsets")
        if tag == b"stco":
            count = struct.unpack_from(">I", data, start + 12)[0]
            require(end - start == 16 + count * 4, "complete chunk table")
            for i in range(count):
                position = start + 16 + i * 4
                old = struct.unpack_from(">I", data, position)[0]
                require(old >= insert, "fixture's media follows sample description")
                replace(position, old + len(clap))
    require(all(a == b or i in changed for i, (a, b) in enumerate(zip(data, updated))),
            "only parent sizes and chunk offsets changed")
    output = bytes(updated[:insert]) + clap + bytes(updated[insert:])
    old_mdat = [(a, b) for t, a, b in boxes(data, 0, len(data)) if t == b"mdat"]
    new_mdat = [(a, b) for t, a, b in boxes(output, 0, len(output)) if t == b"mdat"]
    require(len(old_mdat) == len(new_mdat) == 1, "one media payload")
    a, b = old_mdat[0]
    c, d = new_mdat[0]
    require(data[a:b] == output[c:d], "identical complete compressed payload")
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for source, destination, digest, words in CASES:
        data = (ROOT / source).read_bytes()
        require(hashlib.sha256(data).hexdigest() == digest, "pinned fixture hash")
        result = generate(data, words)
        target = ROOT / destination
        if args.check:
            require(target.read_bytes() == result, "checked-in fixture matches generator")
        else:
            target.write_bytes(result)
        print(json.dumps({"name": destination, "bytes": len(result),
                          "sha256": hashlib.sha256(result).hexdigest(),
                          "source_sha256": digest, "clap": words}))


if __name__ == "__main__":
    main()
