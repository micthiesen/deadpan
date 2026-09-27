import hashlib
import json
from pathlib import Path
import subprocess

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-nested-sequence-20260926')
out = repo / 'tools/media-qualification/evidence/2026-09-26-nested-sequence'
sha = lambda data: hashlib.sha256(data).hexdigest()
expected = json.loads((scratch/'gate-1/source-after.json').read_text())
assert all(sha((repo/p).read_bytes()) == h for p,h in expected.items())
review = json.loads((scratch/'review.json').read_text())['source_sha256']
assert all(expected[p] == h for p,h in review.items())
for name,digest in json.loads((out/'sha256.json').read_text()).items():
    assert sha((out/name).read_bytes()) == digest
links = subprocess.run(['python3', str(scratch/'check-doc-links.py')], cwd=repo,
                       capture_output=True, text=True, check=True)
diff = subprocess.run(['git','-c','core.fsmonitor=false','diff','--check'], cwd=repo,
                      capture_output=True, text=True, check=True)
for name in ('post-verify.py', 'check-doc-links.py'):
    (out/name).write_bytes((scratch/name).read_bytes())
record = dict(source_count=len(expected), sources_match_final_gate=True,
              reviewed_paths=len(review), review_hashes_match=True,
              retained_evidence_hashes_match=True,
              doc_links=json.loads(links.stdout),
              diff_check_exit_code=diff.returncode)
(out/'post-verification.json').write_text(json.dumps(record,indent=2)+'\n')
hashes = {str(p.relative_to(out)):sha(p.read_bytes())
          for p in sorted(out.rglob('*')) if p.is_file() and p.name != 'sha256.json'}
(out/'sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')
print(json.dumps(record))
