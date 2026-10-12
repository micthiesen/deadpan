from pathlib import Path
import gzip
import hashlib
import json
import shutil
import subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
dest = repo/'tools/audio-qualification/evidence/2026-10-11-finite-filter'
dest.mkdir(parents=True,exist_ok=True)
for path in sorted(root.iterdir()):
    if path.suffix == '.log':
        (dest/(path.name+'.gz')).write_bytes(gzip.compress(path.read_bytes(),mtime=0))
    elif path.suffix in ['.json','.py']:
        shutil.copyfile(path,dest/path.name)
patch = subprocess.check_output(['git','diff','--cached','--binary','--','native/deadpan-dsp'],cwd=repo)
(dest/'native-source.patch.gz').write_bytes(gzip.compress(patch,mtime=0))
binary = root/'abi-probe'
(dest/'abi-binary.json').write_text(json.dumps(dict(sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
    bytes=binary.stat().st_size, sanitized=True),indent=2)+'\n')
review = dict(reviewer='primary agent, solo as instructed',
    inspected=['complete staged source, tests, example and documentation',
               'signed overlap-save orientation, DC/Nyquist bins, finite bounds',
               'fixed-size FFI borrows, exception boundaries, failed output publication',
               'allocation-failure cleanup, tile reuse and independent plan ownership',
               'no new authored state or control; full EQ integration remains owed'],
    corrections=['typed arrays at private FFI boundary',
                 'header explicitly permits aliased immutable input arrays',
                 'SHA-256 digest byte formatting and three Clippy test-loop errors'],
    limitations=['no authored EQ recipe, voice/group routing, media context or filter-tail integration',
                 'no EQ editor, real-media preview/export or listening qualification',
                 'bounded kernel measurements exclude the complete application graph'],
    initial_unlogged_failure='The first cargo test -p deadpan-dsp run failed to compile the probe because the SHA-256 digest type does not implement LowerHex; fixed before the retained dsp-tests run. Its diagnostic was observed in the tool transcript, not a retained command log.')
(dest/'review.json').write_text(json.dumps(review,indent=2)+'\n')
manifest = {}
for path in sorted(dest.iterdir()):
    if path.name != 'manifest.json':
        data = path.read_bytes()
        manifest[path.name] = dict(bytes=len(data),sha256=hashlib.sha256(data).hexdigest())
(dest/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
subprocess.run(['python3',str(dest/'check-evidence.py'),'--source'],cwd=repo,check=True)
