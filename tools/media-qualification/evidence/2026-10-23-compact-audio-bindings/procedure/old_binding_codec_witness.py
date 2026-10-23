from pathlib import Path
import subprocess,json,hashlib,time
s=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23')
p=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22/next-profile-plan')
out=s/'old-binding-codec-witness';out.mkdir(exist_ok=False)
deps=p/'early-fallback-prototype-target/release/deps'
source=Path('/Users/michael/Code/deadpan/crates/deadpan-core/tests/compact_audio_bindings.rs')
def sha(f):return hashlib.sha256(f.read_bytes()).hexdigest()
libraries={}
args=['rustc','+1.97.1','--edition','2024','-C','lto=thin','--crate-name','old_binding_codec_witness','--test',str(source),'-L','dependency='+str(deps),'-o',str(out/'tests')]
for name in ['deadpan_core','serde_json']:
 paths=list(deps.glob('lib'+name+'-*.rlib'));assert len(paths)==1
 libraries[name]={'path':str(paths[0]),'sha256':sha(paths[0])}
 args+=['--extern',name+'='+str(paths[0])]
frozen=json.loads((p/'builds/early-fallback-sources.json').read_text())
base=json.loads((Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')/'candidate-all-core-final-source-identities.json').read_text())
binding='crates/deadpan-core/src/audio_binding.rs'
assert frozen[binding]==base[binding]==sha(p/'early-fallback-prototype-source'/binding)
assert sha(source)==json.loads((s/'candidate-all-core-final-source-identities.json').read_text())['crates/deadpan-core/tests/compact_audio_bindings.rs']
(out/'source.rs.txt').write_bytes(source.read_bytes())
rows=[]
for name,cmd,expected in [('build',args,0),('omission',['./tests','--exact','six_exact_binding_defaults_omit_and_restore_with_fixed_wire_bytes','--test-threads','1'],101),('nondefault-control',['./tests','--exact','signed_nonzero_resume_values_and_phase_terms_remain_explicit_and_exact','--test-threads','1'],0)]:
 start=time.monotonic()
 with(out/(name+'.stdout')).open('w')as stdout,(out/(name+'.stderr')).open('w')as stderr:r=subprocess.run(cmd,cwd=out,stdout=stdout,stderr=stderr)
 rows.append({'name':name,'command':cmd,'exit':r.returncode,'expected_exit':expected,'seconds':time.monotonic()-start})
 record={'source_sha256':sha(source),'libraries':libraries,'prior_binding_source_matches_qualified_core57':True,'comparison_library':'early-fallback prototype, Core57 codec unchanged; only audio_reference has unrelated guard','commands':rows}
 (out/'receipt.json').write_text(json.dumps(record,indent=2)+'\n')
 assert r.returncode==expected,rows[-1]
record['binary_sha256']=sha(out/'tests')
(out/'receipt.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps({'omission_expected_failure':True,'nondefault_control_pass':True,'source_sha256':sha(source),'binary_sha256':record['binary_sha256']}))
