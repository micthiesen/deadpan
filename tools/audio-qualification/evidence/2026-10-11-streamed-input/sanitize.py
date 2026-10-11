from pathlib import Path
import hashlib
import json
import os
import subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
binary = root / 'streaming-probe'
flags = ['clang++', '-std=c++17', '-O2', '-g', '-Wall', '-Wextra', '-Werror', '-arch', 'arm64',
         '-mmacosx-version-min=15.0', '-fsanitize=address,undefined', '-fno-omit-frame-pointer',
         '-fno-sanitize-recover=all', '-Inative/deadpan-dsp/src',
         '-isystem', 'native/deadpan-dsp/vendor/signalsmith-stretch/include',
         '-isystem', 'native/deadpan-dsp/vendor/signalsmith-linear/include',
         'native/deadpan-dsp/src/streaming.cpp', 'native/deadpan-dsp/tests/streaming_probe.cpp',
         '-o', str(binary)]
subprocess.run(flags, cwd=repo, check=True)
env = dict(os.environ, ASAN_OPTIONS='detect_leaks=0:halt_on_error=1', UBSAN_OPTIONS='halt_on_error=1:print_stacktrace=1')
result = subprocess.run([str(binary)], cwd=repo, env=env, capture_output=True, text=True)
print(result.stdout, end='')
print(result.stderr, end='')
record = {'command': flags, 'exit': result.returncode,
          'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
          'report': json.loads(result.stdout) if result.returncode == 0 else None,
          'limitation': 'LeakSanitizer is unavailable on this macOS; ASan and UBSan enabled.'}
(root / 'sanitizer-report.json').write_text(json.dumps(record, indent=2) + '\n')
raise SystemExit(result.returncode)
