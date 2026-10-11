from pathlib import Path
import gzip,hashlib,json,subprocess
R=Path(__file__).parent
REPO=Path('/Users/michael/Code/deadpan')
OUT=REPO/'tools/media-qualification/evidence/2026-10-11-local-import'
OUT.mkdir(parents=True,exist_ok=True)
compression=[]
def retain(p,rel):
    b=p.read_bytes();dest=OUT/rel
    if len(b)>32768 and p.suffix in ['.log','.stdout','.json']:
        dest=dest.with_suffix(dest.suffix+'.gz');compression.append({'path':str(dest.relative_to(OUT)),'original_bytes':len(b),'original_sha256':hashlib.sha256(b).hexdigest()});b=gzip.compress(b,mtime=0)
    dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(b)
for p in sorted(R.iterdir()):
    if p.is_file() and p.suffix in ['.py','.log','.json','.stdout','.stderr']:
        retain(p,Path('scripts' if p.suffix=='.py' else 'checks')/p.name)
for p in sorted((R/'output').iterdir()):
    if p.is_file():retain(p,Path('output')/p.name)
files=['Cargo.lock','rust-toolchain.toml','native/deadpan-source/tests/fixtures/cfr-bframes.mp4','native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav']
for name in ['single_source','original_media','original_preparation','prepared_relink','bookmarks']:
    files.append('crates/deadpan-store/tests/'+name+'.rs')
identity={'source_revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'binary_sha256':hashlib.sha256((REPO/'target/release/deadpan-cli').read_bytes()).hexdigest(),'files':{name:hashlib.sha256((REPO/name).read_bytes()).hexdigest() for name in files},'scratch':str(R),'environment':{'machine':'Apple M5 Max, 128 GiB','os':'macOS 26.5.2','rust':'1.97.1','ffmpeg':'8.0.3 (qualified-3 prefix)'}}
(OUT/'source-identity.json').write_text(json.dumps(identity,indent=2)+'\n')
(OUT/'compression.json').write_text(json.dumps(compression,indent=2)+'\n')
(OUT/'README.md').write_text('''# Local Original and sound acceptance, 2026-10-11

Source revision and executed binary are in `source-identity.json`. The sole
working agent ran the real release CLI on the reference Mac. No live GUI was
opened. Repository fixtures were read only; fault injection used a scratch WAV.

`checks/` retains commands, stdout/stderr, authored dumps, admission requests,
full quality reports and the 36-test store run. `output/` retains both emitted
movies and their verified publication reports. Large textual files use gzip;
`compression.json` records their original sizes and hashes. `SHA256SUMS`
covers every retained file. The original dump invocation omitted `--json`;
its usage refusal is retained beside the corrected successful command.

The run began with `cargo build --release --locked -p deadpan-cli`, then
`project create-original` from `cfr-bframes.mp4`, `project dump --json`, and
linked retention of a scratch copy of `pcm-stereo-48000.wav`. `scripts/accept.py`
continues from those recorded outputs: register, place, remove, corrupt,
restore, render, copy-portable, remove the external sound and render again.
`scripts/verify.py` checks every emitted picture and three audio windows.
`scripts/admission.py` tests deduplication, explicit container stream selection,
wrong-stream refusal and unchanged picture/mix after catalog-only registration.
The scripts use exclusive log creation; choose a fresh scratch directory when
repeating and replay the recorded setup commands with that directory.

The two portable ownership records are managed, without linked paths. Missing
and corrupt sound renders fail with `render_audio_failed`, confirmed cleanup,
no publication and identical authored state. Both successful renders publish
120 pictures / 192,192 canonical samples. Three quality comparisons pass
360 picture checks, six zero-offset signal windows and three silent windows;
the third comparison reuses the portable movie after catalog-only import.

The portable project and source package remain in the recorded scratch path.
The source package deliberately has its linked sound unavailable; only scratch
files were removed. No clean-machine, provider-eviction or physical-input result
is implied. The qualification document maps the complete DP-15 acceptance scope.
''')
manifest=[]
for p in sorted(OUT.rglob('*')):
    if p.is_file() and p.name!='SHA256SUMS':manifest.append(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(OUT)))
(OUT/'SHA256SUMS').write_text('\n'.join(manifest)+'\n')
print('retained',len(manifest),'files')
