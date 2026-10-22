from pathlib import Path
import hashlib,json,shutil
s=Path('/tmp/deadpan-this-play-fixed-retime-duration-2026-10-22')
for name,digest in json.loads((s/'baseline-source-identities.json').read_text()).items():
 assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest,name
listing=json.loads((s/'baseline-list.json').read_text())
suites=[(k,v) for k,v in listing['rust-suites'].items() if any(
 'native_this_play_fixed_retime_hold_duration_offers_entry_after_public_isolation_control' in name
 and test['filter-match']['status']=='matches'
 for name,test in v.get('testcases',{}).items())]
assert len(suites)==1
name,suite=suites[0]
p=Path(suite['binary-path'])
t=s/'baseline-bin'/p.name
t.parent.mkdir(exist_ok=True)
shutil.copy2(p,t)
record={'base':'5cd480c6e6a96db4922f6e4c366ea4eb81e7afd9','test':'native_this_play_fixed_retime_hold_duration_offers_entry_after_public_isolation_control','binary':name,'executed':str(p),'retained':str(t),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'scope':'Silent Background Hold structural/availability witness, not real-media qualification.'}
assert hashlib.sha256(t.read_bytes()).hexdigest()==record['sha256']
(s/'baseline.json').write_text(json.dumps(record,indent=2)+'\n')
print('Exact baseline binary retained and hashed; production unchanged from base')
