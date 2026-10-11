from pathlib import Path
import hashlib,json,shutil,subprocess,sys
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
label=sys.argv[1]
binary=root/('bin-'+label)
binary.mkdir()
old=Path('/tmp/deadpan-generated-transitions-20261011/bin')
for name in ['deadpan-app','deadpan-cli','deadpan-media-worker','deadpan-track','deadpan-transcribe']:
    shutil.copy2((repo/'target/release' if name=='deadpan-app' else old)/name,binary/name)
sources=['crates/deadpan-app/src/worker.rs','crates/deadpan-app/src/worker/playback_cache.rs','crates/deadpan-app/src/worker/generated_ahead.rs','crates/deadpan-app/src/worker/provider_ahead.rs','crates/deadpan-app/src/preview/harness.rs','crates/deadpan-app/src/preview/harness/generated_playback.rs','crates/deadpan-app/src/ui_harness.rs']
sources=[name for name in sources if (repo/name).is_file()]
(root/(label+'-pins.json')).write_text(json.dumps(dict(binary={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in binary.iterdir()},source={name:hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in sources}),indent=2)+'\n')
snapshot=root/(label+'-source')
for name in sources:
    p=snapshot/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes((repo/name).read_bytes())
for size,fixture in [('1080p',Path('/tmp/deadpan-generated-transitions-20261011/two-holds.deadpan')),('4k',root/'4k/two-holds.deadpan')]:
    name=label+'-'+size
    command=['time','-l',str(binary/'deadpan-app'),'--ui-check','--scenario','generated-loop','--mode','performance','--hz','120','--project',str(fixture),'--output',str(root/name)]
    subprocess.run(['python3',str(root/'run.py'),name,*command],check=False)
