from pathlib import Path
import hashlib
import json
import subprocess
import time

root = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-follow-creep-2026-10-24')
sources = json.loads((scratch / 'source-files.json').read_text())
def hashes():
    return {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in sources}
before = hashes()
command = ['cargo', 'fmt', '--all', '--', '--check']
started = time.time_ns()
with (scratch / 'format-verified.stdout').open('xb') as stdout, (scratch / 'format-verified.stderr').open('xb') as stderr:
    result = subprocess.run(command, cwd=root, stdout=stdout, stderr=stderr)
after = hashes()
receipt = {
    'command': command, 'cwd': str(root), 'started_unix_ns': started,
    'finished_unix_ns': time.time_ns(), 'exit_code': result.returncode,
    'source_hashes_before': before, 'source_hashes_after': after,
    'sources_unchanged': before == after,
    'streams': {name: {'sha256': hashlib.sha256((scratch / name).read_bytes()).hexdigest(),
                       'bytes': (scratch / name).stat().st_size}
                for name in ['format-verified.stdout', 'format-verified.stderr']},
    'scope': 'Separate post-publication formatting execution; primary package is unchanged.',
}
with (scratch / 'format-verified-receipt.json').open('x') as output:
    json.dump(receipt, output, indent=2)
    output.write('\n')
print(json.dumps({'exit_code': result.returncode, 'sources_unchanged': before == after}))
assert before == after
raise SystemExit(result.returncode)
