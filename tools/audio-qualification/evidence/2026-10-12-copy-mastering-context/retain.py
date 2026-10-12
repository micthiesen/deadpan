import gzip, hashlib, json, shutil, subprocess
from pathlib import Path
s = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
out = repo/'tools/audio-qualification/evidence/2026-10-12-copy-mastering-context'
out.mkdir(parents=True, exist_ok=True)
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def write(name, value):
    (out/name).write_text(json.dumps(value, indent=2)+'\n')
records = []
for path in sorted(s.glob('*.json')):
    record = json.loads(path.read_text())
    if isinstance(record, dict) and 'log_sha256' in record:
        assert sha(s/(record['name']+'.log')) == record['log_sha256']
        records.append(record)
write('commands.json', records)
for path in s.glob('*.log'):
    (out/(path.name+'.gz')).write_bytes(gzip.compress(path.read_bytes(), mtime=0))
base = subprocess.check_output(['git','rev-parse','a459fc28'],cwd=repo,text=True).strip()
sources = ['crates/deadpan-cli/tests/preview_export.rs','crates/deadpan-cli/tests/preview_export/partial_sounds.rs','crates/deadpan-cli/tests/preview_export/recipes.rs','crates/deadpan-audio/src/limiter.rs','native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav','native/deadpan-source/tests/fixtures/cfr-bframes.mp4','Cargo.lock','rust-toolchain.toml']
write('identity.json',dict(base_commit=base,sources={name:sha(repo/name) for name in sources},binaries={name:sha(repo/name) for name in ['target/debug/deadpan-cli','target/release/deadpan-cli']}))
(out/'implementation.patch').write_bytes(subprocess.check_output(['git','diff',base,'--','crates'],cwd=repo))
for name in ['README.md','review.md','run.py','checks.py','retain.py','check-evidence.py','debug-result.json','release-result.json','export-result.json']:
    shutil.copyfile(s/name,out/name)
report=json.loads((s/'release-result.json').read_text())
shutil.make_archive(str(out/'project'),'zip',root_dir=Path(report['package']).parent)
export=json.loads((s/'export-result.json').read_text())
shutil.copyfile(export['movie'],out/'export.mp4')
write('manifest.json',{p.name:sha(p) for p in sorted(out.iterdir()) if p.is_file() and p.name!='manifest.json'})
print('Retained',len(list(out.iterdir())),'files')
