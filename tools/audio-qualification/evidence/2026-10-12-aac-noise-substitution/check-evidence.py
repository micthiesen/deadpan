import gzip,hashlib,json,pathlib,struct,sys,zipfile
here=pathlib.Path(__file__).resolve().parent
sha=lambda data:hashlib.sha256(data).hexdigest()
def read(name):
 data=(here/name).read_bytes()
 return json.loads(gzip.decompress(data) if name.endswith('.gz') else data)
for name,digest in read('manifest.json').items(): assert sha((here/name).read_bytes())==digest,name
commands={r['name']:r for r in read('commands.json')}
for name,record in commands.items(): assert sha(gzip.decompress((here/(name+'.log.gz')).read_bytes()))==record['log_sha256'],name
for name in ['native-green','native-tests2','cli-encoded','jobs-admission','clippy-final','release-native','release-preview2','release-app','fmt-check','qualify-final']: assert commands[name]['exit']==0,name
for name in ['native-red','native-red2','native-tests','release-preview']: assert commands[name]['exit']!=0,name
counts=read('test-counts.json')
for name in ['native-tests2','release-native']: assert counts[name]['passed']==65 and counts[name]['failed']==counts[name]['ignored']==0
assert counts['jobs-admission']['passed']>0 and counts['jobs-admission']['failed']==counts['jobs-admission']['ignored']==0
assert counts['cli-encoded']==dict(passed=150,failed=0,ignored=4,result_groups=1)
assert counts['release-preview2']==dict(passed=31,failed=0,ignored=0,result_groups=1)
coverage=read('recipe-coverage.json')
assert len(coverage['built_fixtures'])==42 and len(coverage['skipped'])==4
final=read('final-result.json.gz'); identity=read('identity.json')
assert final['old_binary_sha256']==identity['old_binary_sha256']
assert final['new_binary_sha256']==identity['final_release_binaries']['target/release/deadpan-cli']
assert final['pcm_bit_identical'] and final['authored_document_unchanged']
assert final['pcm_sha256']==read('reference.json')['pcm_sha256']==identity['sources']['native/deadpan-encode/tests/fixtures/partial-repeat-eq.f32le']
old=read('old-verify-report.json.gz'); assert not old['passed'] and old['audio'][0]['flags']==['audio_block_snr']
assert old['summary']['pictures_checked']==13 and old['summary']['nonzero_offsets']==0
for name,count,windows in [('partial-repeat',13,1),('nested-eq',120,4)]:
 report=read(name+'-verify-report.json.gz'); summary=report['summary']
 assert report['passed'] and len(report['pictures'])==count and all(p['passed'] for p in report['pictures'])
 assert summary['audio_windows_checked']==windows and summary['audio_windows_failed']==0
 assert summary['nonzero_offsets']==0 and summary['offset_verified_windows']==1
 assert sha((here/(name+'.mp4')).read_bytes())==final['reports'][name]['sha256']
 assert report['movie']['audio_edits']['media_time']==2048
report=read('partial-repeat-verify-report.json.gz')
assert report['summary']['blocks_compared']==33 and report['summary']['min_block_snr_db']>18
assert read('release-preview2-result.json.gz')['report']['passed']
probe=read('probe-results.json.gz')['variants']; reproduction=read('reproduction.json')
assert reproduction['bit_identical'] and reproduction['presented_samples']==20821
assert abs(probe['production']['min_snr_db']-1.0882855904811848)<1e-6
assert probe['no_pns']['min_snr_db']>18 and not probe['no_pns']['low_snr']
def pcm(name):
 data=gzip.decompress((here/(name+'.decoded.gz')).read_bytes()); pos=0; result={}
 assert sha(data)==probe[name]['decoded_sha256']
 while pos<len(data):
  pts,n=struct.unpack_from('<qI',data,pos); pos+=12
  for i in range(n):
   assert pts+i not in result
   result[pts+i]=data[pos+i*8:pos+i*8+8]
  pos+=n*8
 assert pos==len(data)
 return result
left,right=pcm('production'),pcm('default')
assert all(left[i]==right[i] for i in range(20821))
for archive in ['project-old.zip','project-current.zip']:
 with zipfile.ZipFile(here/archive) as file: assert file.testzip() is None
if '--source' in sys.argv:
 repo=next(p for p in [pathlib.Path.cwd(),*pathlib.Path.cwd().parents] if (p/'.git').exists())
 for path,digest in identity['sources'].items(): assert sha((repo/path).read_bytes())==digest,path
print('AAC noise substitution evidence verified'+(' against source' if '--source' in sys.argv else ''))
