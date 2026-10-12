import pathlib,subprocess,sys
s=pathlib.Path(__file__).resolve().parent
commands=[
 ('fmt-final',['cargo','fmt','--all']),
 ('native-tests2',['cargo','test','--locked','-p','deadpan-encode','--','--test-threads=1']),
 ('cli-encoded',['cargo','test','--locked','-p','deadpan-cli','--lib','encoded_render','--','--test-threads=1']),
 ('clippy',['cargo','clippy','--locked','--workspace','--all-targets','--features','ui-harness','--','-D','warnings']),
 ('release-native',['cargo','test','--locked','--release','-p','deadpan-encode','--','--test-threads=1']),
 ('release-preview',['env','DEADPAN_PREVIEW_EXPORT_KEEP=1','DEADPAN_PARTIAL_REPEAT_RESULTS='+str(s/'release-preview.json'),'cargo','test','--locked','--release','-p','deadpan-cli','--test','preview_export','--','--test-threads=1']),
 ('release-app',['cargo','build','--locked','--release','-p','deadpan-app','-p','deadpan-cli']),
 ('fmt-check',['cargo','fmt','--all','--check']),
]
for name,command in commands:
 print('Starting '+name,flush=True)
 result=subprocess.run(['python3',str(s/'run.py'),name,*command])
 if result.returncode: sys.exit(result.returncode)
