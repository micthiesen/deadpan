"""Reproduce vfr-long-terminal.mp4 without an encoder or media subprocess.

The retained VideoToolbox fixture encodes a 1001-tick final picture, although
media_probe.c authored 3003 ticks. Keep that historical file unchanged. This
variant restores only the final video duration in stts, mdhd, tkhd and elst.
Every compressed byte, sample PTS, audio byte and movie duration stays intact.

Run this script to generate the variant, or pass --check to verify checked-in
bytes. Parsing is deliberately limited to this hash-pinned, non-fragmented MP4.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct


INPUT_SHA256 = "3fd07d00aa604554dc5389530cd261ac8961aded6361426fb00bf5519ac64884"
INPUT_BYTES = 43_455
ROOT = Path(__file__).parent / "fixtures"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def u32(data: bytes, offset: int) -> int:
    require(0 <= offset <= len(data) - 4, "bounded u32 field")
    return struct.unpack_from(">I", data, offset)[0]


def boxes(data: bytes, start: int, end: int) -> list[tuple[bytes, int, int]]:
    result = []
    require(0 <= start <= end <= len(data), "bounded container")
    while start < end:
        require(len(result) < 128 and end - start >= 8, "bounded box header")
        size = u32(data, start)
        require(8 <= size <= end - start, "ordinary complete fixture box")
        result.append((data[start + 4:start + 8], start + 8, start + size))
        start += size
    return result


def one(data: bytes, parent: tuple[int, int], kind: bytes) -> tuple[int, int]:
    matches = [(start, end) for name, start, end in boxes(data, *parent) if name == kind]
    require(len(matches) == 1, f"exactly one {kind!r} box")
    return matches[0]


def full_v0(data: bytes, box: tuple[int, int], size: int) -> int:
    start, end = box
    require(end - start == size and data[start:start + 4] == bytes(4),
            "exact version-zero full-box extent and flags")
    return start


def inspect(data: bytes, final_duration: int) -> dict:
    require(len(data) == INPUT_BYTES, "exact fixture size")
    moov = one(data, (0, len(data)), b"moov")
    mdat = one(data, (0, len(data)), b"mdat")
    movie = full_v0(data, one(data, moov, b"mvhd"), 100)
    require((u32(data, movie + 12), u32(data, movie + 16)) == (240_000, 1_921_920),
            "unchanged movie clock and audio-sized duration")
    tracks = {}
    for kind, start, end in boxes(data, *moov):
        if kind != b"trak":
            continue
        media = one(data, (start, end), b"mdia")
        handler, handler_end = one(data, media, b"hdlr")
        require(handler_end - handler >= 24, "complete media handler")
        label = data[handler + 8:handler + 12]
        require(label not in tracks, "one track per media handler")
        tracks[label] = ((start, end), media)
    require(set(tracks) == {b"vide", b"soun"}, "exact video/audio track pair")
    track, media = tracks[b"vide"]
    header = one(data, track, b"tkhd")
    require(header[1] - header[0] == 84 and data[header[0]] == 0,
            "version-zero video track header")
    media_header = full_v0(data, one(data, media, b"mdhd"), 24)
    require(u32(data, media_header + 12) == 30_000, "original video clock")
    edits = one(data, track, b"edts")
    edit = full_v0(data, one(data, edits, b"elst"), 20)
    require(u32(data, edit + 4) == 1 and u32(data, edit + 12) == 0
            and u32(data, edit + 16) == 65_536, "one unshifted unity video edit")
    sample_table = one(data, one(data, media, b"minf"), b"stbl")
    require(not any(kind == b"ctts" for kind, _, _ in boxes(data, *sample_table)),
            "no composition offsets: presentation order equals sample order")
    timing = full_v0(data, one(data, sample_table, b"stts"), 8 + 120 * 8)
    require(u32(data, timing + 4) == 120, "120 individual duration runs")
    durations = []
    for ordinal in range(120):
        count, duration = struct.unpack_from(">II", data, timing + 8 + ordinal * 8)
        expected = final_duration if ordinal == 119 else 1001 * (1 + ordinal % 3)
        require(count == 1 and duration == expected, f"sample {ordinal} duration")
        durations.append(duration)
    end = sum(durations)
    require(end == 237_237 + final_duration, "exact last-picture PTS and terminal")
    require(u32(data, media_header + 16) == end, "mdhd agrees with sample table")
    require(u32(data, header[0] + 20) == end * 8, "tkhd agrees with video duration")
    require(u32(data, edit + 8) == end * 8, "elst agrees with video duration")
    pts = []
    at = 0
    for duration in durations:
        pts.append(at)
        at += duration
    return {
        "anchors": (timing + 12 + 119 * 8, media_header + 16, header[0] + 20, edit + 8),
        "pts": pts,
        "mdat": mdat,
        "audio_track": tracks[b"soun"][0],
        "terminal": end,
    }


def generate(original: bytes) -> bytes:
    require(hashlib.sha256(original).hexdigest() == INPUT_SHA256, "pinned input SHA-256")
    before = inspect(original, 1001)
    offsets = before["anchors"]
    # Exact structural anchors protect against accidentally patching a similar
    # value elsewhere, independently of the whole-file digest above.
    require(offsets == (1602, 316, 184, 272), "pinned timing field offsets")
    replacements = ((1001, 3003), (238_238, 240_240),
                    (1_905_904, 1_921_920), (1_905_904, 1_921_920))
    modified = bytearray(original)
    for offset, (old, new) in zip(offsets, replacements, strict=True):
        require(u32(original, offset) == old, "exact original field value")
        struct.pack_into(">I", modified, offset, new)
    output = bytes(modified)
    after = inspect(output, 3003)
    require(before["pts"] == after["pts"], "every picture PTS is unchanged")
    for section in ("mdat", "audio_track"):
        start, end = before[section]
        require(before[section] == after[section] and original[start:end] == output[start:end],
                f"unchanged {section}")
    allowed = {byte for offset in offsets for byte in range(offset, offset + 4)}
    require(all(left == right or index in allowed
                for index, (left, right) in enumerate(zip(original, output, strict=True))),
            "only four video timing fields change")
    return output


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify existing output without writing")
    args = parser.parse_args()
    source = ROOT / "vfr.mp4"
    destination = ROOT / "vfr-long-terminal.mp4"
    require(source.stat().st_size == INPUT_BYTES, "bounded original fixture")
    output = generate(source.read_bytes())
    if args.check:
        require(destination.stat().st_size == INPUT_BYTES, "bounded generated fixture")
        require(destination.read_bytes() == output, "checked-in fixture matches generator")
    else:
        destination.write_bytes(output)
    print(json.dumps({"name": destination.name, "bytes": len(output),
                      "sha256": hashlib.sha256(output).hexdigest(),
                      "input_sha256": INPUT_SHA256, "terminal_ticks": 240_240,
                      "final_duration_ticks": 3003, "time_base": [1, 30_000]}))


if __name__ == "__main__":
    main()
