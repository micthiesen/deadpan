from pathlib import Path
import gzip, hashlib, json, shutil, subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
out = repo / 'tools/media-qualification/evidence/2026-10-11-loop-lookahead'
out.mkdir(exist_ok=True)
def sha(data): return hashlib.sha256(data).hexdigest()
records = []
def keep(path, name):
    raw = path.read_bytes()
    data = gzip.compress(raw, mtime=0) if len(raw) > 12000 else raw
    if len(raw) > 12000: name += '.gz'
    dest = out / name
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(data)
    records.append(dict(path=name, sha256=sha(data), bytes=len(data), original_sha256=sha(raw), source=str(path)))
for path in sorted(root.iterdir()):
    if path.is_file() and path.name != 'fixture-pins.json' and path.suffix in ['.py', '.json', '.log', '.txt']:
        keep(path, path.name)
for path in sorted((root / 'measurements').iterdir()):
    if path.is_file() and path.suffix in ['.json', '.log', '.txt']:
        keep(path, 'measurements/' + path.name)
for path in sorted((root / 'before-tail-fix').rglob('*.rs')):
    keep(path, str(path.relative_to(root)))
pins = repo / 'tools/media-qualification/evidence/2026-10-11-hardware-decode/final/library-pins.json'
keep(pins, 'library-pins.json')
fixtures = {}
for name in ['pyramid-bframes.mp4', 'offset-bframes.mp4', 'vfr.mp4']:
    path = repo / 'native/deadpan-source/tests/fixtures' / name
    fixtures[str(path.relative_to(repo))] = sha(path.read_bytes())
for name in ['1080p60.mp4', '4k30.mp4']:
    path = Path('/tmp/deadpan-hardware-decode-20261011') / name
    fixtures[str(path)] = sha(path.read_bytes())
(root / 'fixture-pins.json').write_text(json.dumps(fixtures, indent=2) + '\n')
keep(root / 'fixture-pins.json', 'fixture-pins.json')
(out / 'manifest.json').write_text(json.dumps(records, indent=2) + '\n')
print(json.dumps(dict(files=len(records), bytes=sum(row['bytes'] for row in records))))
