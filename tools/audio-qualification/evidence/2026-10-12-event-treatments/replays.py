import subprocess,sys
from pathlib import Path
s=Path(__file__).resolve().parent
for name,scenario in [('wet-replay-final','wet-returns'),('sound-replay-final','sound-placement'),('tail-replay','tail-sends'),('audio-replay','audio-treatments'),('eq-replay','equalizer')]:
 r=subprocess.run([sys.executable,str(s/'run.py'),name,'target/release/deadpan-app','--ui-check','--scenario',scenario,'--mode','visual','--output',str(s/name),'--retain-projects'])
 if r.returncode:sys.exit(r.returncode)
