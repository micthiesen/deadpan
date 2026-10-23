from pathlib import Path
import subprocess,time,json,hashlib
s=Path('/tmp/deadpan-neutral-ungroup-2026-10-22');phase='candidate-broad'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
binaries=json.loads((s/(phase+'-binaries.json')).read_text())
helpers=json.loads((s/(phase+'-helpers.json')).read_text())
def guard():
 subprocess.run(['python',str(s/'guard.py'),phase],check=True) if (s/(phase+'-lock.json')).exists() else None
 current={p:sha(p)for p in subprocess.check_output(['rg','--files','-g','*.rs'],text=True).splitlines()}
 assert current==json.loads((s/(phase+'-source-identities.json')).read_text())
 assert all(sha(p)==v for p,v in json.loads((s/(phase+'-inputs.json')).read_text()).items())
 assert all(sha(r['path'])==r['sha256']for r in binaries.values())
 assert all(sha(r['path'])==r['sha256']and sha(r['source'])==r['sha256']for r in helpers.values())
guard()
command=['cargo','nextest','run','--archive-file',str(s/(phase+'.tar.zst')),'--extract-to',str(s/(phase+'-extracted')),'--extract-overwrite','--workspace-remap',str(Path.cwd()),'-E','test(finite_wet::tests::admission_includes_native_plan_and_temporary_kernel_and_network)','--test-threads','1','--no-fail-fast']
start=time.monotonic()
with(s/'broad-isolated-leak.log').open('w')as log:r=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
guard()
row={'command':command,'exit':r.returncode,'seconds':time.monotonic()-start,'test_binary':binaries['deadpan-audio'],'all_original_sources_inputs_and_binaries_unchanged':True,'leak_marker_present':' LEAK 'in(s/'broad-isolated-leak.log').read_text()}
(s/'broad-isolated-leak-receipt.json').write_text(json.dumps(row,indent=2)+'\n')
print(json.dumps({k:v for k,v in row.items()if k!='test_binary'}),flush=True)
raise SystemExit(r.returncode)
