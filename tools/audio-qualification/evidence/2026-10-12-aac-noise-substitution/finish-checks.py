import pathlib,subprocess,sys
s=pathlib.Path(__file__).resolve().parent
commands=[
 ('fmt-jobs',['cargo','fmt','--all']),
 ('jobs-admission',['cargo','test','--locked','-p','deadpan-jobs','render::admission','--','--nocapture']),
 ('clippy-final',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('release-preview2',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1','DEADPAN_PARTIAL_REPEAT_RESULTS='+str(s/'release-preview2.json'),'cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','--','--test-threads=1']),
 ('release-app',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli']),
 ('fmt-check',['cargo','fmt','--all','--check']),
]
for name,command in commands:
 print('Starting '+name,flush=True)
 result=subprocess.run(['python3',str(s/'run.py'),name,*command])
 if result.returncode: sys.exit(result.returncode)
