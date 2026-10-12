#!/usr/bin/env python3
"""Validate retained tail-send results, hashes, and optional current sources."""
import argparse,gzip,hashlib,json,re
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',action='store_true');args=p.parse_args()
here=Path(__file__).resolve().parent;repo=here.parents[3]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def read(name):return json.loads((here/name).read_text())
def log(name):return gzip.decompress((here/(name+'.log.gz')).read_bytes()).decode()
manifest=read('manifest.json')
assert set(manifest)=={p.name for p in here.iterdir() if p.is_file() and p.name!='manifest.json'}
for name,digest in manifest.items():assert sha(here/name)==digest,name
commands={r['name']:r for r in read('commands.json')}
for name,r in commands.items():assert hashlib.sha256(log(name).encode()).hexdigest()==r['log_sha256'],name
for name in ['pcm-final3','store-tests','app-tests2','doc-tests','clippy-final','fmt','whitespace','export-final','release-final','replay-final','hold-effects','audio-treatments','equalizer']:
 assert commands[name]['exit']==0,name
assert '12 passed; 0 failed; 0 ignored;' in log('pcm-final3')
audio=log('suites').split('Running unittests src/lib.rs (target/debug/deps/deadpan_core-')[0]
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',audio)))==583
assert 'FAILED' not in audio
assert not re.search(r'; [1-9][0-9]* (?:failed|ignored);',audio)
rest=log('suites-rest').split('Running unittests src/lib.rs (target/debug/deps/deadpan_store-')[0]
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',rest)))==1555
assert 'FAILED' not in rest
for name in ['store-tests','app-tests2']:
 assert 'test result: FAILED' not in log(name)
 assert not re.search(r'; [1-9][0-9]* failed;',log(name))
assert 'selected_tail_send_preserves_preview_and_verified_export ... ok' in log('export-final')
for name in ['replay-final','hold-effects','audio-treatments','equalizer']:
 r=json.loads(gzip.decompress((here/(name+'-report.json.gz')).read_bytes()))
 assert not r['failed'] and not any(s['failed'] for s in r['scenarios']),name
 assert commands[name]['binaries']['target/release/deadpan-app']==read('identity.json')['binaries']['target/release/deadpan-app']
export=read('export-results.json');report=export['report']
assert report['passed'] and not report['failures']
assert report['summary']['pictures_checked']==60 and report['summary']['blocks_compared']==78
assert len(report['audio'])==2
assert all(w['passed'] and w['offset_status']=='verified_zero' and w['measured_offset_samples']==0 for w in report['audio'])
receipt=read('render-receipt.json')
assert receipt['destination_readback']['sha256']==sha(here/'selected-tail-send.mp4')
assert receipt['provenance']['document_sha256']==report['document_sha256']
assert commands['export-final']['binaries'] and commands['suites']['binaries']
if args.source:
 for name,digest in read('identity.json')['sources'].items():assert sha(repo/name)==digest,name
print('Selected tail-send evidence verified'+(' against current source' if args.source else ''))
