from pathlib import Path
import hashlib,json,shutil,subprocess
r=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
binary=r/'bin'
binary.mkdir()
old=Path('/tmp/deadpan-generated-loops-20261011/bin-verified')
for name in ['deadpan-app','deadpan-cli','deadpan-media-worker','deadpan-track','deadpan-transcribe']:
 shutil.copy2((repo/'target/release' if name=='deadpan-app' else old)/name,binary/name)
sources=['native/deadpan-source/src/'+p for p in ['decoder.c','decoder.h','lib.rs']]+['crates/deadpan-cli/src/picture.rs','crates/deadpan-cli/src/picture/generated.rs','crates/deadpan-app/src/worker.rs','crates/deadpan-app/src/worker/generated_playback_tests.rs','native/deadpan-source/tests/threaded_seek.rs','crates/deadpan-media/examples/compare_ffv1_threads.rs']
pins=dict(binary={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in binary.iterdir()},source={name:hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in sources})
(r/'playback-pins.json').write_text(json.dumps(pins,indent=2)+'\n')
for name in sources:
 p=r/'source'/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes((repo/name).read_bytes())
fixtures={'1080p':Path('/tmp/deadpan-generated-transitions-20261011/two-holds.deadpan'),'4k':Path('/tmp/deadpan-generated-loops-20261011/4k/two-holds.deadpan'),'long':Path('/tmp/deadpan-generated-playback-performance-20261011/1080p-fallback.deadpan')}
for name,fixture in fixtures.items():
 pins={str(p.relative_to(fixture)):dict(bytes=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in sorted(fixture.rglob('*')) if p.is_file()}
 (r/(name+'-fixture-pins.json')).write_text(json.dumps(pins,indent=2)+'\n')
# Predetermined series; retain every result, including any failure.
cases=[(f'play-1080p-{i}','generated-playback','1080p') for i in range(5)]
cases += [('play-4k','generated-playback','4k'),('loop-1080p','generated-loop','1080p'),('loop-4k','generated-loop','4k'),('play-long','generated-playback','long')]
for name,scenario,size in cases:
 command=['time','-l',str(binary/'deadpan-app'),'--ui-check','--scenario',scenario,'--mode','performance','--hz','120','--project',str(fixtures[size]),'--output',str(r/name)]
 subprocess.run(['python3',str(r/'run.py'),name,*command],check=False)
