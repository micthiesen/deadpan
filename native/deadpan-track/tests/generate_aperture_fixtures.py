"""Fractional-aperture Vision fixtures from the existing synthetic originals.

Development-only ffmpeg 9.0.1 / libx264 encodes the original FFV1 pictures at
CRF 12. Deadpan links only pinned LGPL FFmpeg 8.0.3. A shared byte-level
generator adds clap without changing the compressed payload or timestamps.
The first crop retains the moving target; the second excludes the right face.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).parent
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('aperture', ROOT / '../../deadpan-source/tests/generate_aperture_fixtures.py')
aperture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(aperture)

CASES = [
    ('moving-square-cut.mkv', 'moving-square-aperture.mp4', [559,2,319,2,0,1,0,1],
     'de0f4c886b453d3a3f5b3ff8c1461a0ab3cb671a71884c06cd288f82d9b453d0'),
    ('two-drawn-faces.mkv', 'faces-aperture.mp4', [639,2,539,2,-80,1,0,1],
     '563904e45b81ee8c80200e222fb26470089fbe62477f77072f17e8953de1e958'),
]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='deadpan-fractional-vision-') as scratch:
        for source, target, words, digest in CASES:
            aperture.require(hashlib.sha256((ROOT/'fixtures'/source).read_bytes()).hexdigest() == digest,
                             'pinned synthetic original hash')
            encoded = Path(scratch)/target
            subprocess.run(['ffmpeg','-nostdin','-v','error','-i',str(ROOT/'fixtures'/source),
                            '-an','-c:v','libx264','-crf','12','-pix_fmt','yuv420p',
                            '-fflags','+bitexact','-flags:v','+bitexact',
                            '-x264-params','keyint=24:scenecut=0:colorprim=bt709:transfer=bt709:colormatrix=bt709',
                            '-color_range','tv','-colorspace','bt709','-color_trc','bt709',
                            '-color_primaries','bt709','-chroma_sample_location','left',
                            '-movflags','+faststart','-y',str(encoded)],check=True)
            result = aperture.generate(encoded.read_bytes(),words)
            path = ROOT/'fixtures'/target
            if args.check:
                aperture.require(path.read_bytes() == result, 'fixture matches generator')
            else:
                path.write_bytes(result)
            print(json.dumps({'name':target,'bytes':len(result), 'sha256':hashlib.sha256(result).hexdigest(),
                              'source_sha256':hashlib.sha256((ROOT/'fixtures'/source).read_bytes()).hexdigest(),
                              'clap':words}))

if __name__ == '__main__':
    main()
