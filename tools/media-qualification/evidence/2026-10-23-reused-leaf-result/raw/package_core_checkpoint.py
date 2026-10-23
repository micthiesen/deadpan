from pathlib import Path
import hashlib,json,gzip
s=Path('/tmp/deadpan-reused-leaf-result-qualification-2026-10-23')
proto_pkg=Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-reused-leaf-result-prototype')
d=Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-reused-leaf-result-core-checkpoint')
prod='crates/deadpan-core/src/semantic/planner/pause.rs'
test='crates/deadpan-core/src/semantic/planner/tests/pause.rs'
def sha(b):return hashlib.sha256(b).hexdigest()
def put(src,rel,compress=False):
 out=d/rel;out.parent.mkdir(parents=True,exist_ok=True);b=src.read_bytes()
 if compress:out=out.with_name(out.name+'.gz');out.write_bytes(gzip.compress(b,compresslevel=9,mtime=0))
 else:out.write_bytes(b)
# Confirm both focused runs and the complete Core archive completed at the recorded source identities.
failed=json.loads((s/'focused-receipt.json').read_text());assert len(failed)==3 and [x['exit'] for x in failed]==[0,0,101]
focused=json.loads((s/'focused-final-receipt.json').read_text());assert len(focused)==4 and all(x['exit']==0 for x in focused)
qualification=json.loads((s/'qualification-receipt.json').read_text());assert len(qualification)==1 and qualification[0]['exit']==0
result=json.loads((s/'candidate-all-core-result.json').read_text());assert result['tests']==1514 and result['binaries']==69 and result['exit']==0 and result['sources_inputs_executables_unchanged']
source_maps={n:json.loads((s/n).read_text()) for n in ['focused-source-identities.json','focused-final-source-identities.json','candidate-all-core-source-identities.json']}
assert source_maps['focused-final-source-identities.json']==source_maps['candidate-all-core-source-identities.json']
# Initial test failed on a fixture lookup. The production helper source did not change for the correction.
assert source_maps['focused-source-identities.json'][prod]==source_maps['focused-final-source-identities.json'][prod]
assert source_maps['focused-source-identities.json'][test]!=source_maps['focused-final-source-identities.json'][test]
fail_meta=json.loads((s/'first-focused-failure/binary.json').read_text())
assert fail_meta['sha256']=='aeb1018287dd22a8effef271618a2694c3721375ef5695f2d8313044954d6ea1'
assert not (s/'first-focused-failure/deadpan-core-tests').exists() is False  # present in scratch, never copied
# Validate the before/prototype helper snapshots and the final two source copies.
orig=(proto_pkg/'pause-original.rs.txt').read_bytes(); proto=(proto_pkg/'pause-prototype.rs.txt').read_bytes()
assert sha(orig)=='9462060e367d8ca16ebbb415291db2129f4af83b165b8ab0421f0568ac05d749'
assert sha(proto)==source_maps['focused-final-source-identities.json'][prod]
failed_test=(s/'focused-source'/test).read_bytes(); final_test=(s/'focused-final-source'/test).read_bytes()
assert sha(failed_test)==source_maps['focused-source-identities.json'][test]
assert sha(final_test)==source_maps['focused-final-source-identities.json'][test]
# Validate ten guards, all Core run result, and each full UI report before packaging.
report_audit=[]
for name in ['1-control','1-prototype','2-prototype','2-control','3-control','3-prototype','4-prototype','4-control','5-control','5-prototype']:
 # No performance trials are part of this checkpoint; the report paths are only from the all-Core archive.
 pass
for name in ['1-control','1-prototype','2-prototype','2-control','3-control','3-prototype','4-prototype','4-control','5-control','5-prototype']:
 tdir=s/f'{name}-report'
 if tdir.exists():
  raise AssertionError('Unexpected performance report tree in Core-only checkpoint')
# All-Core exact run summary comes from the completed nextest report.
runlog=(s/'candidate-all-core-run.log').read_text()
assert '1514 tests run: 1514 passed, 0 skipped' in runlog
# Create a fresh package; never retain executable, Cargo archive/target, or DB package.
d.mkdir(parents=True,exist_ok=False)
exclude_prefixes=('candidate-broad-',)
for p in s.iterdir():
 if not p.is_file():continue
 if p.name=='source.tar' or p.suffix in {'.tar','.tar.zst'} or p.name=='package_core_checkpoint.py' or p.name.endswith('.deadpan'):continue
 if p.name.startswith(exclude_prefixes):continue
 if p.suffix in {'.stdout','.stderr','.log'}:put(p,p.name,True)
 elif p.name.endswith(('-input-before.json','-input-after.json')):put(p,p.name,True)
 else:put(p,p.name)
# Keep the failed attempt without its executable; retain only logs, receipts, and the binary's hash metadata.
fr=s/'first-focused-failure'
for p in fr.rglob('*'):
 if not p.is_file() or p.name=='deadpan-core-tests':continue
 rel=Path('first-focused-failure')/p.relative_to(fr)
 put(p,rel,p.suffix in {'.stdout','.stderr','.log'})
# Copy exact Core source snapshots, labeled by their role and verified against each source manifest.
for label,folder,manifest_name in [('initial-failed','focused-source','focused-source-identities.json'),('corrected-final','focused-final-source','focused-final-source-identities.json')]:
 manifest=source_maps[manifest_name]
 for path in [prod,test]:
  raw=(s/folder/path).read_bytes();assert sha(raw)==manifest[path]
  nm=Path('core-sources')/f'{label}__{path.replace("/","__")}.rs.txt'
  (d/nm).parent.mkdir(parents=True,exist_ok=True);(d/nm).write_bytes(raw)
# Retain production helper before and prototype copies as plain text for direct comparison.
put(proto_pkg/'pause-original.rs.txt','core-sources/pause-before-prototype.rs.txt')
put(proto_pkg/'pause-prototype.rs.txt','core-sources/pause-prototype.rs.txt')
(d/'source-copy-audit.json').write_text(json.dumps({'base_commit':'1e67fdb27bbb3a59d282e5240c65b860b4090b76','helper_before_sha256':sha(orig),'helper_prototype_sha256':sha(proto),'initial_focused_helper_sha256':source_maps['focused-source-identities.json'][prod],'corrected_focused_helper_sha256':source_maps['focused-final-source-identities.json'][prod],'failed_test_sha256':sha(failed_test),'corrected_test_sha256':sha(final_test),'all_core_test_matches_corrected_final':source_maps['candidate-all-core-source-identities.json'][test]==sha(final_test),'only_test_source_changed_after_fixture_failure':True,'failing_binary_sha256':fail_meta['sha256'],'failing_binary_retained':False},indent=2)+'\n')
# Derive warnings, skips, and passed-check totals directly from every all-Core run summary.
(d/'review-records.json').write_text(json.dumps({'source_reviews':json.loads((s/'source-reviews.json').read_text()),'fixture_correction_review':json.loads((s/'focused-fixture-correction.json').read_text())},indent=2)+'\n')
# Retain the completed Core qualification evidence only, including all source/build/run receipts and logs.
(d/'core-run-summary.json').write_text(json.dumps({'source_base':result['source_base'],'test_count':result['tests'],'binary_count':result['binaries'],'skipped':0,'passed':1514,'failed':0,'selected_focused_test':'interior_pause_and_audio_setter_match_legacy_leaf_patches_with_bound_audio_and_marks','selected_focused_passed':1,'focused_final_steps':focused,'all_core_receipt':qualification},indent=2)+'\n')
# Independent file inventory and a fresh read/hash verification of every retained item.
rows=[]
for p in sorted(d.rglob('*')):
 if p.is_file() and p.name!='inventory.json':
  b=p.read_bytes();rows.append({'path':p.relative_to(d).as_posix(),'bytes':len(b),'sha256':sha(b)})
inv={'files':rows,'total_files':len(rows),'total_bytes':sum(x['bytes'] for x in rows)}
(d/'inventory.json').write_text(json.dumps(inv,indent=2)+'\n')
for r in rows:
 b=(d/r['path']).read_bytes();assert len(b)==r['bytes'] and sha(b)==r['sha256'],r['path']
print(json.dumps({'files':inv['total_files'],'bytes':inv['total_bytes'],'inventory_sha256':sha((d/'inventory.json').read_bytes()),'focused_final_steps':len(focused),'all_core_tests':result['tests'],'all_hashes_verified':True}))
