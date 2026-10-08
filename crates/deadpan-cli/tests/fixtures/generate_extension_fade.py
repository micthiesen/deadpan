"""Rebuild the tiny lossless fade used by temporal-context integration tests."""

from pathlib import Path
import hashlib
import subprocess
import sys


def scene(x, y, cool=False):
    return (10, 20, 90 + y * 4) if cool else (200, 120 + (x * 3 % 192) // 4, 40)


def pixels():
    width, height = 64, 36
    frames = bytearray()
    for ordinal in range(120):
        step = max(0, min(33, ordinal - 79))
        for y in range(height):
            for x in range(width):
                a, b = scene(x // 2, y // 2), scene(x // 2, y // 2, True)
                frames.extend((left * (33 - step) + right * step + 16) // 33
                              for left, right in zip(a, b))
    return bytes(frames)


def main():
    output = Path(__file__).with_name("extension-fade.mp4")
    frames = pixels()
    if sys.argv[1:] == ["--check"]:
        decoded = subprocess.run([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-i", str(output),
            "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
        ], check=True, capture_output=True, timeout=30).stdout
        assert decoded == frames, "decoded RGB differs from generated input"
        print(f"Verified {len(decoded)} decoded RGB channels exactly")
        return
    if sys.argv[1:]:
        raise SystemExit("Usage: generate_extension_fade.py [--check]")
    subprocess.run([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-n",
        "-fflags", "+bitexact", "-f", "rawvideo", "-pixel_format", "rgb24",
        "-video_size", "64x36", "-framerate", "30000/1001",
        "-i", "pipe:0", "-an", "-c:v", "libx264rgb", "-crf", "0", "-g", "1",
        "-vf", "setparams=range=full:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=gbr",
        "-threads", "1", "-flags:v", "+bitexact", "-pix_fmt", "rgb24",
        "-colorspace", "rgb", "-color_trc", "iec61966-2-1",
        "-color_primaries", "bt709", "-color_range", "pc", "-movflags", "+write_colr",
        "-video_track_timescale", "30000", str(output),
    ], input=frames, check=True, timeout=30)
    print(output.name, output.stat().st_size, hashlib.sha256(output.read_bytes()).hexdigest())


if __name__ == "__main__":
    main()
