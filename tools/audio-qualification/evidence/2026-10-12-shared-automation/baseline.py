import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile

scratch = Path(__file__).resolve().parent
repo = Path('/Users/michael/Code/deadpan')
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
target = scratch / 'baseline'
target.mkdir(exist_ok=True)
archive = scratch / 'baseline.tar'
with archive.open('wb') as output:
    subprocess.run(['git', 'archive', revision, 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates', 'native'], cwd=repo, stdout=output, check=True)
with tarfile.open(archive) as source:
    source.extractall(target, filter='data')
overlay = 'crates/deadpan-audio/tests/audio_definition/sound_clocks/group_mix.rs'
shutil.copy2(repo / overlay, target / overlay)
with archive.open('rb') as source:
    checksum = hashlib.file_digest(source, 'sha256').hexdigest()
record = dict(revision=revision, archive_sha256=checksum, overlay={overlay:hashlib.sha256((target / overlay).read_bytes()).hexdigest()}, target=str(scratch / 'baseline-target'))
(scratch / 'baseline-source.json').write_text(json.dumps(record, indent=2)+'\n')
print(json.dumps(record))
