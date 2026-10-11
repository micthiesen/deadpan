from pathlib import Path
import hashlib,json,subprocess
r=Path(__file__).parent
binary=r/'bin'
for scenario in ['original-layout','original-layout-long']:
 command=[str(binary/'deadpan-app'),'--ui-check','--scenario',scenario,'--mode','performance','--hz','120','--output',str(r/scenario)]
 subprocess.run(['python3',str(r/'run.py'),scenario,*command],check=True)
fixtures={'1080p':Path('/tmp/deadpan-generated-transitions-20261011/two-holds.deadpan'),'4k':Path('/tmp/deadpan-generated-loops-20261011/4k/two-holds.deadpan'),'long1080p':Path('/tmp/deadpan-generated-playback-performance-20261011/1080p-fallback.deadpan'),'long4k':Path('/tmp/deadpan-generated-playback-performance-20261011/4k-fallback.deadpan')}
for name,fixture in fixtures.items():
 pins={str(p.relative_to(fixture)):dict(bytes=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in sorted(fixture.rglob('*')) if p.is_file()}
 (r/(name+'-fixture-pins.json')).write_text(json.dumps(pins,indent=2)+'\n')
# Predetermined series. Every result is retained, including failures.
cases=[(f'play-long1080p-{i}','generated-playback','long1080p') for i in range(3)]
cases += [('play-long4k','generated-playback','long4k'),('play-1080p','generated-playback','1080p'),('play-4k','generated-playback','4k'),('loop-1080p','generated-loop','1080p')]
(r/'cases.json').write_text(json.dumps(cases,indent=2)+'\n')
for name,scenario,size in cases:
 command=['time','-l',str(binary/'deadpan-app'),'--ui-check','--scenario',scenario,'--mode','performance','--hz','120','--project',str(fixtures[size]),'--output',str(r/name)]
 subprocess.run(['python3',str(r/'run.py'),name,*command],check=False)
