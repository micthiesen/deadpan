from pathlib import Path
import hashlib,json,shutil,subprocess
r=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
binary=r/'bin';binary.mkdir()
old=Path('/tmp/deadpan-generated-slices-20261011/bin')
for name in ['deadpan-app','deadpan-cli','deadpan-media-worker','deadpan-track','deadpan-transcribe']:
 shutil.copy2((repo/'target/release' if name=='deadpan-app' else old)/name,binary/name)
names=subprocess.check_output(['git','diff','--name-only'],cwd=repo,text=True).splitlines()
names+=subprocess.check_output(['git','ls-files','--others','--exclude-standard'],cwd=repo,text=True).splitlines()
sources=[name for name in names if name.endswith('.rs')]
pins=dict(base_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),binary={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in binary.iterdir()},source={name:hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in sources})
(r/'pins.json').write_text(json.dumps(pins,indent=2)+'\n')
for name in sources:
 p=r/'source'/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes((repo/name).read_bytes())
# Finite regression set. Stop here for implementation fixes if any fails.
for scenario in ['terminal-playback','playback-feedback','original-playback','sound-playback','place-slice','trim','gain','original-layout','original-layout-long']:
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
