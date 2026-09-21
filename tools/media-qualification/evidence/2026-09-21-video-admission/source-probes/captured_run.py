import json
import pathlib
import subprocess
import tempfile

repo = pathlib.Path('/Users/michael/Code/deadpan')
work = pathlib.Path(tempfile.mkdtemp(prefix='deadpan-admission-source-probes-'))
pathlib.Path('/tmp/deadpan-admission-source-probes-current.txt').write_text(str(work))
base = pathlib.Path('/tmp/deadpan-media-compatible-probe-8efkj8h2')
sources = {name: base / f'{name}.mp4' for name in (
    'software-cfr', 'software-no-b', 'software-vfr-no-b', 'software-offset', 'hardware-no-b'
)}
sources['generated-ffv1'] = pathlib.Path('/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/deadpan-durable-acceptance-q1rh36r4/accepted/native.mkv')
results = []
for name, source in sources.items():
    report = work / f'{name}.json'
    with (work / f'{name}.log').open('w') as log:
        result = subprocess.run([str(repo / 'target/debug/examples/inspect_source'), str(source), str(report)],
            cwd=repo, stdout=log, stderr=subprocess.STDOUT, timeout=360)
    results.append({'name': name, 'exit_code': result.returncode, 'report': str(report)})
    print(json.dumps(results[-1]), flush=True)
(work / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
print(str(work), flush=True)
raise SystemExit(int(any(row['exit_code'] for row in results)))
