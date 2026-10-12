from pathlib import Path
import gzip
import hashlib
import json
import shutil
import subprocess

root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
dest=repo/'tools/audio-qualification/evidence/2026-10-11-eq-recipe'
dest.mkdir(parents=True,exist_ok=True)
for path in sorted(root.iterdir()):
    if path.suffix=='.log': (dest/(path.name+'.gz')).write_bytes(gzip.compress(path.read_bytes(),mtime=0))
    elif path.suffix in ['.json','.py']: shutil.copyfile(path,dest/path.name)
patch=subprocess.check_output(['git','diff','--cached','--binary','--','crates'],cwd=repo)
(dest/'source.patch.gz').write_bytes(gzip.compress(patch,mtime=0))
review=dict(reviewer='primary agent, solo as instructed',
    checked=['typed and JSON bounds, fixed-width recipe, exact values and neutral identity',
             'at most six consolidated endpoints, symmetric bounded FIR and fixed context',
             'gain order independence of coefficient compilation, no signal normalization',
             'complete input validation and transactional outputs in active/bypass paths',
             'independent response/PCM tests, cold equality and whole-workspace compilation',
             'full staged source, examples, docs and retained evidence'],
    limitations=['No AudioTreatments stage, authored commands or GUI controls yet',
                 'No retained-owner, group or per-voice filter-tail integration',
                 'No full real-media preview/export EQ or listening claim'],
    observed_setup_failure='An initial cargo fmt ran before equalizer/tests.rs was written and could not resolve the module; corrected before retained format and test runs. The initial diagnostic remains in the tool transcript.')
(dest/'review.json').write_text(json.dumps(review,indent=2)+'\n')
manifest={}
for path in sorted(dest.iterdir()):
    if path.name!='manifest.json':
        data=path.read_bytes()
        manifest[path.name]=dict(bytes=len(data),sha256=hashlib.sha256(data).hexdigest())
(dest/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
subprocess.run(['python3',str(dest/'check-evidence.py'),'--source'],cwd=repo,check=True)
