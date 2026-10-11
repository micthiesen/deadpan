from pathlib import Path
import json,gzip,hashlib,re,subprocess
repo=Path('/Users/michael/Code/deadpan');scratch=Path('/tmp/deadpan-generated-export-20261011')
root=repo/'tools/media-qualification/evidence/2026-10-11-real-generated-export'
def sha(data):return hashlib.sha256(data).hexdigest()
raw={};manifest=json.loads((root/'manifest.json').read_text())
assert len({row['path'] for row in manifest})==len(manifest)
for row in manifest:
 data=(root/row['path']).read_bytes();assert len(data)==row['bytes'] and sha(data)==row['sha256'],row['path']
 plain=gzip.decompress(data) if row['path'].endswith('.gz') else data
 assert sha(plain)==row['original_sha256'],row['path']
 raw[row['path'].removesuffix('.gz')]=plain
identity=json.loads(raw['identity.json'])
for name,expected in identity['binary_sha256'].items():assert sha((Path(identity['binary_dir'])/name).read_bytes())==expected,name
for name,expected in identity['source_sha256'].items():
 assert sha((repo/name).read_bytes())==expected,name
 assert sha(raw['source/'+name])==expected,name
for name,expected in identity['libraries_sha256'].items():assert sha(Path(name).read_bytes())==expected,name
for name,pin in json.loads(raw['artifact-pins.json']).items():
 data=(scratch/name).read_bytes();assert len(data)==pin['bytes'] and sha(data)==pin['sha256'],name
for label,step in json.loads(raw['chirp-v2-steps.json']).items():
 assert step['exit']==0,label
 assert sha(raw[label+'.stdout'])==step['stdout_sha256'],label
 assert sha(raw[label+'.stderr'])==step['stderr_sha256'],label
for name,rate in [('chirp-v2-4k',30),('chirp-v2-1080p',60)]:
 v=json.loads(raw[name+'-verification.json']);assert v['passed'] and not v['failures']
 assert v['summary']['pictures_checked']==rate*5//2
 assert v['summary']['offset_verified_windows']==3
 assert v['summary']['nonzero_offsets']==v['summary']['offset_unobservable_windows']==0
 assert v['movie']['expected_audio_samples']==120000
 assert [p['project_frame'] for p in v['pictures']]==list(range(rate*5//2))
 for p in v['pictures']:
  n=p['project_frame'];generated=rate<=n<rate*3//2
  expected=n-rate if generated else n if n<rate else n-rate//2
  assert p['provenance']['kind']==('generated' if generated else 'original') and p['provenance']['source_frame']==expected
 for label in ['early-chirp','silent-hold','shifted-chirp']:
  a=json.loads(raw[name+'-'+label+'.stdout'])['audio'];peak=max(abs(x) for frame in a['samples'] for x in frame)
  assert len(a['samples'])==256 and (peak==0 if label=='silent-hold' else peak>0.5)
 assert json.loads(raw[name+'-accepted.stdout'])==json.loads(raw[name+'-after.stdout'])
negative=json.loads(raw['negative-fallback-report.json']);assert not negative['passed'] and negative['summary']['pictures_failed']==15
assert json.loads(raw['negative-fallback.json'])['exit']==1
v=json.loads(raw['4k-verification.json']);assert not v['passed'] and v['summary']['pictures_failed']==0
assert v['summary']['offset_unobservable_windows']==19 and all('audio_offset_unobservable' in f for f in v['failures'])
assert json.loads(raw['chirp-steps.json'])['chirp-4k-create']['exit']==1
assert b'unsupported_transfer' in raw['chirp-4k-create.stderr']
links=0
for name in ['docs/REQUIREMENTS.md','docs/PREVIEW_EXPORT_VERIFICATION.md','docs/spec/AGENT_HANDOFF.md','docs/qualification/real-generated-export-2026-10-11.md']:
 page=repo/name
 for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)',page.read_text()):
  if ':' in target or target.startswith('#'):continue
  relative=target.split('#',1)[0].strip('<>')
  if relative:assert (page.parent/relative).exists(),(name,target);links+=1
assert len(re.findall(r'^\| DP-\d+ \|[^\n]+\| Complete \|',(repo/'docs/REQUIREMENTS.md').read_text(),re.M))==8
print(json.dumps(dict(files=len(manifest),pictures=225,zero_offset_windows=6,negative_pictures=15,links=links)))
