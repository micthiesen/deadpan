import gzip,hashlib,json,pathlib,sys,zipfile
here=pathlib.Path(__file__).resolve().parent
sha=lambda data:hashlib.sha256(data).hexdigest()
def read(name):
 data=(here/name).read_bytes()
 return json.loads(gzip.decompress(data) if name.endswith('.gz') else data)
for name,digest in read('manifest.json').items(): assert sha((here/name).read_bytes())==digest,name
commands={r['name']:r for r in read('commands.json')}
for name,record in commands.items():
 assert sha(gzip.decompress((here/(name+'.log.gz')).read_bytes()))==record['log_sha256'],name
for name in ['core-expanded','audio-expanded','store-second','cli-debug','core-plan-audio-playback-store','clippy','release-audio','release-store','release-export','release-prior-export','release-app','fmt-check','old-history-replay','qualify-final']:
 assert commands[name]['exit']==0,name
assert commands['core-red']['exit']!=0
assert commands['audio-second']['exit']!=0
assert commands['store-first']['exit']!=0
assert read('test-counts.json')['core-plan-audio-playback-store']==dict(passed=2880,failed=0,ignored=0,result_groups=146)
for counts in read('test-counts.json').values():
 assert counts['passed']>0 and counts['failed']==counts['ignored']==0
for name in ['cli-debug-result.json.gz','release-export-result.json.gz','final-result.json.gz']:
 report=read(name)['report']
 assert report['passed'] and len(report['pictures'])==17
 assert all(p['passed'] for p in report['pictures'])
 summary=report['summary']
 assert summary['audio_windows_checked']==1 and summary['audio_windows_failed']==0
 assert summary['blocks_compared']==44 and summary['nonzero_offsets']==0 and summary['offset_verified_windows']==1
final=read('final-result.json.gz'); identity=read('identity.json')
assert final['authored_document_unchanged'] and final['pcm_bit_identical']
assert final['new_binary_sha256']==identity['final_release_binaries']['target/release/deadpan-cli']
assert final['debug_binary_sha256']==identity['debug_binary_sha256']
assert len(final['windows'])==5 and all(len(w['samples'])==256 for w in final['windows'])
assert sha((here/'repeat-split.mp4').read_bytes())==final['movie_sha256']
old=read('old-baseline-result.json.gz')
assert old['historical_replay_refused'] and old['document_unchanged']
assert old['old']['exit']!=0 and old['old']['binary_sha256']==identity['old_binary_sha256']
assert 'split sound owners inside Repeat need an occurrence route' in old['old']['stderr']
for name in ['debug-project.zip','release-project.zip']:
 with zipfile.ZipFile(here/name) as archive: assert archive.testzip() is None
if '--source' in sys.argv:
 repo=next(p for p in [pathlib.Path.cwd(),*pathlib.Path.cwd().parents] if (p/'.git').exists())
 for path,digest in identity['sources'].items(): assert sha((repo/path).read_bytes())==digest,path
print('Repeat Split clock evidence verified'+(' against source' if '--source' in sys.argv else ''))
