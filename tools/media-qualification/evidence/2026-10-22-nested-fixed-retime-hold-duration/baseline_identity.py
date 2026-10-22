from pathlib import Path
import hashlib,json,shutil
s=Path('/tmp/deadpan-nested-fixed-retime-duration-2026-10-22')
for name,digest in json.loads((s/'baseline-source-identities.json').read_text()).items():
 assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest,name
listing=json.loads((s/'baseline-list.json').read_text())
test='preview::scoped::model::tests::nested_fixed_retime_duration::native_this_play_nested_fixed_retime_hold_duration_offers_entry_after_public_control'
suites=[(k,v) for k,v in listing['rust-suites'].items() if v.get('testcases',{}).get(test,{}).get('filter-match',{}).get('status')=='matches']
assert len(suites)==1
name,suite=suites[0]; p=Path(suite['binary-path']); target=s/'baseline-bin'/p.name
target.parent.mkdir(exist_ok=True); shutil.copy2(p,target)
row={'base':'95ddb9aebe8d001f32b01e4cd3d9abff0a471585','test':test,'suite':name,'executed':str(p),'retained':str(target),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'scope':'Two nested explicit Repeat bodies, silent structural/public-isolation/raw-duration witness; not audio or media qualification.'}
assert hashlib.sha256(target.read_bytes()).hexdigest()==row['sha256']
(s/'baseline.json').write_text(json.dumps(row,indent=2)+'\n')
print('Exact baseline binary retained/hashed; production unchanged from base')
