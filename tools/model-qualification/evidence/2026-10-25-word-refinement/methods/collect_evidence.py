from pathlib import Path
import hashlib,json,shutil,tarfile
R=Path('/Users/michael/Code/deadpan')
S=Path('/tmp/deadpan-word-refinement-2026-10-25')
D=R/'tools/model-qualification/evidence/2026-10-25-word-refinement'
D.mkdir(parents=True,exist_ok=False)
def copy(p,rel=None):
    if not p.is_file():return
    out=D/(rel if rel is not None else p.relative_to(S))
    out.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(p,out)
for p in sorted(S.iterdir()):
    if p.is_file() and p.name.startswith(('refinement-','native-port-','phonetic-')) and p.suffix in ('.json','.stdout','.stderr'):
        if p.suffix=='.json':
            v=json.loads(p.read_text())
            if isinstance(v,dict) and 'command' in v:
                assert 'exit_code' in v,p
        copy(p,Path('runs')/p.name)
for name in ('run.py','qualify_refinement_bundle.py','qualify_refinement_bundle.v1.py','verify_packaged_acceptance.py','durable-ort-prefix.json','launch-review-disposition.json','source-files.json','removed-files.json'):
    copy(S/name,Path('methods')/name)
for rel in ('native-port-parity-third','native-check-parity'):
    for p in (S/rel).glob('*.json'):copy(p)
for p in (S/'native-port-parity-third').glob('*.f*le'):copy(p)
ref=S/'phoneme/native-reference'
for p in ref.rglob('*'):
    if not p.is_file():continue
    rel=p.relative_to(ref)
    if rel.as_posix() in ('constants/mean.f32le','constants/inverse-stddev.f32le'):continue
    if p.suffix in ('.json','.py','.md','.f32le','.f64le'):copy(p)
for p in (S/'phoneme/candidate-v2').rglob('*.json'):copy(p)
for rel in ('phoneme/candidate-freeze.json','phoneme/download-pins.json','phoneme/inspection.json','phoneme/environment-freeze.txt','phoneme/align.py','phoneme/develop.py','phoneme/fresh_evaluate.py','phoneme/fresh_infer.py','phoneme/summarize.py','phoneme-summary-first.json'):
    copy(S/rel)
for name in ('ort-bootstrap-final','ort-bootstrap-lock-review','signature-layout'):
    for p in (S/name).rglob('*'):
        if p.is_file() and p.suffix in ('.json','.stdout','.stderr','.py'):
            copy(p)
for name in ('native-visual-final','native-playback-final'):
    for p in (S/name).iterdir():
        if p.is_file() and p.suffix in ('.json','.html','.png'):copy(p)
for name in ('bundle-cases with spaces','bundle-cases-second with spaces','packaged-acceptance'):
    for p in (S/name).iterdir():
        if p.is_file() and p.suffix in ('.json','.stdout','.stderr'):copy(p)
for p in (S/'workflow').iterdir():
    if p.is_file() and p.suffix in ('.mp4','.json'):copy(p)
# All asset bytes are the synthetic qualified fixture, never checkpoint/statistics.
with tarfile.open(D/'workflow/Source.deadpan.tar.gz','w:gz') as tar:
    tar.add(S/'workflow/Source.deadpan',arcname='Source.deadpan',filter=lambda info: None if info.name.endswith(('.sqlite-shm','.sqlite-wal')) else info)
verify_text=(S/'refinement-bundle-verify-first.stdout').read_text()
verify_root=Path(verify_text.split('verify: kept ')[-1].strip())
for p in (verify_root/'work/Exports').iterdir():
    if p.is_file():copy(p,Path('bundle-verification')/p.name)
for name in ('build-provenance.json','Notices/sbom.cdx.json'):
    p=S/'bundle-second/Deadpan.app/Contents/Resources'/name
    copy(p,Path('bundle-verification')/Path(name).name)
copy(S/'bundle-second/Deadpan.app.SHA256SUMS',Path('bundle-verification/SHA256SUMS'))
files=json.loads((S/'source-files.json').read_text())
source={f:{'bytes':(R/f).stat().st_size,'sha256':hashlib.sha256((R/f).read_bytes()).hexdigest()} for f in files if not f.startswith('docs/')}
(D/'code-inputs.json').write_text(json.dumps({'base_revision':'32b0620c','package_build_revision':'346bda75','scope':'Final changed code/build/model/notice inputs. Documents and progress data are updated after verification. Complete dependency source is identified by the base revision and Cargo.lock.','files':source},indent=2)+'\n')
(D/'legacy-helper-trust-audit.json').write_text(json.dumps({'status':'open','criterion':'DP-22.3','score':0.5,'source_revision':'32b0620c','workers':{
'deadpan-transcribe':['crates/deadpan-cli/src/transcription.rs:45','crates/deadpan-cli/src/transcription.rs:318'],
'deadpan-track':['crates/deadpan-cli/src/tracking.rs:52','crates/deadpan-cli/src/tracking.rs:494','crates/deadpan-cli/src/generation/runtime.rs:562'],
'deadpan-media-worker':['crates/deadpan-cli/src/proxy.rs:527','crates/deadpan-cli/src/youtube/acquire.rs:1378','crates/deadpan-cli/src/generation/runtime.rs:555']},
'launchers':['crates/deadpan-jobs/src/process.rs:394','native/deadpan-process/src/spawn.rs:18'],
'finding':'Build-time signing exists; no global launch verifier or compiled image/library pins cover these three existing native workers.',
'thread_constraints':'Proxy selection currently runs on UI; tracking/proposal and AI runtime selection runs on the project writer. Put expensive qualification at preparation/CLI launch boundaries, not those early selectors.'},indent=2)+'\n')
(D/'README.md').write_text('''# Word-refinement qualification evidence

Primary Mac: Apple M5 Max, macOS 26.5.2, Rust 1.97.1.
The Markdown qualification is docs/qualification/word-refinement-2026-10-25.md.

The runs directory retains exact invocations, selected environment, changed-source
and executed-binary digests, stdout/stderr, failures and bounded continuations.
A receipt with changed source is not described as a fresh full-tree pass.
Intermediate source digests are retained; not every failed intermediate source
tree is published. Final changed code/build input identities are in code-inputs.json.

The 24 native numerical/path cases are observed development/parity data, not an
unseen accuracy split. Historical heldout directory names describe an earlier
experiment stage. The stricter retention and foreign-energy targets remain failed.
Human phonetic/listening judgment is an owner check under specification 29.1.

Original model weights, original/raw learned normalization statistics, ORT/Python
runtime binaries, model-pack installations and developer environments are omitted.
The two normalization constant payloads named by native-reference/manifest.json
are intentionally absent. Use the independently pinned original pack inputs to
regenerate them. Deterministic Hann/Mel constants, measured PCM/features/emissions,
algorithm scripts, hashes and notices remain. No archive or pickle executes in
the application. The source fixture and pictures are synthesized workflow input.

The first bundle failed on stale signature bytes after forced re-signing. Clean
signing preserved strict validation. The first negative-case driver failed while
copying restricted system file flags; its source/trace and the corrected data-only
substitution driver are retained. Neither failed invocation is relabeled green.

Three pre-existing native-worker launch-trust paths remain open separately under
DP-22.3. The verified alignment worker does not close that gap. Launch admission
does not claim containment against a concurrent same-user executable writer.
''')
allfiles=sorted(p for p in D.rglob('*') if p.is_file())
assert not any(p.suffix in ('.ckpt','.whl','.dylib','.onnx') for p in allfiles)
inventory={'schema':'deadpan-word-refinement-evidence-1','files':[{'path':p.relative_to(D).as_posix(),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in allfiles]}
(D/'inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
print(json.dumps({'files':len(allfiles),'bytes':sum(p.stat().st_size for p in allfiles),'inventory_sha256':hashlib.sha256((D/'inventory.json').read_bytes()).hexdigest()}))
