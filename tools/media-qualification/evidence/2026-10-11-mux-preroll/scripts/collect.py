from pathlib import Path
import gzip
import hashlib
import json
import subprocess

root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
evidence=repo/'tools/media-qualification/evidence/2026-10-11-mux-preroll'
compression=[]
def retain(source, relative):
    data=source.read_bytes()
    compressed=len(data)>16000 and source.suffix != '.mp4'
    target=evidence/(str(relative)+('.gz' if compressed else ''))
    target.parent.mkdir(parents=True,exist_ok=True)
    if compressed:
        compression.append(dict(path=str(target.relative_to(evidence)),original_bytes=len(data),original_sha256=hashlib.sha256(data).hexdigest()))
        data=gzip.compress(data,mtime=0)
    target.write_bytes(data)
for source in sorted(root.iterdir()):
    if source.is_file() and source.suffix in ['.log','.json','.py']:
        retain(source, (Path('scripts') if source.suffix=='.py' else Path('checks'))/source.name)
retain(root/'fixture-identity.json',Path('fixture-identity.json'))
retain(root/'capture-1/pq-640x360-hardware-targettwo.mp4',Path('current-aac-source.mp4'))
paths=['native/deadpan-encode/src/mux_duration.rs','native/deadpan-encode/src/mux_duration_tests.rs',
       'crates/deadpan-cli/src/encoded_render/verification/inspect/hdr_tests.rs']
identity=dict(base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),
    files={name:hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in paths},
    review='Sole-agent review; derives the existing exact AAC priming budget without changing audio bytes, clocks or acceptance tolerances.',
    os=subprocess.check_output(['sw_vers'],text=True),cpu=subprocess.check_output(['sysctl','-n','machdep.cpu.brand_string'],text=True).strip())
(evidence/'source-identity.json').write_text(json.dumps(identity,indent=2)+'\n')
(evidence/'compression.json').write_text(json.dumps(compression,indent=2)+'\n')
for row in compression:
    data=gzip.decompress((evidence/row['path']).read_bytes())
    assert len(data)==row['original_bytes'] and hashlib.sha256(data).hexdigest()==row['original_sha256']
(evidence/'SHA256SUMS').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(evidence))+'\n'
    for p in sorted(evidence.rglob('*')) if p.is_file() and p.name!='SHA256SUMS'))
print('Collected',sum(p.is_file() for p in evidence.rglob('*')),'files')
