"""Development-only rotated and anamorphic tracking fixtures (synthetic, not people).

Uses the ffmpeg 9.0.1 CLI producer (FFV1 and libx264); Deadpan decodes them only
through the pinned FFmpeg 8.0.3 libraries. Each fixture is 24 pictures at
24 fps of a bright 36x36 square with a dark cross moving (+5, +1.5) px per
picture from (40, 40) over a static 20 px textured background, drawn in a
320x180 square-pixel picture *before* rotation:

* square-rotated-90.mp4: H.264 320x180, display matrix 90 degrees clockwise
  (upright 180x320).
* square-anamorphic.mkv: FFV1 240x180 with sample aspect ratio 4:3 (upright
  320x180); each coded pixel samples the square-pixel picture at its centre.
* square-rotated-270-anamorphic.mp4: H.264 240x180, SAR 4:3, display matrix
  270 degrees clockwise (upright 180x320).

The independent truth is FFmpeg's own presentation, not Deadpan's mapping:
each fixture is decoded with FFmpeg's automatic rotation, scaled by its SAR to
square pixels, and the centroid and extent of pixels brighter than the
background (luma above 170) are recorded per picture, normalized to the
upright picture, in square-geometry-truth.json. --check regenerates into a
scratch directory and compares bytes and truth without writing.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).parent / 'fixtures'
W, H, FRAMES, SQUARE = 320, 180, 24, 36
COMMON = ['-nostdin', '-v', 'error']
COLOR = ['-color_range', 'tv', '-colorspace', 'bt709', '-color_trc', 'bt709',
         '-color_primaries', 'bt709', '-chroma_sample_location', 'left']


def level(x, y):
    cx, cy = int(x) // 20, int(y) // 20
    value = (cx * 73856093 ^ cy * 19349663 ^ 83492791) & 0xFFFF
    return 40 + value % 60


def luma(x, y, picture):
    sx, sy = round(40 + 5 * picture), round(40 + 1.5 * picture)
    if sx <= x < sx + SQUARE and sy <= y < sy + SQUARE:
        cross = abs(x - sx - SQUARE / 2) < 3 or abs(y - sy - SQUARE / 2) < 3
        return 30 if cross else 230
    return level(x, y)


def raw(coded_width):
    """Planar yuv420p pictures sampling the square-pixel scene at coded centres."""
    scale = W / coded_width
    frames = []
    for picture in range(FRAMES):
        plane = bytearray(coded_width * H)
        for y in range(H):
            for u in range(coded_width):
                plane[y * coded_width + u] = luma((u + 0.5) * scale, y + 0.5, picture)
        chroma = bytes([128]) * (coded_width // 2 * H // 2)
        frames.append(bytes(plane) + chroma + chroma)
    return b''.join(frames)


def encode(scratch, coded_width, sar, codec, output):
    source = scratch / f'raw-{coded_width}.yuv'
    source.write_bytes(raw(coded_width))
    filters = f'setparams=range=tv:color_primaries=bt709:color_trc=bt709:colorspace=bt709,setsar={sar}'
    if codec == 'ffv1':
        codec_args = ['-c:v', 'ffv1', '-level', '3']
    else:
        codec_args = ['-c:v', 'libx264', '-crf', '12',
                      '-x264-params', 'keyint=24:scenecut=0:colorprim=bt709:transfer=bt709:colormatrix=bt709',
                      '-movflags', '+faststart']
    subprocess.run(['ffmpeg', *COMMON, '-f', 'rawvideo', '-pix_fmt', 'yuv420p',
                    '-video_size', f'{coded_width}x{H}', '-framerate', '24', '-i', str(source),
                    '-vf', filters, '-an', '-fflags', '+bitexact', '-flags:v', '+bitexact',
                    *codec_args, '-pix_fmt', 'yuv420p', *COLOR, '-y', str(output)], check=True)


def rotate(source, counterclockwise_degrees, output):
    # FFmpeg's -display_rotation is counterclockwise; Deadpan counts clockwise.
    subprocess.run(['ffmpeg', *COMMON, '-display_rotation', str(counterclockwise_degrees),
                    '-i', str(source), '-c', 'copy', '-fflags', '+bitexact',
                    '-movflags', '+faststart', '-y', str(output)], check=True)


def presented(path):
    """FFmpeg's upright, square-pixel presentation as grey pictures."""
    probe = subprocess.run(['ffmpeg', *COMMON, '-i', str(path), '-vf',
                            'scale=trunc(iw*sar*2/2)*2:ih*2:flags=neighbor,setsar=1,format=gray',
                            '-f', 'rawvideo', '-'], check=True, capture_output=True).stdout
    size = subprocess.run(['ffmpeg', '-nostdin', '-v', 'info', '-i', str(path), '-frames:v', '1', '-vf',
                           'scale=trunc(iw*sar*2/2)*2:ih*2:flags=neighbor,setsar=1,'
                           'format=gray,showinfo', '-f', 'null', '-'],
                          check=True, capture_output=True, text=True).stderr
    width, height = (int(v) for v in re.search(r' s:(\d+)x(\d+)', size).groups())
    frame = width * height
    assert len(probe) == frame * FRAMES, (path, len(probe), width, height)
    return width, height, [probe[i * frame:(i + 1) * frame] for i in range(FRAMES)]


def truth(path):
    width, height, pictures = presented(path)
    samples = []
    for pixels in pictures:
        xs = [i % width for i, v in enumerate(pixels) if v > 170]
        ys = [i // width for i, v in enumerate(pixels) if v > 170]
        samples.append({
            'center': [(sum(xs) / len(xs) + 0.5) / width, (sum(ys) / len(ys) + 0.5) / height],
            'size': [(max(xs) - min(xs) + 1) / width, (max(ys) - min(ys) + 1) / height],
        })
    return {'upright': [width, height], 'pictures': samples}


def build(directory):
    with tempfile.TemporaryDirectory(prefix='deadpan-track-geometry-') as scratch:
        scratch = Path(scratch)
        wide = scratch / 'wide.mp4'
        encode(scratch, W, '1/1', 'h264', wide)
        rotate(wide, -90, directory / 'square-rotated-90.mp4')
        encode(scratch, 240, '4/3', 'ffv1', directory / 'square-anamorphic.mkv')
        squeezed = scratch / 'squeezed.mp4'
        encode(scratch, 240, '4/3', 'h264', squeezed)
        rotate(squeezed, 90, directory / 'square-rotated-270-anamorphic.mp4')
    names = ['square-rotated-90.mp4', 'square-anamorphic.mkv', 'square-rotated-270-anamorphic.mp4']
    report = {}
    for name in names:
        data = (directory / name).read_bytes()
        report[name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(),
                        **truth(directory / name)}
    (directory / 'square-geometry-truth.json').write_text(json.dumps(report, indent=1) + '\n')
    return names + ['square-geometry-truth.json']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if args.check:
        with tempfile.TemporaryDirectory(prefix='deadpan-track-geometry-check-') as out:
            for name in build(Path(out)):
                if (Path(out) / name).read_bytes() != (ROOT / name).read_bytes():
                    raise SystemExit(f'{name} differs from the generator')
    else:
        for name in build(ROOT):
            print(name)


if __name__ == '__main__':
    main()
