from pathlib import Path
import gzip,hashlib,json,re,subprocess,sys
R=Path(__file__).parent
REPO=Path('/Users/michael/Code/deadpan')
REL=Path('tools/media-qualification/evidence/2026-10-11-hardware-decode')
OUT=REPO/REL
committed='--committed' in sys.argv
def read(path):
 if committed:return subprocess.check_output(['git','show','HEAD:'+str(path)],cwd=REPO)
 return (REPO/path).read_bytes()
def retained(name):
 relative=str(name)
 if relative in seen:return read(REL/relative)
 compressed=relative+'.gz';assert compressed in seen,relative
 return gzip.decompress(read(REL/compressed))
manifest=json.loads(read(REL/'manifest.json'))
seen=set()
for row in manifest:
 assert row['path'] not in seen,row['path'];seen.add(row['path'])
 data=read(REL/row['path'])
 assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256'],row
 plain=gzip.decompress(data) if row['path'].endswith('.gz') else data
 assert hashlib.sha256(plain).hexdigest()==row['original_sha256'],row
for line in read(REL/'SHA256SUMS').decode().splitlines():
 expected,name=line.split('  ',1)
 assert hashlib.sha256(read(REL/name)).hexdigest()==expected,name
identity=json.loads(retained(Path('final/identity.json')))
for name,expected in identity['source_sha256'].items():
 assert hashlib.sha256(read(Path(name))).hexdigest()==expected,name
for name,expected in json.loads(retained(Path('committed-fixture-pins.json'))).items():
 assert hashlib.sha256(read(Path('native/deadpan-source/tests/fixtures')/name)).hexdigest()==expected,name
pictures=0;seeks=0
for name in ['1080p60','4k30','hevc-4k','vp9-4k']:
 d=json.loads(retained(Path('final')/(name+'.json')))
 assert d['pictures']==len(d['reference_pictures'])
 assert all(b['pts']>a['pts'] for a,b in zip(d['reference_pictures'],d['reference_pictures'][1:]))
 assert len(d['cases'])==2 and [c['prefer_videotoolbox'] for c in d['cases']]==[False,True]
 for c in d['cases']:
  assert len(c['seeks'])==100
  assert all(0<=s['anchor']<=s['target']<d['pictures'] for s in c['seeks'])
  times=sorted(s['milliseconds'] for s in c['seeks'])
  assert c['seek_ms']['p95']==times[94]
  assert (c['work']['hardware_frames']>0)==c['prefer_videotoolbox']
  if c['prefer_videotoolbox']: assert c['work']['hardware_transfers']==d['pictures']+100
  pictures+=d['pictures'];seeks+=100
 assert d['cases'][1]['seek_ms']['p95']>d['cases'][0]['seek_ms']['p95']
 movie=R/(name+'.mp4');assert hashlib.sha256(movie.read_bytes()).hexdigest()==d['sha256']
assert (pictures,seeks)==(5760,800)
runs=json.loads(retained(Path('final/runs.json')))
assert len(runs)==4 and all(r['exit']==0 and r['load_before'][0]<=4 for r in runs)
assert all(r['binary_sha256']==identity['binary_sha256'] for r in runs)
assert hashlib.sha256((R/'final/compare_decode').read_bytes()).hexdigest()==identity['binary_sha256']
for name in ['final-source-media-tests-2','final-admission-tests','final-native-hardware','final-clippy','final-compare-build','final-format-check','sanitizers']:
 d=json.loads(retained(Path('initial')/(name+'.json')));assert d['exit']==0,name
 log=retained(Path('initial')/(name+'.log'));assert hashlib.sha256(log).hexdigest()==d['log_sha256'],name
requirements=read(Path('docs/REQUIREMENTS.md')).decode()
assert sum(' | Complete | ' in line for line in requirements.splitlines() if line.startswith('| DP-'))==8
links=0
for name in ['docs/REQUIREMENTS.md','docs/spec/AGENT_HANDOFF.md','docs/PERFORMANCE.md','native/deadpan-source/README.md','docs/qualification/hardware-decode-2026-10-11.md']:
 text=read(Path(name)).decode();assert 'filled from the completed reports' not in text
 for target in re.findall(r'\]\(([^)]+)\)',text):
  target=target.split('#')[0]
  if target and '://' not in target:
   assert (REPO/Path(name).parent/target).resolve().exists(),(name,target)
   links+=1
print(json.dumps({'committed':committed,'manifest_files':len(manifest),'matching_long_clip_pictures':pictures,'matching_random_seeks':seeks,'links':links,'completed_requirements':8}))
