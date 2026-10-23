from pathlib import Path
import hashlib,json,shutil
p=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22/next-profile-plan')
out=Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-first-pause-binding-prototypes')
out.mkdir(exist_ok=False)
def sha(f):return hashlib.sha256(f.read_bytes()).hexdigest()
def copy(src,dest):
 target=out/dest;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,target)
for f in sorted(p.iterdir()):
 if f.is_file():copy(f,Path('procedure')/f.name)
for name in ['capture-profile-controlled','early-fallback-controlled','binding-defaults-v2-controlled','combined-controlled','builds']:
 for f in sorted((p/name).iterdir()):
  if f.is_file() and f.suffix in ['.json','.stdout','.stderr','.log']:
   copy(f,Path(name)/f.name)
for name in ['capture-profile','early-fallback','binding-defaults','binding-defaults-v2','combined']:
 receipt=p/'builds'/(name+'-build.json')
 if not receipt.exists():continue
 r=json.loads(receipt.read_text())
 assert r['exit']==0
 binary=Path(r['binary']['path']);assert sha(binary)==r['binary']['sha256']
 for rel in r['modified_rust_paths']:
  copy(Path(r['source'])/rel,Path('overlays')/name/(rel+'.txt'))
prototype=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22/production-test-drafts')
for name in ['build.stdout','build.stderr','build-receipt.json','build-lto.stdout','build-lto.stderr','build-lto-receipt.json','run.stdout','run.stderr','run-receipt.json']:
 f=prototype/name
 if f.exists():copy(f,Path('initial-test-probe')/name)
(out/'README.md').write_text('''# First-pause binding prototypes, 2026-10-23

Diagnostic experiments against qualified shipping commit
6916ee0d578a11c37e6e66a470eb430b02773764, with isolated sources and target
directories. No production or target qualification is claimed. See
docs/qualification/first-pause-binding-prototypes-2026-10-23.md.

The five-pair batches retain raw command JSON, load reports, timings and
source/input/executable hashes. Binaries, Cargo targets and generated 10,000-beat
SQLite packages are deliberately excluded. Reproduce using retained procedure
scripts and overlays against the recorded source snapshot. Rust overlays use
.rs.txt to avoid rustfmt discovery.

Binding v1 omitted only four fields and was not measured. v2 fixes that
preparation error. Both v2 and combined measured prototypes still lack the
standalone reader preflight correction. Their timing runs do not prove
reopening or codec correctness. The initial direct test probe first failed at
Apple LLVM linking; Rust thin LTO linked it and exposed missing arguments in
the standalone reader (five controls passed, one roundtrip failed). The draft
source was later refined before retention; the probe receipts retain its
original source hash, but its exact original source copy is unavailable.
No claim is made that the current refined draft was executed by that probe.

The production candidate is separate: Core58/SQLite87, bounded optional raw
path preflight, historical fixture adapter and stronger tests. Its verification
does not belong to this diagnostic inventory.
''')
files={str(f.relative_to(out)):{'bytes':f.stat().st_size,'sha256':sha(f)}for f in sorted(out.rglob('*'))if f.is_file()}
inv={'files':files,'payload_files':len(files),'payload_bytes':sum(v['bytes']for v in files.values())}
(out/'inventory.json').write_text(json.dumps(inv,indent=2)+'\n')
for name,info in files.items():
 f=out/name;assert f.stat().st_size==info['bytes'] and sha(f)==info['sha256']
print(json.dumps({'path':str(out),'files':len(files),'bytes':inv['payload_bytes'],'inventory_sha256':sha(out/'inventory.json')}))
