#!/usr/bin/env python3
"""Generate the small, developer-only real-media converter fixtures."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


WIDTH = 4
HEIGHT = 2


def rgb_frames(frame_count: int) -> bytes:
    output = bytearray()
    for frame in range(frame_count):
        for y in range(HEIGHT):
            for x in range(WIDTH):
                output.extend(
                    (
                        (17 * frame + 31 * x + 7 * y + 3) % 256,
                        (29 * frame + 5 * x + 47 * y + 11) % 256,
                        (43 * frame + 13 * x + 19 * y + 23) % 256,
                    )
                )
    return bytes(output)


def run_encoder(
    ffmpeg: Path,
    destination: Path,
    raw: Path,
    rate_num: int,
    rate_den: int,
    tagged: bool,
    audio: bool,
) -> None:
    command = [
        str(ffmpeg),
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-video_size",
        f"{WIDTH}x{HEIGHT}",
        "-framerate",
        f"{rate_num}/{rate_den}",
        "-i",
        str(raw),
    ]
    if audio:
        command.extend(
            ["-f", "lavfi", "-i", "sine=frequency=880:sample_rate=8000:duration=0.125"]
        )
    command.extend(["-map", "0:v:0"])
    if audio:
        command.extend(["-map", "1:a:0"])
    command.extend(
        [
            "-vf",
            (
                "setparams=range=full:color_primaries=bt709:"
                "color_trc=iec61966-2-1:colorspace=gbr"
                if tagged
                else "format=rgb24"
            ),
            "-c:v",
            "libx264rgb",
            "-crf",
            "0",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "rgb24",
            "-movflags",
            "+write_colr",
            "-video_track_timescale",
            str(rate_num),
        ]
    )
    if tagged:
        command.extend(
            [
                "-color_range",
                "pc",
                "-colorspace",
                "rgb",
                "-color_trc",
                "iec61966-2-1",
                "-color_primaries",
                "bt709",
            ]
        )
    if audio:
        # Keep the tiny audio stream long enough to survive AAC priming. The
        # raw video naturally ends after its requested frames.
        command.extend(["-c:a", "aac", "-b:a", "16k", "-t", "0.125"])
    else:
        command.append("-an")
    command.append(str(destination))
    subprocess.run(command, check=True)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(64 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def fixture(
    ffmpeg: Path,
    output: Path,
    name: str,
    frames: int,
    rate_num: int,
    rate_den: int,
    *,
    tagged: bool = True,
    audio: bool = False,
) -> dict[str, object]:
    raw = rgb_frames(frames)
    with tempfile.NamedTemporaryFile(prefix="deadpan-rgb-", suffix=".raw") as source:
        source.write(raw)
        source.flush()
        destination = output / f"{name}.mp4"
        run_encoder(ffmpeg, destination, Path(source.name), rate_num, rate_den, tagged, audio)
    return {
        "name": name,
        "file": destination.name,
        "width": WIDTH,
        "height": HEIGHT,
        "frames": frames,
        "rate_num": rate_num,
        "rate_den": rate_den,
        "rgb_sha256": hashlib.sha256(raw).hexdigest(),
        "file_sha256": sha256(destination),
        "file_bytes": destination.stat().st_size,
        "has_audio": audio,
        "tags": tagged,
        "raw_bytes": len(raw),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ffmpeg", type=Path, default=Path("ffmpeg"))
    parser.add_argument("--output", type=Path, default=Path(__file__).parent / "fixtures")
    args = parser.parse_args()
    if not args.ffmpeg.is_absolute() and shutil.which(str(args.ffmpeg)) is None:
        raise SystemExit(f"ffmpeg executable was not found: {args.ffmpeg}")
    args.output.mkdir(parents=True, exist_ok=True)
    version = subprocess.run(
        [str(args.ffmpeg), "-version"], check=True, capture_output=True, text=True
    ).stdout.splitlines()[0]
    entries = [
        fixture(args.ffmpeg, args.output, "rgb30_30000_1001", 30, 30000, 1001),
        fixture(args.ffmpeg, args.output, "rgb25_24", 25, 24, 1),
        fixture(args.ffmpeg, args.output, "rgb1_24", 1, 24, 1),
        fixture(args.ffmpeg, args.output, "rgb2_24_audio", 2, 24, 1, audio=True),
        fixture(args.ffmpeg, args.output, "rgb1_24_no_tags", 1, 24, 1, tagged=False),
    ]
    valid = args.output / "rgb1_24.mp4"
    valid_bytes = valid.read_bytes()
    # The first video packet starts at byte 48. Corrupt a run inside its H.264
    # slice payload, rather than changing only a container metadata atom.
    corruption_offset = 1000
    corruption_length = 20
    if corruption_offset + corruption_length >= len(valid_bytes):
        raise SystemExit("fixture is unexpectedly too small for the corruption probe")
    corrupt = args.output / "rgb1_24_corrupt.mp4"
    corrupted = bytearray(valid_bytes)
    corrupted[corruption_offset : corruption_offset + corruption_length] = b"\0" * corruption_length
    corrupt.write_bytes(corrupted)
    entries.append(
        {
            "name": "rgb1_24_corrupt",
            "file": corrupt.name,
            "width": WIDTH,
            "height": HEIGHT,
            "frames": 1,
            "rate_num": 24,
            "rate_den": 1,
            "rgb_sha256": hashlib.sha256(rgb_frames(1)).hexdigest(),
            "file_sha256": sha256(corrupt),
            "file_bytes": corrupt.stat().st_size,
            "has_audio": False,
            "tags": True,
            "raw_bytes": len(rgb_frames(1)),
        }
    )
    (args.output / "manifest.json").write_text(
        json.dumps(
            {
                "generator": "tests/generate_fixtures.py",
                "producer": {
                    "ffmpeg": version,
                    "video_command": "rawvideo rgb24 -> libx264rgb -crf 0 -preset ultrafast -pix_fmt rgb24, full-range GBR/sRGB/BT.709 tags, MP4 track timescale=rate numerator",
                    "audio_command": "sine 880Hz/8000Hz -> AAC 16kbit/s, duration=0.125s",
                },
                "pattern": "r=(17f+31x+7y+3)%256; g=(29f+5x+47y+11)%256; b=(43f+13x+19y+23)%256",
                "fixtures": entries,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
