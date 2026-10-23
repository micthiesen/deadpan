from pathlib import Path
import os,json,hashlib,subprocess,time,shutil
s=Path('/tmp/deadpan-first-pause-profile-2026-10-22')
source=s/'source';root=s/'compact-edges-source';target=s/'compact-edges-target'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot(p):
 names=subprocess.check_output(['rg','--files','-g','*.rs'],cwd=p,text=True).splitlines()
 return {n:sha(p/n)for n in names}
base=json.loads((s/'source-identities.json').read_text());assert snapshot(source)==base
shutil.copytree(source,root)
p=root/'crates/deadpan-core/src/audio_reference.rs'
def patch(old,new):
 text=p.read_text();assert text.count(old)==1,(old[:60],text.count(old));p.write_text(text.replace(old,new,1))
patch('    pub duration: FrameDuration,\n    pub edges: AudioEdgePolicies,\n',
 '    pub duration: FrameDuration,\n    #[serde(default, skip_serializing_if = "AudioEdgePolicies::is_automatic")]\n    pub edges: AudioEdgePolicies,\n')
old='''        let edges = match self.edges.iter().find(|(edges, _)| *edges == frozen.edges) {
            Some((_, bytes)) => *bytes,
            None => {
                let bytes = json_len(&frozen.edges)?;
                self.edges.push((frozen.edges, bytes));
                bytes
            }
        };
        self.add(",\\\"edges\\\":".len() + edges)?;'''
new='''        if !frozen.edges.is_automatic() {
            let edges = match self.edges.iter().find(|(edges, _)| *edges == frozen.edges) {
                Some((_, bytes)) => *bytes,
                None => {
                    let bytes = json_len(&frozen.edges)?;
                    self.edges.push((frozen.edges, bytes));
                    bytes
                }
            };
            self.add(",\\\"edges\\\":".len() + edges)?;
        }'''
patch(old,new)
identities=snapshot(root);assert set(identities)==set(base)
assert [n for n in base if base[n]!=identities[n]]==['crates/deadpan-core/src/audio_reference.rs']
(s/'compact-edges-source-identities.json').write_text(json.dumps(identities,indent=2)+'\n')
shutil.copy2(p,s/'compact-edges-overlay.rs.txt')
env=dict(os.environ,CARGO_TARGET_DIR=str(target),DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
cmd=['cargo','build','--locked','--release','-p','deadpan-cli','--example','perf'];started=time.monotonic()
with(s/'compact-edges-build.stdout').open('w')as out,(s/'compact-edges-build.stderr').open('w')as err:r=subprocess.run(cmd,cwd=root,env=env,stdout=out,stderr=err)
receipt={'command':cmd,'cwd':str(root),'target_dir':str(target),'exit':r.returncode,'seconds':time.monotonic()-started,'script_sha256':sha(s/'compact_edges.py'),'overlay_sha256':sha(s/'compact-edges-overlay.rs.txt'),'prototype_only':True,'format_version_unchanged_for_experiment_not_shipping':True}
(s/'compact-edges-build.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True)
if r.returncode:raise SystemExit(r.returncode)
assert snapshot(root)==identities
binary=s/'compact-edges-perf';shutil.copy2(target/'release/examples/perf',binary)
binaries=json.loads((s/'binaries.json').read_text());binaries['instrumented']={'source_path':str(target/'release/examples/perf'),'executed_path':str(binary),'sha256':sha(binary)}
(s/'compact-edges-binaries.json').write_text(json.dumps(binaries,indent=2)+'\n')
# Neither binary has timers. Reuse the load-gated driver and skip phase parsing
# while preserving its paired fresh-package and exact binary checks.
code=(s/'measure.py').read_text().replace("out=s/'controlled'","out=s/'compact-edges-controlled'").replace("s/'binaries.json'","s/'compact-edges-binaries.json'").replace("if mode=='instrumented':","if False:")
(s/'compact-edges-measure.py').write_text(code)
with(s/'compact-edges-driver.log').open('w')as log:r=subprocess.run(['python',str(s/'compact-edges-measure.py')],stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0,r.returncode
assert sha(binary)==binaries['instrumented']['sha256'] and snapshot(root)==identities
print('Compact-edge prototype completed; no shipping or qualification claim',flush=True)
