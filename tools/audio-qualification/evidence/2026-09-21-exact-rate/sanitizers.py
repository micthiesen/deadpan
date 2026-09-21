import datetime
import json
import os
from pathlib import Path
import subprocess

repo = Path('/Users/michael/Code/deadpan')
output = Path('/tmp/deadpan-exact-rate-20260921/sanitizers')
output.mkdir(parents=True, exist_ok=True)
environment = dict(os.environ)
environment['PATH'] = str(output) + os.pathsep + environment['PATH']
environment['ASAN_OPTIONS'] = 'detect_leaks=0:halt_on_error=1'
environment['UBSAN_OPTIONS'] = 'halt_on_error=1:print_stacktrace=1'
report = {'schema_version': 1, 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
          'checks': [], 'status': 'running',
          'limitations': ['LeakSanitizer is unsupported on this macOS platform; AddressSanitizer and UndefinedBehaviorSanitizer remain enabled.']}
flags = ['clang++', '-std=c++17', '-O2', '-g', '-Wall', '-Wextra', '-Werror', '-arch', 'arm64',
         '-mmacosx-version-min=15.0', '-fsanitize=address,undefined', '-fno-omit-frame-pointer',
         '-fno-sanitize-recover=all', '-Inative/deadpan-dsp/src',
         '-isystem', 'native/deadpan-dsp/vendor/signalsmith-stretch/include',
         '-isystem', 'native/deadpan-dsp/vendor/signalsmith-linear/include',
         'native/deadpan-dsp/src/adapter.cpp']
def run(name, command):
    result = subprocess.run(command, cwd=repo, env=environment, capture_output=True)
    (output / (name + '.stdout')).write_bytes(result.stdout)
    (output / (name + '.stderr')).write_bytes(result.stderr)
    report['checks'].append({'name': name, 'command': command, 'exit_code': result.returncode})
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(name, result.returncode, flush=True)
    if result.returncode:
        raise RuntimeError(result.stderr.decode())
try:
    for name in ['abi_probe', 'exact_rate_probe']:
        run(name + '-build', flags + ['native/deadpan-dsp/tests/' + name + '.cpp', '-o', str(output / name)])
        run(name, [name])
    report['status'] = 'passed'
except Exception as error:
    report['status'] = 'failed'
    report['error'] = repr(error)
    raise
finally:
    report['completed_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
