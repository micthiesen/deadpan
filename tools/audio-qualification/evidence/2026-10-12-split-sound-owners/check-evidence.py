import gzip,hashlib,json,pathlib,sys,zipfile
here=pathlib.Path(__file__).resolve().parent
sha=lambda data:hashlib.sha256(data).hexdigest()
read=lambda name:json.loads((here/name).read_text())
for name,digest in read('manifest.json').items():
 assert sha((here/name).read_bytes())==digest,name
commands={r['name']:r for r in read('commands.json')}
for name,record in commands.items():
 log=gzip.decompress((here/(name+'.log.gz')).read_bytes())
 assert sha(log)==record['log_sha256'],name
for name in ['core-audio-store','clippy','release-audio','release-store','release-export','release-app','fmt-check','baseline','baseline-final']:
 assert commands[name]['exit']==0,name
for name in ['core-first','audio-first','cli-first']:
 assert commands[name]['exit']!=0,name
counts=read('test-counts.json')
assert counts['passed']>2800 and counts['failed']==counts['ignored']==0
for name in ['baseline-result.json.gz','baseline-final-result.json.gz']:
 result=json.loads(gzip.decompress((here/name).read_bytes()))
 assert result['old_refused_without_changes'] and result['new_accepted']
 assert len(result['old_binary_sha256'])==len(result['new_binary_sha256'])==64
 assert any(call['exit']!=0 and 'sound owner interval changes require an attachment route' in call['stderr'] for call in result['calls'])
final=json.loads(gzip.decompress((here/'baseline-final-result.json.gz').read_bytes()))
assert final['new_binary_sha256']==read('identity.json')['final_release_binaries']['target/release/deadpan-cli']
for name in ['debug-export.json','release-export-result.json']:
 report=read(name)['report']
 assert report['passed']
 assert len(report['pictures'])==9
 assert len(report['audio'])==1
 assert all(window['passed'] and window['offset_status']=='verified_zero' and window['measured_offset_samples']==0 and window['blocks']['compared']==21 for window in report['audio'])
with zipfile.ZipFile(here/'project.zip') as archive:
 assert archive.testzip() is None
if '--source' in sys.argv:
 repo=next(p for p in [pathlib.Path.cwd(),*pathlib.Path.cwd().parents] if (p/'.git').exists())
 for path,digest in read('identity.json')['sources'].items():
  assert sha((repo/path).read_bytes())==digest,path
print('Split sound evidence verified'+(' against source' if '--source' in sys.argv else ''))
