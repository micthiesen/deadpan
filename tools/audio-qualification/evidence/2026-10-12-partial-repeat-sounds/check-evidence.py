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
for name in ['core-plan','audio-clocks','store-clocks','clippy','debug-export','release-audio','release-export','release-app','fmt-check','baseline-final']:
 assert commands[name]['exit']==0,name
for name in ['cli-first','cli-reserve','audio-nested','nested-trace']:
 assert commands[name]['exit']!=0,name
for counts in read('test-counts.json').values():
 assert counts['passed']>0 and counts['failed']==counts['ignored']==0
result=json.loads(gzip.decompress((here/'baseline-final-result.json.gz').read_bytes()))
assert result['old_refused_without_changes'] and result['new_accepted']
assert result['new_binary_sha256']==read('identity.json')['final_release_binaries']['target/release/deadpan-cli']
assert any(call['exit']!=0 and 'split sound owners inside Repeat need an occurrence route' in call['stderr'] for call in result['calls'])
for name in ['debug-export-result.json','release-export-result.json']:
 report=read(name)['report']
 assert not report['passed']
 assert len(report['pictures'])==13 and all(p['passed'] for p in report['pictures'])
 assert len(report['audio'])==1
 window=report['audio'][0]
 assert window['flags']==['audio_block_snr']
 assert window['offset_status']=='verified_zero' and window['measured_offset_samples']==0
 assert window['blocks']['compared']==33 and window['blocks']['low_snr_blocks']==1
 assert window['blocks']['min_snr_db'] < report['thresholds']['min_block_snr_db']
with zipfile.ZipFile(here/'project.zip') as archive:
 assert archive.testzip() is None
if '--source' in sys.argv:
 repo=next(p for p in [pathlib.Path.cwd(),*pathlib.Path.cwd().parents] if (p/'.git').exists())
 for path,digest in read('identity.json')['sources'].items():
  assert sha((repo/path).read_bytes())==digest,path
print('Partial Repeat evidence verified; AAC fidelity and nested EQ preparation remain unqualified'+(' against source' if '--source' in sys.argv else ''))
