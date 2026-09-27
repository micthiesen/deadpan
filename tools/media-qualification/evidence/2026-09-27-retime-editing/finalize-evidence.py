"""Validate final links, source review identity and evidence, then preserve a read-only Git checkpoint."""
from pathlib import Path
import gzip,hashlib,json,subprocess
r=Path('/Users/michael/Code/deadpan'); s=Path('/tmp/deadpan-retime-20260927')
e=r/'tools/media-qualification/evidence/2026-09-27-retime-editing'
subprocess.run(['python3',str(s/'summarize-tree.py')],check=True)
checked=subprocess.run(['git','-c','core.fsmonitor=false','diff','--check'],cwd=r,capture_output=True,text=True)
(s/'whitespace.json').write_text(json.dumps({'exit_code':checked.returncode,'stdout':checked.stdout,'stderr':checked.stderr},indent=2)+'\n')
checked.check_returncode()
for name in ['summarize-tree.py','doc-links.json','increment.json','whitespace.json','finalize-evidence.py']:
 (e/name).write_bytes((s/name).read_bytes())
(e/'increment.diff.gz').write_bytes(gzip.compress((s/'increment.diff').read_bytes(),mtime=0))
hashes={str(p.relative_to(e)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(e.rglob('*')) if p.is_file() and p!=e/'sha256.json'}
(e/'sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')
subprocess.run(['python3',str(s/'checkpoint.py')],check=True)
