from pathlib import Path
import subprocess
root=Path(__file__).parent
for name,fixture in [('verified-4k','4k-fallback'),('verified-1080p','1080p-fallback')]:
 command=['time','-l',str(root/'bin-verified/deadpan-app'),'--ui-check','--scenario','generated-playback','--mode','performance','--hz','120','--project',str(root/(fixture+'.deadpan')),'--output',str(root/name)]
 subprocess.run(['python3',str(root/'run.py'),name,*command],check=True)
for name,scenario in [('control-original','original-playback'),('control-sound','sound-playback')]:
 command=[str(root/'bin-verified/deadpan-app'),'--ui-check','--scenario',scenario,'--mode','performance','--hz','120','--output',str(root/name)]
 subprocess.run(['python3',str(root/'run.py'),name,*command],check=True)
