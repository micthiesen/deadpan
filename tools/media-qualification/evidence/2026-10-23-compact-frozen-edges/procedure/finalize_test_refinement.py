from pathlib import Path
import subprocess,json,hashlib,time,os
s=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
root=Path('/Users/michael/Code/deadpan')
env=dict(os.environ,CARGO_PROFILE_DEV_OPT_LEVEL='2',DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
rows=[]
def run(label,args,expected=0):
 start=time.monotonic()
 with(s/(label+'.stdout')).open('w')as out,(s/(label+'.stderr')).open('w')as err:
  r=subprocess.run(args,cwd=root,env=env,stdout=out,stderr=err)
 row={'command':args,'exit':r.returncode,'seconds':time.monotonic()-start}
 rows.append(row)
 (s/'refined-tests-receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
 if r.returncode!=expected:raise SystemExit(r.returncode or 1)
# Close out runtime-media guards before the test-only source refinement.
run('portable-final',['python',str(s/'retain_media_and_check.py')])
target=root/'crates/deadpan-core/tests/compact_frozen_edges.rs'
before=json.loads((s/'candidate-broad-final-source-identities.json').read_text())
assert sha(target)==before[str(target.relative_to(root))]
draft=s/'final-test-refinement/compact_frozen_edges.rs.txt'
assert sha(draft)=='84e4c6a448cd71622a3ab4e901df90f2dd5bfff29caa4bf6f007146b93d1bcdb'
(s/'final-test-refinement/previous.rs.txt').write_bytes(target.read_bytes())
target.write_bytes(draft.read_bytes())
run('refined-format',['cargo','fmt','--all'])
after={n:sha(root/n)for n in subprocess.check_output(['rg','--files','-g','*.rs'],cwd=root,text=True).splitlines()}
assert set(after)==set(before)
changed=[n for n in before if before[n]!=after[n]]
assert changed==['crates/deadpan-core/tests/compact_frozen_edges.rs'],changed
(s/'refined-test-source-attribution.json').write_text(json.dumps({'changed_only':changed,'all_production_and_other_tests_unchanged':True,'before_sha256':before[changed[0]],'after_sha256':after[changed[0]],'cause':'Asymmetric policies and fixed historical codec bytes from immutable a18 library replace self-derived expectations.'},indent=2)+'\n')
run('refined-core',['python',str(s/'qualify.py'),'refined-core','candidate','core'])
run('refined-clippy',['cargo','clippy','--locked','-p','deadpan-core','--all-targets','--','-D','warnings'])
run('refined-fmt-check',['cargo','fmt','--all','--','--check'])
# Link the final test source against the old, already-qualified Core library.
deps=s/'baseline-target/debug/deps'
libraries={name:list(deps.glob('lib'+name+'-*.rlib'))for name in ['deadpan_core','serde_json']}
assert all(len(paths)==1 for paths in libraries.values()),libraries
digests={name:sha(paths[0])for name,paths in libraries.items()}
witness=s/'historical-codec-probe/refined-old-tests'
args=['rustc','--edition','2024','--test',str(target),'-L','dependency='+str(deps),'-o',str(witness)]
for name,paths in libraries.items():args+=['--extern',name+'='+str(paths[0])]
run('refined-old-build',args)
run('refined-old-run',[str(witness),'--test-threads','1'],101)
text=(s/'refined-old-run.stdout').read_text()
assert '4 passed; 2 failed; 0 ignored' in text,text
assert 'automatic_edges_are_omitted_and_restore_exactly ... FAILED' in text
assert 'source_and_repeat_edge_policies_survive_a_mixed_layout_roundtrip ... FAILED' in text
assert 'standalone_context_preserves_historical_codec_bytes_for_all_schemas ... ok' in text
assert all(sha(paths[0])==digests[name]for name,paths in libraries.items())
(s/'refined-old-witness.json').write_text(json.dumps({'source_test_sha256':sha(target),'library_sha256':digests,'executed_binary_sha256':sha(witness),'expected_omission_failures':2,'controls_passed':4,'old_libraries_unchanged':True},indent=2)+'\n')
