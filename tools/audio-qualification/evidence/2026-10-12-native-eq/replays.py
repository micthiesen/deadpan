import subprocess, sys
from pathlib import Path
root = Path(__file__).parent
subprocess.run([sys.executable,str(root/'freeze.py')],check=True)
for name, scenario, mode in [('equalizer','equalizer','visual'),('gain-final','gain','visual'),('audio-treatments','audio-treatments','visual'),('equalizer-performance','equalizer','performance')]:
 subprocess.run([sys.executable,str(root/'run.py'),name,str(root/'bin/deadpan-app'),'--ui-check','--scenario',scenario,'--mode',mode,'--hz','120','--output',str(root/name)],check=True)
