from pathlib import Path
s=Path('/tmp/deadpan-retime-20260927')
p=Path('/tmp/deadpan-original-audition-20260927/final-checks.py').read_text()
p=p.replace('/tmp/deadpan-original-audition-20260927', str(s)).replace('gate-2/report.json','gate-1/report.json').replace("json.loads((s/'stable-before-tests.json').read_text())['source_sha256']", "json.loads((s/'gate-1/source-before-tests.json').read_text())").replace('original-playback','retime')
(s/'final-checks.py').write_text(p)
