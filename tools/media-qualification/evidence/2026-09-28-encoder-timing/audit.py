"""Check retained hashes, archive members and terminal command outcomes."""
import gzip
import hashlib
import json
from pathlib import Path
import sys
import tarfile

root=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).parent
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()
manifest=json.loads((root/'manifest.json').read_text())
assert {str(p.relative_to(root)) for p in root.rglob('*') if p.is_file() and p.name!='manifest.json'}==set(manifest)
for name,row in manifest.items():
    path=root/name
    assert path.stat().st_size==row['bytes'] and sha(path)==row['sha256'],name
archives=json.loads((root/'archive-contents.json').read_text())
for name,records in archives.items():
    expected={row['name']:row for row in records}
    with tarfile.open(root/name,'r:gz') as archive:
        seen=set()
        for entry in archive:
            assert entry.isfile() and entry.name in expected and entry.name not in seen
            assert not Path(entry.name).is_absolute() and '..' not in Path(entry.name).parts
            seen.add(entry.name)
            assert entry.size==expected[entry.name]['bytes']
            with archive.extractfile(entry) as stream:
                assert hashlib.file_digest(stream,'sha256').hexdigest()==expected[entry.name]['sha256']
        assert seen==set(expected)
summary=json.loads((root/'summary.json').read_text())
for command in summary['commands']:
    original=json.loads((root/'commands'/(command['name']+'.json')).read_text())
    assert original=={key:value for key,value in command.items() if key!='name'}
    assert 'exit_code' in original and original['seconds']>=0
    source=gzip.decompress((root/'sources'/(original['source_manifest_sha256']+'.json.gz')).read_bytes())
    assert hashlib.sha256(source).hexdigest()==original['source_manifest_sha256']
for row in summary['reports']:
    report=json.loads(gzip.decompress((root/'reports'/(row['name']+'.json.gz')).read_bytes()))
    assert report['result']==row['result'] and len(report['cases'])==len(row['cases'])
    assert report['source_unchanged_during_run'] and not report['process_faults']
print(json.dumps({'files':len(manifest),'archives':len(archives),
                  'archive_members':sum(len(values) for values in archives.values()),
                  'commands':len(summary['commands']),
                  'failed_commands':sum(row['exit_code']!=0 for row in summary['commands'])}))
