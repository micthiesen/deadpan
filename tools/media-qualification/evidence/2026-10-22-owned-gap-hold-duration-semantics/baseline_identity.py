from pathlib import Path
import hashlib,json,shutil
s=Path('/tmp/deadpan-owned-gap-hold-duration-next-2026-10-21')
for name,digest in json.loads((s/'baseline-source-identities.json').read_text()).items():
 assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest,name
listing=json.loads((s/'baseline-list.json').read_text())
suites=[(k,v) for k,v in listing['rust-suites'].items() if any(
 'native_mixed_owned_gap_hold_duration_offers_entry_after_public_controls' in name
 and test['filter-match']['status']=='matches'
 for name,test in v.get('testcases',{}).items())]
assert len(suites)==1
name,suite=suites[0]
p=Path(suite['binary-path'])
t=s/'baseline-bin'/p.name
t.parent.mkdir(exist_ok=True)
shutil.copy2(p,t)
record={'base':'9b0c9e620f5a56af37ed5b7d7535acdae7e168a6','test':'native_mixed_owned_gap_hold_duration_offers_entry_after_public_controls','binary':name,'executed':str(p),'retained':str(t),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'scope':'Silent Background Hold structural/availability witness, not real-media qualification.'}
assert hashlib.sha256(t.read_bytes()).hexdigest()==record['sha256']
(s/'baseline.json').write_text(json.dumps(record,indent=2)+'\n')
print('Exact baseline binary retained and hashed; production unchanged from base')
