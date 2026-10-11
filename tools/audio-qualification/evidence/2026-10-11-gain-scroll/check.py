from pathlib import Path
import gzip,hashlib,json
r=Path(__file__).parent;repo=Path('/Users/michael/Code/deadpan');root=repo/'tools/audio-qualification/evidence/2026-10-11-gain-scroll'
def sha(data):return hashlib.sha256(data).hexdigest()
def read(p):return json.loads(p.read_bytes())
for name,pin in read(r/'gestures-source-pins.json').items():
 assert sha((repo/name).read_bytes())==pin
 assert sha((r/'source/gestures-gain.rs').read_bytes())==pin
counts={}
for name,pins in [('gestures-visual','gestures-pins'),('release-60','release-pins'),('release-120','release-pins')]:
 result=read(r/(name+'.json'));assert result['exit']==0,name
 assert sha((r/(name+'.log')).read_bytes())==result['log_sha256'],name
 report=read(r/name/'report.json');assert report['metadata']['binary_sha256']==read(r/(pins+'.json'))['deadpan-app']
 for scenario in report['scenarios']:
  assert all(c['passed'] for c in scenario['checks']),name
  assert all(f['severity']!='failure' for f in scenario['findings']),name
 s=next(s for s in report['scenarios'] if s['name']=='gain')
 assert len(s['checks'])==296 and len(s['steps'])<6000,name
 wheels=[c for c in s['checks'] if c['name'].startswith('A discrete mouse wheel')]
 assert len(wheels)==2 and all(c['actual']['delta']==-24 for c in wheels),name
 circuits=[c for c in s['checks'] if c['name'].startswith('A complete keyboard circuit')]
 assert len(circuits)==4 and all(c['actual']['completed'] for c in circuits),name
 counts[name]=dict(checks=len(s['checks']),frames=len(s['steps']))
for name in ['clippy','release-build','format-check']:assert read(r/(name+'.json'))['exit']==0,name
for name,expected in [('diagnose-visual','Gain graph did not fit'),('fixed-visual','Scenario exceeded the 6000-frame trace budget')]:
 report=read(r/name/'report.json');assert read(r/(name+'.json'))['exit']==1,name
 assert any(f['message'].startswith(expected) and f['severity']=='failure' for s in report['scenarios'] for f in s['findings']),name
if (root/'manifest.json').exists():
 for item in read(root/'manifest.json'):
  data=(root/item['path']).read_bytes();assert len(data)==item['bytes'] and sha(data)==item['sha256'],item['path']
  if item['path'].endswith('.gz'):data=gzip.decompress(data)
  assert sha(data)==item['original_sha256'],item['path']
print(json.dumps(counts))
