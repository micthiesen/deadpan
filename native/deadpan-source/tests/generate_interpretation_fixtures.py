"""Remove only range tags from retained FFV1 pictures with FFmpeg 9.0.1 CLI.

No pixels are re-encoded. --check reproduces retained bytes without writes.
Deadpan decodes these fixtures with its pinned FFmpeg 8.0.3 libraries.
"""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import tempfile

ROOT = Path(__file__).parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    version = subprocess.run(['ffmpeg', '-version'], check=True, text=True, capture_output=True).stdout.splitlines()[0]
    assert version.startswith('ffmpeg version 9.0.1 '), version
    with tempfile.TemporaryDirectory(prefix='deadpan-source-interpretation-') as scratch:
        for name, source in [
            ('missing-range-yuv.mkv', ROOT/'fixtures/limited709.mkv'),
        ]:
            assert hashlib.sha256(source.read_bytes()).hexdigest() == '5b8ef6643040319156c935fcf9c06b4d3ed54539ab20934b1d061dbf2a0e8c58'
            out = Path(scratch)/name
            command = ['ffmpeg', '-nostdin', '-v', 'error', '-i', str(source),
                '-map', '0:v:0', '-c:v', 'copy', '-color_range', 'unknown',
                '-fflags', '+bitexact', str(out)]
            subprocess.run(command, check=True)
            probe = subprocess.run(['ffprobe', '-v', 'error', '-show_frames', '-of', 'json', str(out)], check=True, text=True, capture_output=True)
            frames = json.loads(probe.stdout)['frames']
            assert frames and all('color_range' not in frame for frame in frames)
            data = out.read_bytes()
            destination = ROOT/'fixtures'/name
            if args.check:
                assert destination.read_bytes() == data, name
            else:
                destination.write_bytes(data)
            print(json.dumps(dict(name=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(), frames=len(frames), producer=version)))

if __name__ == '__main__':
    main()
