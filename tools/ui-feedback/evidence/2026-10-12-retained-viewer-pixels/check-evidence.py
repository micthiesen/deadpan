import argparse,gzip,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',action='store_true');args=p.parse_args()
here=Path(__file__).resolve().parent;repo=here.parents[3]
def read(n):return json.loads((here/n).read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
manifest=read('manifest.json')
assert set(manifest)=={p.name for p in here.iterdir() if p.is_file() and p.name!='manifest.json'}
for n,d in manifest.items():assert sha(here/n)==d,n
commands={r['name']:r for r in read('commands.json')}
for n,r in commands.items():
 raw=gzip.decompress((here/(n+'.log.gz')).read_bytes())
 assert hashlib.sha256(raw).hexdigest()==r['log_sha256']
for n in ['debug-regression-build','debug-pixel-replay','release','clippy','fmt','replay']:
 assert commands[n]['exit']==0,n
for n in ['debug-pixel-replay','replay']:
 report=json.loads(gzip.decompress((here/(n+'-report.json.gz')).read_bytes()))
 assert not report['failed'] and all(not c['failed'] for c in report['scenarios'])
 case=next(c for c in report['scenarios'] if c['name']=='sound-placement')
 assert len(case['checks'])==273
 pixels=[c['actual']['primary_pixels'] for c in case['checks'] if isinstance(c['actual'],dict) and 'primary_pixels' in c['actual']]
 assert len(pixels)==2 and all(v>10 for counts in pixels for v in counts)
for row in read('archived-pixels.json'):
 assert row['primary_pixels']==[2860,2860,2912]
 assert row['displayed']=='Showing sequence frame 3'
identity=read('identity.json')
assert commands['replay']['binaries']['target/release/deadpan-app']==identity['binaries']['target/release/deadpan-app']
if args.source:
 for n,d in identity['sources'].items():assert sha(repo/n)==d,n
 for row in read('archived-pixels.json'):assert sha(repo/row['png'])==row['sha256']
print('Retained viewer pixel evidence verified'+(' against current source' if args.source else ''))
