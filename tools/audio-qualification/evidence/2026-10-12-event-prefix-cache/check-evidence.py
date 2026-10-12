import argparse,gzip,hashlib,json,re
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',action='store_true');args=p.parse_args()
here=Path(__file__).resolve().parent;repo=here.parents[3]
def read(n):return json.loads((here/n).read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
manifest=read('manifest.json')
assert set(manifest)=={p.name for p in here.iterdir() if p.is_file() and p.name!='manifest.json'}
for n,d in manifest.items():assert sha(here/n)==d,n
commands={r['name']:r for r in read('commands.json')}
logs={}
for n,r in commands.items():
 raw=gzip.decompress((here/(n+'.log.gz')).read_bytes())
 assert hashlib.sha256(raw).hexdigest()==r['log_sha256'],n
 logs[n]=raw.decode()
for n in ['fmt','audio-playback','clippy','release-loop','release-tail','release','export']:
 assert commands[n]['exit']==0,n
for n in ['loops-bounded','loops-cached','loops-trace']:
 assert commands[n]['exit']!=0 and 'plan work per read' in logs[n],n
for n in ['release-loop','release-tail']:
 assert '1 passed; 0 failed' in logs[n],n
results=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',logs['audio-playback'])
assert read('test-counts.json')==dict(passed=sum(int(x[0]) for x in results),ignored=sum(int(x[2]) for x in results),failed=0,result_groups=len(results))
assert read('test-counts.json')['passed']==706 and read('test-counts.json')['ignored']==0
report=read('export-results.json')['report']
assert report['passed'] and not report['failures']
assert report['summary']['pictures_checked']==30 and report['summary']['blocks_compared']==4
assert report['summary']['offset_verified_windows']==1 and report['summary']['nonzero_offsets']==0
assert all(x['passed'] and x['offset_status']=='verified_zero' for x in report['audio'])
if args.source:
 for n,d in read('identity.json')['sources'].items():assert sha(repo/n)==d,n
print('Event prefix evidence verified'+(' against current source' if args.source else ''))
