from pathlib import Path
import hashlib, json, shutil, sys
scratch = Path('/tmp/deadpan-selected-hold-duration-next-2026-10-21')
name = sys.argv[1] if len(sys.argv) > 1 else 'final-source'
paths = set()
for listing in ['tracked-code-paths.txt', 'new-code-paths.txt']:
    paths.update((scratch/listing).read_text().splitlines())
hashes = {}
for source in sorted(paths):
    path = Path(source)
    if path.suffix != '.rs' or not path.is_file():
        continue
    target = scratch/name/source
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, target)
    hashes[source] = hashlib.sha256(path.read_bytes()).hexdigest()
(scratch/(name+'-identities.json')).write_text(json.dumps(hashes, indent=2)+'\n')
print(f'{len(hashes)} Rust files frozen in {name}')
