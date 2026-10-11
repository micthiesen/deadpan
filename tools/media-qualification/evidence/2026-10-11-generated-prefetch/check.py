from pathlib import Path
import gzip,hashlib,json,re,subprocess,sys
repo=Path('/Users/michael/Code/deadpan')
root=repo/'tools/media-qualification/evidence/2026-10-11-generated-prefetch'
def sha(data):return hashlib.sha256(data).hexdigest()
manifest=json.loads((root/'manifest.json').read_text())
assert len({row['path'] for row in manifest})==len(manifest)
raw={}
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
assert sha(Path(identity['binary']).read_bytes())==identity['binary_sha256']
for name,expected in identity['libraries_sha256'].items():
 assert sha(Path(name).read_bytes())==expected,name
assert sha((repo/'native/deadpan-source/tests/fixtures/cfr-bframes.mp4').read_bytes())==identity['fixture_source_sha256']
fixture=Path('/tmp/deadpan-generated-lookahead-20261011/conditioned.deadpan')
for name,pin in json.loads(raw['fixture-pins.json']).items():
 data=(fixture/name).read_bytes()
 assert len(data)==pin['bytes'] and sha(data)==pin['sha256'],name
for name in ['generated-playback','app-worker','media','clippy-2','format-check','resources']:
 result=json.loads(raw[name+'.json'])
 assert result['exit']==0,name
 assert sha(raw[name+'.log'])==result['log_sha256'],name
assert json.loads(raw['clippy.json'])['exit']!=0,'retain the fixed type-complexity failure'
assert '4 passed; 0 failed; 0 ignored' in raw['generated-playback.log'].decode()
links=0
for name in ['docs/REQUIREMENTS.md','docs/PLAYBACK.md','docs/PERFORMANCE.md','docs/spec/AGENT_HANDOFF.md','docs/qualification/generated-prefetch-2026-10-11.md']:
 page=repo/name
 for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)',page.read_text()):
  if ':' in target or target.startswith('#'):continue
  path=target.split('#',1)[0].strip('<>')
  if path:
   assert (page.parent/path).exists(),(name,target)
   links+=1
assert len(re.findall(r'^\| DP-\d+ \|[^\n]+\| Complete \|',(repo/'docs/REQUIREMENTS.md').read_text(),re.M))==8
print(json.dumps(dict(files=len(manifest),sources=len(identity['source_sha256']),links=links)))
