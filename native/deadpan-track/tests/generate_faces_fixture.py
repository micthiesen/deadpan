"""Development-only deterministic face-detection fixture; uses the ffmpeg 9.0.1 CLI producer.

Synthetic drawn faces, not person footage. 4 pictures of 480x270 at 24 fps,
eight-bit limited-range BT.709 FFV1 in Matroska (grey: U = V = 128):

* Pictures 0-1: two shaded cartoon heads (hair, eye sockets, pupils, brows,
  nose shadow, mouth, neck) on a flat grey background, a larger one left of
  center and a smaller one right of center, slightly higher.
* Pictures 2-3: the background alone, with no face.

Apple Vision's `VNDetectFaceRectanglesRequest` (revision 3, macOS 26) finds
both drawn heads in pictures 0-1 and nothing in pictures 2-3. Requires Pillow.
The application and tests decode only through the pinned FFmpeg 8.0.3 libraries.
"""
from pathlib import Path
import subprocess
import tempfile

from PIL import Image, ImageFilter

W, H, FRAMES, FACES_UNTIL = 480, 270, 4, 2
SKIN = (225, 175, 140)
HAIR = (50, 35, 25)


def draw_face(pixels, cx, cy, s):
    for y in range(int(cy - 1.6 * s), int(cy + 1.6 * s)):
        for x in range(int(cx - 1.2 * s), int(cx + 1.2 * s)):
            if not (0 <= x < W and 0 <= y < H):
                continue
            u, v = (x - cx) / (0.75 * s), (y - cy) / s
            r = u * u + v * v
            colour = None
            if r < 1:
                base = [int(c * (1 - 0.35 * r)) for c in SKIN]
                colour = base
                for ex in (-0.36, 0.36):
                    du, dv = (x - (cx + ex * s)) / (0.22 * s), (y - (cy - 0.15 * s)) / (0.13 * s)
                    socket = du * du + dv * dv
                    if socket < 1:
                        colour = [int(c * (0.55 + 0.45 * socket)) for c in base]
                    pu, pv = (x - (cx + ex * s)) / (0.08 * s), (y - (cy - 0.15 * s)) / (0.08 * s)
                    if pu * pu + pv * pv < 1:
                        colour = [30, 25, 20]
                    if abs(y - (cy - 0.36 * s)) < 0.04 * s and abs(x - (cx + ex * s)) < 0.22 * s:
                        colour = [60, 45, 35]
                if abs(x - (cx + 0.05 * s)) < 0.05 * s and cy - 0.05 * s < y < cy + 0.25 * s:
                    colour = [int(c * 0.8) for c in base]
                if abs(x - cx) < 0.14 * s and abs(y - (cy + 0.28 * s)) < 0.04 * s:
                    colour = [int(c * 0.6) for c in base]
                if abs(x - cx) < 0.28 * s and abs(y - (cy + 0.5 * s)) < 0.05 * s:
                    colour = [140, 60, 60]
                if v < -0.55:
                    colour = list(HAIR)
            elif r < 1.25 and v < 0.0:
                colour = list(HAIR)
            elif abs(u) < 0.4 and v > 0.9:
                colour = [int(c * 0.8) for c in SKIN]
            if colour:
                pixels[x, y] = tuple(colour)


def picture(index):
    image = Image.new('RGB', (W, H), (120, 130, 140))
    if index < FACES_UNTIL:
        pixels = image.load()
        draw_face(pixels, 135, 128, 60)
        draw_face(pixels, 352, 120, 45)
    luma = image.filter(ImageFilter.GaussianBlur(1.5)).convert('L')
    # Full-range grey to limited range, rounded half up.
    limited = bytes((16 + (value * 219 + 127) // 255) for value in luma.tobytes())
    chroma = bytes([128]) * (W // 2 * H // 2)
    return limited + chroma + chroma


root = Path(__file__).parent
with tempfile.TemporaryDirectory(prefix='deadpan-faces-fixture-') as scratch:
    raw = Path(scratch) / 'frames.yuv'
    raw.write_bytes(b''.join(picture(index) for index in range(FRAMES)))
    subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'rawvideo', '-pix_fmt', 'yuv420p',
                    '-video_size', f'{W}x{H}', '-framerate', '24', '-i', str(raw),
                    '-vf', 'setparams=range=tv:color_primaries=bt709:color_trc=bt709:colorspace=bt709',
                    '-an', '-fflags', '+bitexact', '-flags:v', '+bitexact', '-c:v', 'ffv1', '-level', '3', '-pix_fmt', 'yuv420p',
                    '-color_range', 'tv', '-colorspace', 'bt709', '-color_trc', 'bt709',
                    '-color_primaries', 'bt709', '-chroma_sample_location', 'left', '-y', str(root / 'fixtures' / 'two-drawn-faces.mkv')],
                   check=True)
