"""Development-only deterministic tracking fixture; uses the ffmpeg 9.0.1 CLI producer.

Synthetic, not person footage. 48 pictures of 320x180 at 24 fps, eight-bit
limited-range BT.709 FFV1 in Matroska (grey: U = V = 128):

* Shot A, pictures 0-29: a bright 36x36 square with a dark cross moves
  (+5, +1.5) px per picture over a static dark textured background. A dark
  50 px pillar at x = 140..190 hides it completely in pictures 20-22 and
  partially in 14-19 and 23-28.
* Shot B, pictures 30-47: a hard cut to a bright textured background with a
  second, identical square moving left from (230, 110).

The application and tests decode only through the pinned FFmpeg 8.0.3 libraries.
"""
from pathlib import Path
import subprocess
import tempfile

W, H, FRAMES, CUT = 320, 180, 48, 30
SQUARE = 36
PILLAR = (140, 190)


def cell_level(seed, cx, cy, low, high):
    value = (cx * 73856093 ^ cy * 19349663 ^ seed * 83492791) & 0xFFFF
    return low + value % (high - low)


def square_origin(picture):
    if picture < CUT:
        return 40 + 5 * picture, 40 + 1.5 * picture
    return 230 - 4 * (picture - CUT), 110 - 1 * (picture - CUT)


def picture(index):
    shot_a = index < CUT
    seed, low, high = (1, 40, 100) if shot_a else (2, 150, 215)
    luma = bytearray(W * H)
    for y in range(H):
        for x in range(W):
            luma[y * W + x] = cell_level(seed, x // 20, y // 20, low, high)
    sx, sy = square_origin(index)
    sx, sy = int(round(sx)), int(round(sy))
    for y in range(sy, sy + SQUARE):
        for x in range(sx, sx + SQUARE):
            if 0 <= x < W and 0 <= y < H:
                cross = abs(x - sx - SQUARE // 2) < 3 or abs(y - sy - SQUARE // 2) < 3
                luma[y * W + x] = 30 if cross else 230
    if shot_a:
        for y in range(H):
            for x in range(*PILLAR):
                luma[y * W + x] = 20
    chroma = bytes([128]) * (W // 2 * H // 2)
    return bytes(luma) + chroma + chroma


root = Path(__file__).parent
with tempfile.TemporaryDirectory(prefix='deadpan-track-fixture-') as scratch:
    raw = Path(scratch) / 'frames.yuv'
    raw.write_bytes(b''.join(picture(index) for index in range(FRAMES)))
    subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'rawvideo', '-pix_fmt', 'yuv420p',
                    '-video_size', f'{W}x{H}', '-framerate', '24', '-i', str(raw),
                    '-vf', 'setparams=range=tv:color_primaries=bt709:color_trc=bt709:colorspace=bt709',
                    '-an', '-fflags', '+bitexact', '-flags:v', '+bitexact', '-c:v', 'ffv1', '-level', '3', '-pix_fmt', 'yuv420p',
                    '-color_range', 'tv', '-colorspace', 'bt709', '-color_trc', 'bt709',
                    '-color_primaries', 'bt709', '-chroma_sample_location', 'left', '-y', str(root / 'fixtures' / 'moving-square-cut.mkv')],
                   check=True)
