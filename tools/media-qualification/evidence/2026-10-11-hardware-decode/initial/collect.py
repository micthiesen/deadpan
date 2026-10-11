from pathlib import Path
import gzip,hashlib,json,re,subprocess
R=Path(__file__).parent
REPO=Path('/Users/michael/Code/deadpan')
OUT=REPO/'tools/media-qualification/evidence/2026-10-11-hardware-decode'
OUT.mkdir(parents=True,exist_ok=True)
manifest=[]
def keep(src,relative):
 data=src.read_bytes(); relative=Path(relative)
 original_sha=hashlib.sha256(data).hexdigest()
 if len(data)>4096 and relative.suffix in ['.log','.json','.jsonl','.stderr','.txt']:
  data=gzip.compress(data,mtime=0);relative=Path(str(relative)+'.gz')
 dest=OUT/relative;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
 manifest.append(dict(path=str(relative),sha256=hashlib.sha256(data).hexdigest(),bytes=len(data),original_sha256=original_sha,source=str(src)))
for f in sorted(R.iterdir()):
 if f.is_file() and f.suffix in ['.py','.c','.json','.log','.txt','.stderr','.jsonl']:
  keep(f,Path('initial')/f.name)
for version in ['v2','v3']:
 for f in sorted((R/version).iterdir()):
  if f.is_file() and f.suffix in ['.py','.c','.json','.log','.stderr','.jsonl']:
   keep(f,Path('probes')/version/f.name)
for f in sorted((R/'final').iterdir()):
 if f.is_file() and f.name!='compare_decode': keep(f,Path('final')/f.name)
for name in ['report.json','test.log']:
 p=R/'sanitizers'/name
 if p.exists():keep(p,Path('sanitizers')/name)
# Retain the sanitizer report's actual filename rather than assuming one.
for p in (R/'sanitizers').glob('*.json'):
 if p.name!='report.json':keep(p,Path('sanitizers')/p.name)
source=(R/'final/identity.json').read_text();identity=json.loads(source)
for name,expected in identity['source_sha256'].items():
 assert hashlib.sha256((REPO/name).read_bytes()).hexdigest()==expected,name
pins={}
for logname in ['final-source-media-tests-2.log','final-admission-tests.log','final-native-hardware.log']:
 for raw in re.findall(r'Running .*?\((target/[^)]+)\)',(R/logname).read_text()):
  path=REPO/raw
  if path.is_file():pins[raw]=hashlib.sha256(path.read_bytes()).hexdigest()
(R/'executed-test-binaries.json').write_text(json.dumps(pins,indent=2)+'\n')
keep(R/'executed-test-binaries.json','executed-test-binaries.json')
fixture_names=set(re.findall(r'"([a-z0-9-]+\.(?:mp4|webm|mkv|mov))"',(REPO/'native/deadpan-source/tests/hardware_decode.rs').read_text()))
fixtures={name:hashlib.sha256((REPO/'native/deadpan-source/tests/fixtures'/name).read_bytes()).hexdigest() for name in sorted(fixture_names)}
(R/'committed-fixture-pins.json').write_text(json.dumps(fixtures,indent=2)+'\n')
keep(R/'committed-fixture-pins.json','committed-fixture-pins.json')
readme="""# Optional VideoToolbox decode evidence, 2026-10-11

See [qualification](../../../../docs/qualification/hardware-decode-2026-10-11.md).
`manifest.json` records SHA-256 of every retained file and its uncompressed
original. `SHA256SUMS` also covers the manifest and this README. Large logs
and JSON are gzip-compressed deterministically. Source/test fixture pins and
executed binary hashes are retained separately from performance claims.

The final comparison binary and all four generated movies remain at
`/tmp/deadpan-hardware-decode-20261011`; `final/identity.json`, generated
fixture recipes and their hashes identify them. The final reports retain
every canonical pixel hash/PTS/duration and all 800 random target/anchor
comparisons. Reproduce with `compare_decode` and the recorded pinned prefix.
The generator's Homebrew FFmpeg is not the decoder library.

`initial/` includes the early prototype, its full-range normalization mismatch,
corrected native runs, failed initial example build, formatting correction and
all earlier timings. `probes/v2` and `probes/v3` preserve the corrected public
FFmpeg comparisons and software-layout observations. `sanitizers/` instruments
the native adapter, not FFmpeg or Apple frameworks. One agent implemented and
reviewed this change; the review notes record the callback-ownership correction.

No app default enables hardware decoding. Correctness passed; long-GOP
interactive latency failed. These reports do not close DP-16 or DP-24.
"""
(OUT/'README.md').write_text(readme)
(OUT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
entries=[]
for p in sorted(OUT.rglob('*')):
 if p.is_file() and p.name!='SHA256SUMS':entries.append(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(OUT)))
(OUT/'SHA256SUMS').write_text('\n'.join(entries)+'\n')
print(json.dumps({'manifest_files':len(manifest),'total_files':len(entries)+1,'bytes':sum(p.stat().st_size for p in OUT.rglob('*') if p.is_file())}))
