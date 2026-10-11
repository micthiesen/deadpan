from pathlib import Path
import gzip,hashlib,json,re,shutil,subprocess
R=Path(__file__).parent
REPO=Path('/Users/michael/Code/deadpan')
OUT=REPO/'tools/media-qualification/evidence/2026-10-11-source-operations'
summary=json.loads((R/'matrix-summary.json').read_text());ROOT=Path(summary['root'])
OUT.mkdir(parents=True,exist_ok=True)
compressed=[]
def keep(source,relative):
    b=source.read_bytes();p=OUT/relative
    if len(b)>32768 and source.suffix in ['.log','.json']:
        p=p.with_suffix(p.suffix+'.gz');compressed.append({'path':str(p.relative_to(OUT)),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()});b=gzip.compress(b,mtime=0)
    p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b)
for p in sorted(R.iterdir()):
    if p.is_file() and p.suffix in ['.py','.log','.json','.rs','.txt']:
        keep(p,Path('scripts' if p.suffix=='.py' else 'checks')/p.name)
artifacts=[]
for p in sorted(ROOT.rglob('*')):
    if p.is_file():artifacts.append({'path':str(p.relative_to(ROOT)),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
for p in sorted(ROOT.glob('*/*/verification.json')):keep(p,Path('runs')/p.relative_to(ROOT))
for p in sorted(ROOT.glob('*/*/exports/*')):
    if p.is_file() and p.suffix in ['.json','.mp4']:keep(p,Path('runs')/p.relative_to(ROOT))
files=['Cargo.lock','rust-toolchain.toml','crates/deadpan-cli/tests/preview_export.rs','crates/deadpan-cli/tests/preview_export/recipes.rs','crates/deadpan-cli/tests/preview_export/formats.rs']
fixtures=sorted({row['source'] for row in summary['rows']})
files += ['native/deadpan-source/tests/fixtures/'+name for name in fixtures]
for file in ['generate_fixtures.py','generate_vfr_terminal_fixture.py','generate_webm_fixtures.py','generate_av1_fixtures.py','generate_prores_fixtures.py']:
    p=REPO/'native/deadpan-source/tests'/file
    if p.exists():files.append(str(p.relative_to(REPO)))
binaries=['target/release/deadpan-cli','target/release/deps/preview_export-748171a0b2c2a124']
for name in binaries:shutil.copy2(REPO/name,R/('final-'+Path(name).name))
identity={'base_revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'files':{name:hashlib.sha256((REPO/name).read_bytes()).hexdigest() for name in files},'executed_binaries':{name:hashlib.sha256((REPO/name).read_bytes()).hexdigest() for name in binaries},'note':'Only the three listed Rust test files differ from the base revision; runtime source and fixture bytes are unchanged. Release run 2 is final.','scratch':str(R),'environment':{'machine':'Apple M5 Max 128 GiB','os':'macOS 26.5.2','rust':'1.97.1','ffmpeg':'8.0.3, qualified-3 with libopus/dav1d'}}
(OUT/'source-identity.json').write_text(json.dumps(identity,indent=2)+'\n')
(OUT/'artifact-inventory.json').write_text(json.dumps(artifacts,indent=2)+'\n')
(OUT/'compression.json').write_text(json.dumps(compressed,indent=2)+'\n')
(OUT/'README.md').write_text('''# Source-format combined edits, 2026-10-11

The real release test creates 40 profiled projects from 20 retained source
fixtures. Each uses typed cuts, a six-frame Retime of Original [4,8), two
plays with a three-frame silent gap, and a three-frame freeze at Edit 22.
Both Preserve and Tape pitch policies produce exactly 26 output pictures.
Expected source ordinals come from fixture-generator clocks and manually
flattened edits, independently of the production index and render plan.

All 1,040 picture checks pass. All 36 signal windows measure zero offset;
four video-only windows are silent and make no offset claim. Exact emitted
sample endpoints are 41,642 at 30000/1001, 24,960 at 50 and 20,821 at 60000/1001.
The release test takes 85.79 seconds after its build. Strict CLI all-target
Clippy and formatting pass. Runtime source, fixtures and policies did not change.

`source-identity.json` pins source, fixtures, generators and both executed
binaries. `runs/` retains every full quality report, emitted movie and publication
report. `artifact-inventory.json` hashes the complete scratch projects and
requests. `checks/` retains commands, logs, summary and the first failed run.
`compression.json` describes gzip files; `SHA256SUMS` covers all retained files.

The initial test helper used a filename containing a dot to construct a
revision ID and failed before editing or export. The helper now uses a valid
fixed ID prefix and keeps the filename as its report label. Review also corrected
the VP9 fixture's oracle to its authored alternating 60/40 ms intervals; it is
not the AV1 dropped-frame pattern. A post-run console summary initially tried
to take the maximum over optional level values, including null for silence;
this did not affect any verification report or the saved matrix summary.

Reproduce with the project's qualified FFmpeg prefix and Rust toolchain:

    DEADPAN_PREVIEW_EXPORT_KEEP=1 cargo test --release --locked -p deadpan-cli --test preview_export source_formats_preserve_independent_clocks_through_combined_structural_edits -- --nocapture

The original complete packages remain at the `root` in `checks/matrix-summary.json`.
No native window, acoustic/device playback, real-person footage, fresh bundle
or sanitizer run is claimed by these test-only changes.
''')
manifest=[]
for p in sorted(OUT.rglob('*')):
    if p.is_file() and p.name!='SHA256SUMS':manifest.append(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(OUT)))
(OUT/'SHA256SUMS').write_text('\n'.join(manifest)+'\n')
print('retained',len(manifest),'files')
print(json.dumps(identity['executed_binaries']))
print('max audio level error',max(row['max_level_error_db'] for row in summary['rows'] if row['max_level_error_db'] is not None))
