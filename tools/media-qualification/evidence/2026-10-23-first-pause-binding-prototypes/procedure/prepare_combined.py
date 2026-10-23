from pathlib import Path
import json,hashlib,shutil,subprocess
p=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22/next-profile-plan')
source=p/'binding-defaults-v2-prototype-source';dest=p/'combined-prototype-source';target=p/'combined-prototype-target'
def sha(q):return hashlib.sha256(q.read_bytes()).hexdigest()
def snap(root):return {n:sha(root/n)for n in subprocess.check_output(['rg','--files','-g','*.rs'],cwd=root,text=True).splitlines()}
expected=json.loads((p/'builds/binding-defaults-v2-sources.json').read_text());assert snap(source)==expected
assert not dest.exists() and not target.exists()
shutil.copytree(source,dest);target.mkdir()
q=dest/'crates/deadpan-core/src/audio_reference.rs'
anchor='        if durations.len() != document.nodes().len() {\n            return Ok(None);\n        }\n'
code=q.read_text();assert code.count(anchor)==1
code=code.replace(anchor,anchor+'        // At most one missing alias cannot reduce the scoped node count.\n        if required.len() >= document.nodes().len().saturating_sub(1) {\n            return Ok(None);\n        }\n')
q.write_text(code)
base=json.loads((p.parent/'candidate-all-core-final-source-identities.json').read_text());actual=snap(dest)
changed=sorted(n for n in base if base[n]!=actual[n])
assert changed==['crates/deadpan-core/src/audio_binding.rs','crates/deadpan-core/src/audio_reference.rs']
assert snap(source)==expected
(p/'combined-sources.json').write_text(json.dumps(actual,indent=2)+'\n')
for n in changed:
 path=p/'combined-overlays'/(n+'.txt');path.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(dest/n,path)
(p/'combined-preparation.json').write_text(json.dumps({'prototype_only':True,'changed_rust':changed,'base_immutable':True,'wire_versions_unchanged_for_disposable_experiment':True,'cargo_invoked':False},indent=2)+'\n')
