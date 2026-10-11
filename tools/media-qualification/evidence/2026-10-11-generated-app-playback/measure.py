from pathlib import Path
import subprocess
root=Path(__file__).parent
for name,fixture in [('real-4k','4k-fallback'),('real-1080p','1080p-fallback')]:
 command=['time','-l',str(root/'bin/deadpan-app'),'--ui-check','--scenario','generated-playback','--mode','performance','--hz','120','--project',str(root/(fixture+'.deadpan')),'--output',str(root/name)]
 result=subprocess.run(['python3',str(root/'run.py'),name,*command])
 print(name,result.returncode,flush=True)
