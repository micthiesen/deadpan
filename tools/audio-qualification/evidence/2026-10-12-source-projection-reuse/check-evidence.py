import gzip,hashlib,json,pathlib,sys,zipfile
here=pathlib.Path(__file__).resolve().parent
sha=lambda data:hashlib.sha256(data).hexdigest()
def read(name):
 data=(here/name).read_bytes()
 return json.loads(gzip.decompress(data) if name.endswith('.gz') else data)
for name,digest in read('manifest.json').items():
 assert sha((here/name).read_bytes())==digest,name
commands={r['name']:r for r in read('commands.json')}
for name,record in commands.items():
 assert sha(gzip.decompress((here/(name+'.log.gz')).read_bytes()))==record['log_sha256'],name
for name in ['keyed-first','cache-tests5','plan-keys','cli-third','baseline-debug2','plan-audio-playback','clippy','release-cache','release-nested','release-real','release-export','release-app','fmt-check','baseline-final','qualify-release']:
 assert commands[name]['exit']==0,name
assert commands['first-witness']['exit']!=0
assert read('test-counts.json')['plan-audio-playback']==dict(passed=1139,failed=0,ignored=0,result_groups=52)
for counts in read('test-counts.json').values():
 assert counts['passed']>0 and counts['failed']==counts['ignored']==0
for name in ['baseline-debug-result.json.gz','baseline-result.json.gz']:
 result=read(name)
 assert result['old_and_new_accepted'] and result['authored_document_unchanged'] and result['pcm_bit_identical']
 assert result['old_binary_sha256']==read('identity.json')['old_binary_sha256']
assert read('baseline-result.json.gz')['new_binary_sha256']==read('identity.json')['final_release_binaries']['target/release/deadpan-cli']
assert read('old-render.json')['exit']!=0
assert b'prepared stages per read' in gzip.decompress((here/'old-render.stderr.gz').read_bytes())
assert read('new-debug-render.json')['exit']==read('release-render.json')['exit']==0
assert read('release-render.json')['binary_sha256']==read('identity.json')['final_release_binaries']['target/release/deadpan-cli']
for name in ['debug-verify-report.json.gz','release-verify-report.json.gz']:
 report=read(name)
 assert report['passed'] and len(report['pictures'])==120
 assert all(p['passed'] for p in report['pictures'])
 summary=report['summary']
 assert summary['audio_windows_checked']==4 and summary['audio_windows_failed']==0
 assert summary['blocks_compared']==20 and summary['nonzero_offsets']==0 and summary['offset_verified_windows']==1
for name in ['real-media.json','release-real-result.json']:
 assert read(name)['max_difference']==0.0 and read(name)['samples']==256 and read(name)['cold_windows']==3
negative=read('release-export-result.json.gz')['report']
assert not negative['passed'] and negative['audio'][0]['flags']==['audio_block_snr']
with zipfile.ZipFile(here/'project.zip') as archive: assert archive.testzip() is None
if '--source' in sys.argv:
 repo=next(p for p in [pathlib.Path.cwd(),*pathlib.Path.cwd().parents] if (p/'.git').exists())
 for path,digest in read('identity.json')['sources'].items(): assert sha((repo/path).read_bytes())==digest,path
print('Source projection reuse evidence verified'+(' against source' if '--source' in sys.argv else ''))
