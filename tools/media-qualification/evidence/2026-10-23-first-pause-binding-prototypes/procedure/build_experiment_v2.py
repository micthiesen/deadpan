from pathlib import Path
import os,sys,subprocess,time,hashlib,json,shutil
name=sys.argv[1]
s=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
plan=s/'next-profile-plan';out=plan/'builds';out.mkdir(exist_ok=True)
roots={'capture-profile':('prepared-capture-profile-source','capture-profile-target'),'early-fallback':('early-fallback-prototype-source','early-fallback-prototype-target'),'binding-defaults-v2':('binding-defaults-v2-prototype-source','binding-defaults-v2-prototype-target')}
source,target=(plan/n for n in roots[name])
allowed={'capture-profile':{'crates/deadpan-core/src/audio_binding_lifecycle.rs','crates/deadpan-core/src/audio_reference.rs','crates/deadpan-core/src/insert_time.rs','crates/deadpan-core/src/command.rs','crates/deadpan-store/src/lib.rs','crates/deadpan-cli/examples/perf/edit.rs'},'early-fallback':{'crates/deadpan-core/src/audio_reference.rs'},'binding-defaults-v2':{'crates/deadpan-core/src/audio_binding.rs'}}[name]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot(root):
 return {n:sha(root/n)for n in subprocess.check_output(['rg','--files','-g','*.rs'],cwd=root,text=True).splitlines()}
base=json.loads((s/'candidate-all-core-final-source-identities.json').read_text())
frozen=snapshot(source);assert set(frozen)==set(base)
changed={n for n in base if base[n]!=frozen[n]};assert changed==allowed,(name,changed)
inputs={n:sha(source/n)for n in json.loads((s/'baseline-core-inputs.json').read_text())}
assert inputs==json.loads((s/'baseline-core-inputs.json').read_text())
(out/(name+'-sources.json')).write_text(json.dumps(frozen,indent=2)+'\n')
(out/(name+'-inputs.json')).write_text(json.dumps(inputs,indent=2)+'\n')
current=snapshot(Path('/Users/michael/Code/deadpan'))
test_differences={n for n in base if base[n]!=current[n]}
assert test_differences=={'crates/deadpan-core/tests/compact_frozen_edges.rs','crates/deadpan-store/tests/edited_slice/child_capture/range_compat.rs','crates/deadpan-store/tests/revision_storage.rs'},test_differences
env=dict(os.environ,CARGO_TARGET_DIR=str(target),DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
script_digest=sha(Path(__file__))
args=['cargo','build','--locked','--release','-p','deadpan-cli','--example','perf','-j','8'];start=time.monotonic()
with(out/(name+'.stdout')).open('w')as stdout,(out/(name+'.stderr')).open('w')as stderr:
 result=subprocess.run(args,cwd=source,env=env,stdout=stdout,stderr=stderr)
record={'name':name,'source':str(source),'target':str(target),'command':args,'exit':result.returncode,'seconds':time.monotonic()-start,'modified_rust_paths':sorted(changed),'prepared_base_vs_shipping_difference_only_uncompiled_test_files':sorted(test_differences),'script_sha256':script_digest}
(out/(name+'-build.json')).write_text(json.dumps(record,indent=2)+'\n')
assert snapshot(source)==frozen and all(sha(source/n)==h for n,h in inputs.items())
if result.returncode:raise SystemExit(result.returncode)
binary=out/(name+'-perf');shutil.copy2(target/'release/examples/perf',binary)
record['binary']={'path':str(binary),'sha256':sha(binary)}
(out/(name+'-build.json')).write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record),flush=True)
