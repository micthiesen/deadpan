"""Development-only deterministic HDR source fixtures.

Producers: Homebrew ffmpeg 9.0.1 CLI with libx265 4.x and libx264 (GPL,
development-only; Deadpan never links them) and the native AAC encoder, plus
tests/generate_hdr_static_metadata.c built against DEADPAN_FFMPEG_PREFIX
(pinned LGPL FFmpeg 8.0.3) to add stream mastering/content-light boxes.
The application and tests decode only through the pinned FFmpeg 8.0.3 libraries.

Run from the repository root with DEADPAN_FFMPEG_PREFIX exported:
    python3 native/deadpan-source/tests/generate_hdr_fixtures.py

Picture content (10-bit limited-range Y'CbCr codes, BT.2020 NCL):
* Small 64x36 fixtures, 24 fps: rows 0..23 hold four 16-pixel-wide flat
  patches (Y, Cb, Cr) = (64,512,512), (P,512,512), (400,450,600),
  (940,512,512), where P = 573 for PQ (about 203 cd/m2) and 721 for HLG
  (75%). Rows 24..35 hold Y = 64 + ((12x + 37f) mod 877), neutral chroma.
* AV fixtures, 320x180, 30 fps, 60 frames: rows 0..89 hold five 64-pixel
  patches Y = 64, P, 400 (Cb 450, Cr 600), Q, 940 with neutral chroma
  elsewhere, where (P, Q) = (573, 723) for PQ (203 and about 1000 cd/m2)
  and (721, 502) for HLG (75% and 50%). Rows 90..179 hold
  Y = 64 + ((2x + y + 8f) mod 877) with a 16x16 Y=940 box at
  x = (5f) mod 304, y = 120..135. Frame 30 (t = 1 s) alone also has a
  32x32 Y=940 marker at x 288..319, y 148..179. The 48 kHz stereo AAC track
  is silent except a 10 ms 1 kHz sine burst (amplitude 0.5) starting at
  sample 48000 (t = 1 s) in both channels.
* Grain AV fixtures (`generate_hdr_fixtures.py grain` regenerates only
  these), 320x180, 30 fps, 60 frames of real-like footage for picture-gate
  calibration: rows 0..99 a sky Y = 300 + 3y with Cb/Cr 560-1.2y/470+y (at
  chroma rows), a highlight disc of radius 16 (soft to 26) at (250, 38) with
  Y = H; rows 100..179 a panning texture Y = 230 + 60 sin((x+2f)/9) sin(y/5)
  with 2-pixel fence posts (+140) every 12 pixels; a dark subject disc of
  radius 20 (Y 180, Cb 470, Cr 570) at (40 + 3f, 112); a graphics-white sign
  Y = P at x 20..99, y 140..149. Clumped grain (half the sum of a 2x2
  neighborhood of unit noise from an integer hash, no library PRNG) of sigma
  10 luma and 5 chroma codes covers everything. (P, H) = (573, 723) for PQ,
  (721, 860) for HLG. Same AAC track; x265 CRF 18 with -tune grain.
"""
from pathlib import Path
import math
import os
import struct
import subprocess
import sys
import tempfile

root = Path(__file__).parent
fixtures = root / 'fixtures'
prefix = Path(os.environ['DEADPAN_FFMPEG_PREFIX'])
MASTER = 'G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1)'
COMMON = ('pools=none:frame-threads=1:lookahead-threads=0:info=0:repeat-headers=0:'
          'chromaloc=0:range=limited:colorprim={prim}:transfer={trc}:colormatrix={mat}')


def plane_bytes(values):
    return struct.pack(f'<{len(values)}H', *values)


def small_frames(path, patch, frames):
    w, h = 64, 36
    out = bytearray()
    for f in range(frames):
        y_plane = [[64, patch, 400, 940][x // 16] if y < 24 else 64 + ((12 * x + 37 * f) % 877)
                   for y in range(h) for x in range(w)]
        out += plane_bytes(y_plane)
        for c in range(2):
            out += plane_bytes([[512, 512, (450, 600)[c], 512][(2 * x) // 16] if y < 12 else 512
                                for y in range(h // 2) for x in range(w // 2)])
    path.write_bytes(out)


def av_frames(path, patches):
    w, h = 320, 180
    out = bytearray()
    for f in range(60):
        y_plane = []
        for y in range(h):
            for x in range(w):
                if y < 90:
                    value = patches[x // 64]
                elif f == 30 and x >= 288 and y >= 148:
                    value = 940
                elif (5 * f) % 304 <= x < (5 * f) % 304 + 16 and 120 <= y < 136:
                    value = 940
                else:
                    value = 64 + ((2 * x + y + 8 * f) % 877)
                y_plane.append(value)
        out += plane_bytes(y_plane)
        for c in range(2):
            out += plane_bytes([(450, 600)[c] if y < 45 and (2 * x) // 64 == 2 else 512
                                for y in range(h // 2) for x in range(w // 2)])
    path.write_bytes(out)


def noise_hash(*values):
    """Deterministic 32-bit integer hash (no library PRNG, so any language
    and Python version reproduces the same grain)."""
    h = 0x811C9DC5
    for value in values:
        h = ((h ^ (value & 0xFFFFFFFF)) * 0x01000193) & 0xFFFFFFFF
        h ^= h >> 15
        h = (h * 0x2C1B3C6D) & 0xFFFFFFFF
        h ^= h >> 12
    return h


def white_noise(seed, frame, width, height):
    """Approximately unit-variance Gaussian noise: the sum of three uniform
    values in [-1, 1) has variance 1."""
    out = []
    for y in range(height):
        for x in range(width):
            h = noise_hash(seed, frame, x, y)
            total = 0.0
            for shift in (0, 10, 20):
                total += ((h >> shift) & 0x3FF) / 512.0 - 1.0
            out.append(total)
    return out


def grain(seed, frame, width, height):
    """Film-grain-like clumped noise: each value is half the sum of a 2x2
    neighborhood of unit white noise (unit variance, 2-pixel correlation)."""
    w = white_noise(seed, frame, width + 1, height + 1)
    stride = width + 1
    return [(w[y * stride + x] + w[y * stride + x + 1] + w[(y + 1) * stride + x]
             + w[(y + 1) * stride + x + 1]) / 2.0
            for y in range(height) for x in range(width)]


def grain_frames(path, white, highlight):
    """Realistic-like grainy HDR footage, 320x180, 60 frames (see docstring)."""
    w, h = 320, 180
    clamp = lambda value: max(64, min(940, int(round(value))))
    clamp_c = lambda value: max(64, min(960, int(round(value))))
    out = bytearray()
    for f in range(60):
        noise = grain(1, f, w, h)
        y_plane = []
        for y in range(h):
            for x in range(w):
                if y < 100:
                    value = 300 + 3.0 * y  # sky: 300 at the top to 597 at the horizon
                    sun = math.hypot(x - 250, y - 38)
                    if sun < 16:
                        value = highlight
                    elif sun < 26:
                        value = value + (highlight - value) * (26 - sun) / 10
                else:
                    pan = x + 2 * f
                    value = 230 + 60 * math.sin(pan / 9.0) * math.sin(y / 5.0)
                    if pan % 12 < 2:
                        value += 140  # fence posts: 2-pixel edges
                subject = math.hypot(x - (40 + 3 * f), y - 112)
                if subject < 20:
                    value = value + (180 - value) * min(1.0, 20 - subject)
                if 140 <= y < 150 and 20 <= x < 100:
                    value = white  # a graphics-white sign the grain also covers
                y_plane.append(clamp(value + 10.0 * noise[y * w + x]))
        out += plane_bytes(y_plane)
        cw, ch = w // 2, h // 2
        for c, seed in enumerate((2, 3)):
            chroma_noise = grain(seed, f, cw, ch)
            plane = []
            for y in range(ch):
                for x in range(cw):
                    if y < 50:
                        value = (560 - 1.2 * y, 470 + 1.0 * y)[c]
                    else:
                        value = (490, 545)[c]
                    if math.hypot(2 * x - (40 + 3 * f), 2 * y - 112) < 20:
                        value = (470, 570)[c]
                    plane.append(clamp_c(value + 5.0 * chroma_noise[y * cw + x]))
            out += plane_bytes(plane)
    path.write_bytes(out)


def audio(path):
    samples = []
    for n in range(96000):
        value = 0.0
        if 48000 <= n < 48480:
            value = 0.5 * math.sin(2 * math.pi * 1000 * (n - 48000) / 48000)
        sample = int(round(value * 32767))
        samples += [sample, sample]
    path.write_bytes(struct.pack(f'<{len(samples)}h', *samples))


def run(*args):
    subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-threads', '1', *args], check=True)


def x265(raw, size, rate, out, params, extra=(), audio_input=None, tag='hvc1'):
    inputs = ['-f', 'rawvideo', '-pix_fmt', 'yuv420p10le', '-s', size, '-r', rate, '-i', str(raw)]
    if audio_input:
        inputs += ['-f', 's16le', '-ar', '48000', '-ac', '2', '-i', str(audio_input)]
    codec = ['-c:v', 'libx265', '-preset', 'medium', '-x265-params', params, '-tag:v', tag]
    if out.suffix == '.hevc':
        codec = ['-c:v', 'libx265', '-preset', 'medium', '-x265-params', params, '-f', 'hevc']
    audio_codec = ['-c:a', 'aac', '-b:a', '128k'] if audio_input else ['-an']
    run(*inputs, *codec, *extra, *audio_codec, '-fflags', '+bitexact', '-flags:v', '+bitexact',
        '-flags:a', '+bitexact', '-map_metadata', '-1', '-movflags', '+write_colr+faststart', '-y', str(out))


def tags(trc, prim='bt2020', mat='bt2020nc'):
    # setparams labels the raw input identically, so ffmpeg's filter-graph
    # color negotiation inserts no conversion and lossless coding stays exact.
    return ['-vf', f'setparams=range=tv:color_primaries={prim}:color_trc={trc}:colorspace={mat}',
            '-color_primaries', prim, '-color_trc', trc, '-colorspace', mat,
            '-color_range', 'tv', '-chroma_sample_location', 'left']


def grain_fixtures(scratch, remux, pq, hlg, static, pcm):
    """Grainy AV fixtures: CRF 18 with x265's grain tuning, IDR every 30
    pictures, two B frames, the same AAC track as the clean AV fixtures."""
    gop30 = 'keyint=30:min-keyint=30:scenecut=0:bframes=2:open-gop=0:crf=18'
    grain_frames(scratch / 'pq-grain.yuv', 573, 723)
    x265(scratch / 'pq-grain.yuv', '320x180', '30', scratch / 'pq-grain.mp4', f'{pq}:{gop30}:{static}',
         [*tags('smpte2084'), '-tune', 'grain'], audio_input=pcm)
    subprocess.run([str(remux), str(scratch / 'pq-grain.mp4'), str(fixtures / 'hdr-pq-grain-av.mp4'), 'pq',
                    'static'], check=True)
    grain_frames(scratch / 'hlg-grain.yuv', 721, 860)
    x265(scratch / 'hlg-grain.yuv', '320x180', '30', fixtures / 'hdr-hlg-grain-av.mp4', f'{hlg}:{gop30}',
         [*tags('arib-std-b67'), '-tune', 'grain'], audio_input=pcm)


def invalid_static_fixture(scratch, pq):
    """PQ HEVC whose SEI static metadata fail the shared rule set: a mastering
    peak of 10 cd/m2 (below the 50 cd/m2 floor) and MaxFALL above MaxCLL.
    Admission ignores both with a recorded note instead of refusing."""
    small_frames(scratch / 'invalid-static.yuv', 573, 2)
    two = 'keyint=2:min-keyint=2:scenecut=0:bframes=0:open-gop=0:lossless=1'
    master = MASTER.replace('L(10000000,1)', 'L(100000,1)')
    x265(scratch / 'invalid-static.yuv', '64x36', '24', fixtures / 'hevc-pq-invalid-static.mp4',
         f'{pq}:{two}:hdr10=1:hdr10-opt=0:master-display={master}:max-cll=100,400', tags('smpte2084'))


def open_gop_fixture(scratch, pq):
    """Open-GOP PQ HEVC: IDR, then a CRA every 8 pictures whose three leading
    B pictures are RASL (they reference the previous GOP), lossless, 24 frames
    of the small picture content."""
    small_frames(scratch / 'open-gop.yuv', 573, 24)
    gop = 'keyint=8:min-keyint=8:scenecut=0:bframes=3:b-adapt=0:open-gop=1:radl=0:lossless=1'
    x265(scratch / 'open-gop.yuv', '64x36', '24', fixtures / 'hevc-pq-open-gop.mp4', f'{pq}:{gop}',
         tags('smpte2084'))


# `generate_hdr_fixtures.py grain` regenerates only the grainy AV fixtures;
# `generate_hdr_fixtures.py review` regenerates only the open-GOP and invalid
# static-metadata fixtures.
only_grain = sys.argv[1:] == ['grain']
only_review = sys.argv[1:] == ['review']
with tempfile.TemporaryDirectory(prefix='deadpan-source-hdr-') as scratch:
    scratch = Path(scratch)
    remux = scratch / 'remux'
    subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Werror', f'-I{prefix}/include',
                    str(root / 'generate_hdr_static_metadata.c'), f'-L{prefix}/lib',
                    f'-Wl,-rpath,{prefix}/lib', '-lavformat', '-lavcodec', '-lavutil',
                    '-o', str(remux)], check=True)
    pq = COMMON.format(prim='bt2020', trc='smpte2084', mat='bt2020nc')
    hlg = COMMON.format(prim='bt2020', trc='arib-std-b67', mat='bt2020nc')
    gop4 = 'keyint=4:min-keyint=4:scenecut=0:bframes=0:open-gop=0:lossless=1'
    static = f'hdr10=1:hdr10-opt=0:master-display={MASTER}:max-cll=1000,400'
    pcm = scratch / 'audio.pcm'
    audio(pcm)
    if only_review:
        open_gop_fixture(scratch, pq)
        invalid_static_fixture(scratch, pq)
    elif not only_grain:
        pq_small, hlg_small = scratch / 'pq.yuv', scratch / 'hlg.yuv'
        small_frames(pq_small, 573, 8)
        small_frames(hlg_small, 721, 8)
        # (a) PQ HEVC Main10 with in-band SEI and mdcv/clli boxes, lossless.
        x265(pq_small, '64x36', '24', scratch / 'pq.mp4', f'{pq}:{gop4}:{static}', tags('smpte2084'))
        subprocess.run([str(remux), str(scratch / 'pq.mp4'), str(fixtures / 'hevc-pq.mp4'), 'pq', 'static'], check=True)
        # (b) HLG HEVC Main10 without static metadata, lossless.
        x265(hlg_small, '64x36', '24', fixtures / 'hevc-hlg.mp4', f'{hlg}:{gop4}', tags('arib-std-b67'))
        # (c) H.264 High10 PQ without static metadata (near-lossless, qp 1).
        run('-f', 'rawvideo', '-pix_fmt', 'yuv420p10le', '-s', '64x36', '-r', '24', '-i', str(pq_small),
            '-c:v', 'libx264', '-profile:v', 'high10', '-pix_fmt', 'yuv420p10le',
            '-x264-params', 'keyint=4:min-keyint=4:scenecut=0:bframes=0:threads=1:qp=1:'
            'colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:chromaloc=0:range=tv',
            *tags('smpte2084'), '-an', '-fflags', '+bitexact', '-flags:v', '+bitexact', '-map_metadata', '-1',
            '-movflags', '+write_colr+faststart', '-y', str(fixtures / 'h264-high10-pq.mp4'))
        # Refusals: ten-bit SDR, and PQ tagged with BT.709 primaries/matrix.
        sdr = COMMON.format(prim='bt709', trc='bt709', mat='bt709')
        two = 'keyint=2:min-keyint=2:scenecut=0:bframes=0:open-gop=0:lossless=1'
        small_frames(scratch / 'two.yuv', 573, 2)
        x265(scratch / 'two.yuv', '64x36', '24', fixtures / 'hevc-ten-bit-sdr.mp4', f'{sdr}:{two}',
             tags('bt709', 'bt709', 'bt709'))
        pq709 = COMMON.format(prim='bt709', trc='smpte2084', mat='bt709')
        x265(scratch / 'two.yuv', '64x36', '24', fixtures / 'hevc-pq-bt709.mp4', f'{pq709}:{two}',
             tags('smpte2084', 'bt709', 'bt709'))
        # Refusal: two IDR segments whose identical parameter sets repeat in-band
        # and whose SEI mastering metadata differs (second max luminance 4000).
        other = MASTER.replace('L(10000000,1)', 'L(40000000,1)')
        x265(scratch / 'two.yuv', '64x36', '24', scratch / 'one.hevc', f'{pq}:{two}:{static}:repeat-headers=1', tags('smpte2084'))
        x265(scratch / 'two.yuv', '64x36', '24', scratch / 'two.hevc',
             f'{pq}:{two}:hdr10=1:hdr10-opt=0:master-display={other}:max-cll=1000,400:repeat-headers=1',
             tags('smpte2084'))
        (scratch / 'joined.hevc').write_bytes((scratch / 'one.hevc').read_bytes() + (scratch / 'two.hevc').read_bytes())
        run('-f', 'hevc', '-r', '24', '-i', str(scratch / 'joined.hevc'), '-c', 'copy', '-tag:v', 'hvc1',
            '-fflags', '+bitexact', '-map_metadata', '-1', '-movflags', '+write_colr+faststart',
            '-y', str(fixtures / 'hevc-pq-mastering-change.mp4'))

        # Whole-project AV fixtures: lossy CRF 18, IDR every 30 pictures, two B frames.
        gop30 = 'keyint=30:min-keyint=30:scenecut=0:bframes=2:open-gop=0:crf=18'
        av_frames(scratch / 'pq-av.yuv', [64, 573, 400, 723, 940])
        x265(scratch / 'pq-av.yuv', '320x180', '30', scratch / 'pq-av.mp4', f'{pq}:{gop30}:{static}',
             tags('smpte2084'), audio_input=pcm)
        subprocess.run([str(remux), str(scratch / 'pq-av.mp4'), str(fixtures / 'hdr-pq-av.mp4'), 'pq', 'static'],
                       check=True)
        av_frames(scratch / 'hlg-av.yuv', [64, 721, 400, 502, 940])
        x265(scratch / 'hlg-av.yuv', '320x180', '30', fixtures / 'hdr-hlg-av.mp4', f'{hlg}:{gop30}',
             tags('arib-std-b67'), audio_input=pcm)
    if not only_review:
        grain_fixtures(scratch, remux, pq, hlg, static, pcm)
    if not only_grain and not only_review:
        open_gop_fixture(scratch, pq)
        invalid_static_fixture(scratch, pq)

names = ['hdr-pq-grain-av.mp4', 'hdr-hlg-grain-av.mp4']
if only_review:
    names = ['hevc-pq-open-gop.mp4', 'hevc-pq-invalid-static.mp4']
elif not only_grain:
    names = ['hevc-pq.mp4', 'hevc-hlg.mp4', 'h264-high10-pq.mp4', 'hevc-ten-bit-sdr.mp4', 'hevc-pq-bt709.mp4',
             'hevc-pq-mastering-change.mp4', 'hdr-pq-av.mp4', 'hdr-hlg-av.mp4', *names,
             'hevc-pq-open-gop.mp4', 'hevc-pq-invalid-static.mp4']
for name in names:
    path = fixtures / name
    digest = subprocess.run(['shasum', '-a', '256', str(path)], check=True, capture_output=True,
                            text=True).stdout.split()[0]
    print(name, path.stat().st_size, digest)
