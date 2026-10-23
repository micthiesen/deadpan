from pathlib import Path
import json,hashlib,shutil
s=Path('/tmp/deadpan-compact-audio-bindings-2026-10-23')
out=Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-compact-bindings-core-checkpoint')
out.mkdir(exist_ok=False)
def sha(f):return hashlib.sha256(f.read_bytes()).hexdigest()
def copy(f,name):
 t=out/name;t.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(f,t)
phases=['candidate-all-core','candidate-all-core-2','candidate-all-core-final']
for phase in phases:
 for f in sorted(s.glob(phase+'-*')):
  if f.is_file()and f.suffix in ['.json','.log','.stdout','.stderr']:copy(f,f.name)
  elif f.is_dir()and f.name==phase+'-overlays':
   for src in f.rglob('*.txt'):copy(src,src.relative_to(s))
for name in ['reviews.json','qualify.py','prepare_fixture_refinement.py','refined-compact-audio-bindings.rs.txt','visible-pause-measurement-plan.md','core-driver.stdout','core-driver.stderr','core-driver-2.stdout','core-driver-2.stderr','core-driver-final.stdout','core-driver-final.stderr']:
 copy(s/name,name)
(out/'README.md').write_text('''# Compact audio-binding Core checkpoint, 2026-10-23

Working Core58/SQLite87 candidate, not yet production-qualified or committed.
Final Core run: 1,513 tests /69 binaries, zero skipped. Source, six inputs and
executed test binaries match their recorded hashes before and after the run.

Retain the initial build refusal (two borrow errors in new tests), then
1,512/1,513 before a negative nested fixture correction. The corrected fixture
shares a validated state with a resume term and reanchor, confirms a positive
roundtrip, then tests null/oversized paths in all three locations. The original
rejection fixture had no enclosing resume/reanchor and failed its expected
three-placement control. Production logic stayed unchanged through that
fixture correction; compare source manifests.

Independent general/codec/capture reviews and test-fixture follow-up are in
reviews.json. Broader Store/UI, strict checks, actual media/native, package
reopening and current-format performance remain owed. See docs/REQUIREMENTS.md.

Source overlays are relative to the immutable a18ff654 snapshot named by the
qualification script; the declared new-change base is917f9a54. Cargo targets,
nextest archives and executables are excluded; executable identities are kept.
''')
files={str(f.relative_to(out)):{'bytes':f.stat().st_size,'sha256':sha(f)}for f in sorted(out.rglob('*'))if f.is_file()}
inventory={'files':files,'payload_files':len(files),'payload_bytes':sum(v['bytes']for v in files.values())}
(out/'inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
for n,r in files.items():
 assert (out/n).stat().st_size==r['bytes']and sha(out/n)==r['sha256']
print(json.dumps({'files':len(files),'bytes':inventory['payload_bytes'],'inventory_sha256':sha(out/'inventory.json')}))
