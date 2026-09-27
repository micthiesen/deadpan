"""Record the required local gate without suppressing any test failures."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

repo = Path('/Users/michael/Code/deadpan')
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=False)
env = dict(os.environ, DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-media-compatible-xyhilms4/prefix')
commands = [
    ['cargo', 'fmt', '--all', '--', '--check'],
    ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings'],
    ['cargo', 'test', '--workspace', '--locked', '--no-fail-fast'],
    ['cargo', 'build', '--workspace', '--locked'],
    ['cargo', 'run', '--locked', '-p', 'deadpan-cli', '--', 'doctor'],
]

def seal():
    paths = subprocess.check_output([
        'git', '-c', 'core.fsmonitor=false', 'ls-files', '-z',
        '--cached', '--others', '--exclude-standard',
    ], cwd=repo).decode().split('\0')
    selected = sorted(set(p for p in paths if p and (
        p.startswith(('crates/', 'native/'))
        or p in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'rustfmt.toml')
    )))
    return {p: hashlib.sha256((repo / p).read_bytes()).hexdigest()
            for p in selected if (repo / p).is_file()}

before = seal()
(out / 'source-before.json').write_text(json.dumps(before, indent=2) + '\n')
report = {'commands': [], 'source_count': len(before)}
for index, command in enumerate(commands):
    print(json.dumps({'started': index, 'command': command}), flush=True)
    started = time.monotonic()
    with (out / f'{index}.log').open('w') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    record = {'command': command, 'exit_code': result.returncode,
              'seconds': time.monotonic() - started, 'log': f'{index}.log'}
    report['commands'].append(record)
    print(json.dumps(record), flush=True)
    # Preserve formatting failure while allowing independent compilation/tests
    # to run during concurrent UI work. A failed compile still stops the gate.
    # Test failures remain visible while build/doctor run independently.
    if result.returncode and index == 1:
        break
after = seal()
(out / 'source-after.json').write_text(json.dumps(after, indent=2) + '\n')
report['source_unchanged'] = before == after
report['changed_sources'] = [p for p in sorted(set(before) | set(after))
                             if before.get(p) != after.get(p)]
if (out / '2.log').exists():
    counts = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',
                        (out / '2.log').read_text())
    report['tests'] = dict(zip(('passed', 'failed', 'ignored'),
                              (sum(int(row[i]) for row in counts) for i in range(3))))
report['passed'] = (len(report['commands']) == len(commands)
                    and all(c['exit_code'] == 0 for c in report['commands'])
                    and report['source_unchanged'])
(out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
sys.exit(0 if report['passed'] else 1)
