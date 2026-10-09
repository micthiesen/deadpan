"""Build and run the native field adapter's direct ASan/UBSan contract tests."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
root = Path(__file__).resolve().parents[3]
prefix = Path(os.environ['DEADPAN_FFMPEG_PREFIX'])
source = root / 'native/deadpan-source/src'
binary = args.output.resolve() / 'deinterlace-test'
command = ['clang', '-std=c11', '-Wall', '-Wextra', '-Werror',
           '-fsanitize=address,undefined', '-fno-sanitize-recover=all',
           '-fno-omit-frame-pointer', '-g', '-I'+str(source), '-I'+str(prefix/'include'),
           str(source/'deinterlace.c'), str(Path(__file__).with_name('deinterlace_test.c')),
           '-L'+str(prefix/'lib'), '-Wl,-rpath,'+str(prefix/'lib'), '-lavfilter', '-lavutil',
           '-o', str(binary)]
subprocess.run(command, check=True, timeout=60)
result = subprocess.run([str(binary)], text=True, capture_output=True, timeout=30)
(args.output/'run.log').write_text(result.stdout + result.stderr)
report = {'command':command, 'exit_code':result.returncode,
          'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),
          'scope':'C adapter instrumented with ASan/UBSan; pinned FFmpeg libraries uninstrumented'}
print(result.stdout, end='')
print(result.stderr, end='')
timing_binary = args.output.resolve() / 'h264-timing-test'
timing_command = command.copy()
timing_command[timing_command.index(str(Path(__file__).with_name('deinterlace_test.c')))] = str(Path(__file__).with_name('h264_timing_test.c'))
timing_command[-1] = str(timing_binary)
timing_command += ['-lavcodec', '-lavformat', '-lswscale']
subprocess.run(timing_command, check=True, timeout=60)
timing = subprocess.run([str(timing_binary)], text=True, capture_output=True, timeout=30)
(args.output/'h264-timing.log').write_text(timing.stdout + timing.stderr)
report['picture_timing'] = {'command':timing_command, 'exit_code':timing.returncode,
                            'binary_sha256':hashlib.sha256(timing_binary.read_bytes()).hexdigest()}
report['exit_code'] = result.returncode or timing.returncode
(args.output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
print(timing.stdout, end='')
print(timing.stderr, end='')
print(json.dumps(report))
raise SystemExit(report['exit_code'])
