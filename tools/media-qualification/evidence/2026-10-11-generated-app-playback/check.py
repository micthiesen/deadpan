from pathlib import Path
import gzip,hashlib,json,re,subprocess,sys
repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-generated-app-playback'
def sha(data):return hashlib.sha256(data).hexdigest()
manifest=json.loads((root/'manifest.json').read_text()); raw={}
assert len({row['path'] for row in manifest})==len(manifest)
for row in manifest:
 data=(root/row['path']).read_bytes()
 assert len(data)==row['bytes'] and sha(data)==row['sha256'],row['path']
 plain=gzip.decompress(data) if row['path'].endswith('.gz') else data
 assert sha(plain)==row['original_sha256'],row['path']
 raw[row['path'].removesuffix('.gz')]=plain
identity=json.loads(raw['identity.json'])
for name,expected in identity['source_sha256'].items():
 data=subprocess.check_output(['git','show','HEAD:'+name],cwd=repo) if '--committed' in sys.argv else (repo/name).read_bytes()
 assert sha(data)==expected,name
 assert sha(raw['source/'+name])==expected,name
for name,expected in identity['binary_sha256'].items():
 assert sha((Path(identity['binary_dir'])/name).read_bytes())==expected,name
for name,expected in identity['libraries_sha256'].items():
 assert sha(Path(name).read_bytes())==expected,name
for name in ['4k-fallback','1080p-fallback']:
 fixture=Path(identity['binary_dir']).parent/(name+'.deadpan')
 for relative,pin in json.loads(raw[name+'-pins.json']).items():
  data=(fixture/relative).read_bytes()
  assert len(data)==pin['bytes'] and sha(data)==pin['sha256'],relative
for name in ['4k-fixture','real-generate','real-accept','1080p-lengthen','1080p-real-generate','1080p-real-accept','harness-tests','telemetry-tests','telemetry-clippy','telemetry-format-check','telemetry-build','telemetry-fixed','verified-4k','verified-1080p','control-original','control-sound']:
 result=json.loads(raw[name+'.json'])
 assert result['exit']==0,name
 assert sha(raw[name+'.log'])==result['log_sha256'],name
assert json.loads(raw['1080p-fixture.json'])['exit']!=0
assert b'ideal native frame count lies outside the provider capability' in raw['1080p-fixture.log']
assert json.loads(raw['telemetry-old.json'])['exit']==101
assert b'12 passed; 2 failed' in raw['telemetry-old.log']
assert b'14 passed; 0 failed' in raw['telemetry-tests.log']
assert b'6 passed; 0 failed' in raw['harness-tests.log']
for name,frames,generated in [('verified-4k',615,15),('verified-1080p',1830,30)]:
 summary=json.loads(raw[name+'/generated-playback.json'])
 measured=json.loads(raw['measurements.json'])[name]
 assert summary['frames']==frames and summary['generated_frames']==generated and summary['ended']
 assert summary['missing_frames']==[]
 assert {p['frame'] for p in summary['pictures']}==set(range(frames))
 assert summary['audio']['underruns']==summary['audio']['faults']==0
 assert summary['native_picture_counts']['skipped']==summary['native_picture_counts']['reduced']==0
 assert measured['request_to_composed_ms']['count']==measured['worker_ms']['count']==frames
 assert len(measured['transitions'])==2
for name in ['verified-4k','verified-1080p','control-original','control-sound']:
 report=json.loads(raw[name+'/report.json'])
 assert report['metadata']['binary_sha256']==identity['binary_sha256']['deadpan-app']
 for scenario in report['scenarios']:
  assert all(check['passed'] for check in scenario['checks']),name
  assert all(f['severity']!='failure' for f in scenario['findings']),name
links=0
for name in ['docs/REQUIREMENTS.md','docs/PERFORMANCE.md','docs/PLAYBACK.md','docs/UI_FEEDBACK.md','docs/spec/AGENT_HANDOFF.md','docs/qualification/generated-app-playback-2026-10-11.md']:
 page=repo/name
 for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)',page.read_text()):
  if ':' in target or target.startswith('#'):continue
  path=target.split('#',1)[0].strip('<>')
  if path:
   assert (page.parent/path).exists(),(name,target)
   links+=1
assert len(re.findall(r'^\| DP-\d+ \|[^\n]+\| Complete \|',(repo/'docs/REQUIREMENTS.md').read_text(),re.M))==8
print(json.dumps(dict(files=len(manifest),sources=len(identity['source_sha256']),links=links)))
