from pathlib import Path
import shutil,json,hashlib
root=Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
src=Path('/tmp/deadpan-first-pause-profile-2026-10-22/source')
dst=root/'baseline'
def ignore(path,names):
 return [n for n in names if n=='.git' or (Path(path)==src and n=='target') or (Path(path)==src/'tools/media-qualification' and n=='evidence')]
shutil.copytree(src,dst,ignore=ignore)
name='crates/deadpan-core/tests/compact_frozen_edges.rs'
shutil.copy2(Path('/Users/michael/Code/deadpan')/name,dst/name)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
original={str(p.relative_to(src)):sha(p)for p in src.rglob('*.rs') if 'target' not in p.relative_to(src).parts[:1]}
for n in original:
 if not (dst/n).is_file():
  p=dst/n;p.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(src/n,p)
actual={str(p.relative_to(dst)):sha(p)for p in dst.rglob('*.rs')}
assert {n:v for n,v in actual.items() if n!=name}==original
(root/'baseline-copy-receipt.json').write_text(json.dumps({'base':'a18ff65443aa8fd1aa5ab63fa33678c9afd88220','overlay':name,'sha256':sha(dst/name),'all_other_rust_unchanged':True},indent=2)+'\n')
print(len(actual))
