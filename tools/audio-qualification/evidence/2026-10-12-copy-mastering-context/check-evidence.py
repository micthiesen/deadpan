import argparse, gzip, hashlib, json
from pathlib import Path
p=argparse.ArgumentParser()
p.add_argument('--source',action='store_true')
args=p.parse_args()
here=Path(__file__).resolve().parent
repo=here.parents[3]
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def read(name): return json.loads((here/name).read_text())
manifest=read('manifest.json')
assert set(manifest)=={p.name for p in here.iterdir() if p.is_file() and p.name!='manifest.json'}
for name,digest in manifest.items(): assert sha(here/name)==digest,name
commands={r['name']:r for r in read('commands.json')}
logs={}
for name,r in commands.items():
    raw=gzip.decompress((here/(name+'.log.gz')).read_bytes())
    assert hashlib.sha256(raw).hexdigest()==r['log_sha256'],name
    logs[name]=raw.decode()
for name in ['original-copy-before','mixed-copy-before','fmt-check','debug','clippy','release']:
    assert commands[name]['exit']==0,name
assert commands['clippy-before']['exit']!=0 and 'does not contain this feature' in logs['clippy-before']
assert '1 passed; 0 failed' in logs['debug']
assert '2 passed; 0 failed' in logs['release']
debug=read('debug-result.json')
release=read('release-result.json')
assert {k:v for k,v in debug.items() if k!='package'}=={k:v for k,v in release.items() if k!='package'}
assert release['max_authored_difference']==release['max_gain_prediction_error']==0
assert release['frames_compared']==256 and release['channels']==2
assert release['before_min_gain']==1 and 0<release['after_min_gain']<0.99
assert release['preceding_original_peak']>0.9 and release['max_limited_difference']>0.001
export=read('export-result.json')['report']
assert export['passed'] and export['summary']['nonzero_offsets']==0
assert export['summary']['pictures_checked']==7
if args.source:
    for name,digest in read('identity.json')['sources'].items(): assert sha(repo/name)==digest,name
print('Copy mastering evidence verified'+(' against source' if args.source else ''))
