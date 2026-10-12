from pathlib import Path
import hashlib,json,os,shutil,subprocess,time
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
copy=root/'single-slot-source'
copy.mkdir()
paths=subprocess.check_output(['git','ls-files','-z','--','Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo','crates','native'],cwd=repo).decode().split('\0')
paths += ['crates/deadpan-playback/src/preparation/queue.rs','crates/deadpan-playback/src/tests/ahead.rs']
for name in sorted(set(paths)-{''}):
 source=repo/name
 if not source.is_file():continue
 dest=copy/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,dest)
queue=copy/'crates/deadpan-playback/src/preparation/queue.rs'
old='pub(crate) const CAPACITY: usize = 30 * 48_000 / BATCH_FRAMES;'
text=queue.read_text();assert text.count(old)==1
queue.write_text(text.replace(old,'pub(crate) const CAPACITY: usize = 1;'))
(root/'control-source.json').write_text(json.dumps(dict(base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),control='Working source copied with prepared FIFO capacity reduced to one; not an exact historical checkout.',queue_sha256=hashlib.sha256(queue.read_bytes()).hexdigest()),indent=2)+'\n')
command=['cargo','test','--locked','-p','deadpan-playback','tests::ahead::a_future_preserve_finishes_before_its_audio_is_heard_and_delivery_is_canonical','--','--exact','--nocapture']
env=dict(os.environ,CARGO_TARGET_DIR=str(root/'single-slot-target'),DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
start=time.monotonic()
with (root/'control.log').open('x') as log:
 result=subprocess.run(command,cwd=copy,env=env,stdout=log,stderr=subprocess.STDOUT)
record=dict(command=command,exit=result.returncode,seconds=time.monotonic()-start,log_sha256=hashlib.sha256((root/'control.log').read_bytes()).hexdigest())
(root/'control.json').write_text(json.dumps(record,indent=2)+'\n')
print(record)
