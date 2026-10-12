import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
for name,scenario in [('wet-replay','wet-returns'),('tail-replay','tail-sends'),('sound-replay','sound-placement'),('audio-replay','audio-treatments'),('eq-replay','equalizer')]:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,'target/release/deadpan-app','--ui-check','--scenario',scenario,'--mode','visual','--output',str(s/name),'--retain-projects'])
 if r.returncode:sys.exit(r.returncode)
