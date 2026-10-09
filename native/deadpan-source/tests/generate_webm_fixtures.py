"""Bitexact VP9 WebM/Matroska remuxes of the retained VP9 MP4 fixtures.

Development FFmpeg 9.0.1 copies compressed pictures, drops the AAC track, and
records explicit left chroma siting. Container clocks are 1/1000, deliberately
different from the original MP4 clock. --check performs no repository writes.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).parent / 'fixtures'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    manifest = json.loads((ROOT / 'manifest.json').read_text())
    hashes = {entry['name']: entry['sha256'] for entry in manifest['files']}
    names = [f'vp9-sdr-{bits}-{range_}' for bits in [8, 10] for range_ in ['limited', 'full']]
    names += ['vp9-altref', 'vp9-existing-8', 'vp9-existing-10']
    cases = [(name, name + '.webm', []) for name in names]
    cases += [('vp9-sdr-8-limited', 'vp9-sdr-8-limited.mkv', [])]
    cases += [('vp9-sdr-8-limited', 'vp9-anamorphic.webm', ['-aspect', '2/1'])]
    cases += [('vp9-sdr-10-limited', 'vp9-vfr.webm', [
        '-bsf:v', 'setts=pts=N*3000+mod(N\\,2)*600:dts=N*3000+mod(N\\,2)*600:duration=if(mod(N\\,2)\\,2400\\,3600)',
    ])]
    with tempfile.TemporaryDirectory(prefix='deadpan-webm-') as tmp:
        for source, name, options in cases:
            source = ROOT / (source + '.mp4')
            assert hashlib.sha256(source.read_bytes()).hexdigest() == hashes[source.name], source
            target = Path(tmp) / name
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-y', '-i', str(source),
                '-map', '0:v:0', '-c', 'copy', '-chroma_sample_location', 'left',
                '-fflags', '+bitexact', '-map_metadata', '-1', *options, str(target)], check=True)
            data = target.read_bytes()
            if args.check:
                assert data == (ROOT / name).read_bytes(), name
            else:
                (ROOT / name).write_bytes(data)
            print(json.dumps({'name': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(),
                'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest()}))


if __name__ == '__main__':
    main()
