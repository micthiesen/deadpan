from pathlib import Path
import os,subprocess
r=Path(__file__).parent
commands=[
 ('final-source-media-tests-2',['cargo','test','--locked','--release','-p','deadpan-source','-p','deadpan-media','--','--test-threads=4']),
 ('workspace-clippy',['cargo','clippy','--locked','--workspace','--all-targets','--','-D','warnings']),
 ('sanitizers',['python3','tools/media-qualification/host/build_sanitized.py','--work',str(r/'sanitizers'),'--package','deadpan-source']),
 ('compare-final-build',['cargo','build','--locked','--release','-p','deadpan-media','--example','compare_decode']),
 ('format-check',['cargo','fmt','--all','--check']),
]
for name,args in commands:
 print('START',name,flush=True)
 subprocess.run(['python3',str(r/'run.py'),name,*args],check=True)
