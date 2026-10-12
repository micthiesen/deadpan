from pathlib import Path
import hashlib,json,os,shutil,subprocess,tarfile

root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
baseline=root/'baseline'
baseline.mkdir()
revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
archive=root/'baseline.tar'
with archive.open('wb') as out:
 subprocess.run(['git','archive',revision,'Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo','crates','native'],cwd=repo,stdout=out,check=True)
with tarfile.open(archive) as tar:
 tar.extractall(baseline,filter='data')
overlay='crates/deadpan-audio/tests/audio_definition/sound_clocks/group_mix.rs'
shutil.copy2(repo/overlay,baseline/overlay)
(root/'baseline-source.json').write_text(json.dumps(dict(revision=revision,archive_sha256=hashlib.file_digest(archive.open('rb'),'sha256').hexdigest(),overlay={overlay:hashlib.sha256((baseline/overlay).read_bytes()).hexdigest()},target=str(root/'baseline-target')),indent=2)+'\n')
