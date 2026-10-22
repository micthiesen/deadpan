from pathlib import Path
import hashlib,json,shutil,sys
scratch=Path('/tmp/deadpan-native-fixed-retime-hold-2026-10-21')
ident=json.loads((scratch/'source-identities.json').read_text())
for source,expected in ident.items():
 assert hashlib.sha256(Path(source).read_bytes()).hexdigest()==expected,source
if sys.argv[1:]==['snapshot']:
 directory=scratch/'bin'
 directory.mkdir()
 binaries={}
 for name in ['deadpan-app','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
  source=Path('target/release')/name
  dest=directory/name
  shutil.copy2(source,dest)
  expected=hashlib.sha256(source.read_bytes()).hexdigest()
  assert hashlib.sha256(dest.read_bytes()).hexdigest()==expected
  binaries[name]={'source':str(source),'executed':str(dest),'sha256':expected}
 (scratch/'binaries.json').write_text(json.dumps(binaries,indent=2)+'\n')
print('Sources verified'+('; release binaries copied and hashed' if sys.argv[1:]==['snapshot'] else ''))
