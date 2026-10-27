#!/usr/bin/env python3
"""Generate an owned 260-picture shot-range parity input, no external media."""
import hashlib
import json
import pathlib
import subprocess
import sys

output = pathlib.Path(sys.argv[1]).resolve()
pixels = bytearray()
for frame in range(260):
    for y in range(36):
        for x in range(64):
            warm = (210, 100 + x, 20)
            blue = (10, 20, 120 + y)
            if frame < 64:
                color = warm
            elif frame < 89:
                blend = frame - 63
                color = tuple((a * (26 - blend) + b * blend) // 26 for a, b in zip(warm, blue))
            elif frame < 128:
                color = blue
            elif frame == 191:
                color = (255, 255, 255)
            else:
                color = (20, 180 + x // 2, 40 + y)
            pixels.extend(color)

argv = [
    "ffmpeg", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb24",
    "-s", "64x36", "-r", "24", "-i", "pipe:0", "-an",
    "-vf", "format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
    "-c:v", "libx264", "-preset", "veryslow", "-crf", "18",
    "-pix_fmt", "yuv420p", "-g", "64", "-bf", "2", "-threads", "2",
    "-color_range", "tv", "-colorspace", "bt709",
    "-color_primaries", "bt709", "-color_trc", "bt709",
    "-bsf:v", "h264_metadata=colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1:video_full_range_flag=0",
    "-movflags", "+faststart+write_colr", str(output),
]
subprocess.run(argv, input=pixels, check=True)
record = {
    "kind": "procedural owned test media",
    "license": "CC0-1.0",
    "pictures": 260,
    "width": 64,
    "height": 36,
    "rate": "24/1",
    "construction": {
        "warm_static": [0, 64],
        "linear_warm_to_blue": [64, 89],
        "blue_static": [89, 128],
        "green_static": [128, 260],
        "one_picture_white_flash": 191,
        "hard_cut": 128,
    },
    "raw_rgb24_sha256": hashlib.sha256(pixels).hexdigest(),
    "encoded_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
    "encoded_bytes": output.stat().st_size,
    "generation_argv": argv[:-1] + ["controlled-scenes.mp4"],
    "generator_sha256": hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
    "ffmpeg_version": subprocess.run(["ffmpeg", "-version"], capture_output=True, text=True, check=True).stdout.splitlines()[0],
    "qualification": "Encoded bytes are pinned; do not assume another encoder build regenerates identical bytes. No quality or throughput claim.",
}
output.with_suffix(".json").write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(record, indent=2))
