#!/usr/bin/env python3
"""Validate retained owner-return evidence and optional current source."""
import argparse,gzip,hashlib,json,re
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--source',action='store_true');args=p.parse_args()
here=Path(__file__).resolve().parent;repo=here.parents[3]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(n):return json.loads((here/n).read_text())
def log(n):return gzip.decompress((here/(n+'.log.gz')).read_bytes()).decode()
manifest=read('manifest.json')
assert set(manifest)=={p.name for p in here.iterdir() if p.is_file() and p.name!='manifest.json'}
for n,d in manifest.items():assert sha(here/n)==d,n
commands={r['name']:r for r in read('commands.json')}
for n,r in commands.items():assert hashlib.sha256(log(n).encode()).hexdigest()==r['log_sha256'],n
for n in ['tail-policy-after','store-final','doc-tests','service-final','clippy-final','clippy-verified','release','export-confirmed','wet-replay','tail-replay','audio-replay','eq-replay','fmt-final','whitespace-final']:
 assert commands[n]['exit']==0,n
assert commands['tail-policy-before']['exit']!=0
assert 'explicit_tail_provider_excludes_unselected_incoming_parent_returns ... FAILED' in log('tail-policy-before')
assert '9 passed; 0 failed;' in log('tail-policy-after')
prefix=log('full-tests-final').split('Running unittests src/lib.rs (target/debug/deps/deadpan_store-')[0]
assert 'FAILED' not in prefix
assert '1121 passed; 0 failed; 9 ignored;' in prefix
assert len(re.findall(r'test wet_owners::[^\n]+ \.\.\. ok',prefix))==10
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',prefix)))==3282
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',log('store-final').split('Doc-tests')[0])))==667
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',log('doc-tests'))))==1
assert '320 passed; 0 failed; 1 ignored;' in log('service-final')
assert commands['full-tests-final']['binaries']
assert 'continuous_owner_return_preserves_preview_and_verified_export ... ok' in log('export-confirmed')
identity=read('identity.json')
for n in ['wet-replay','tail-replay','audio-replay','eq-replay']:
 r=json.loads(gzip.decompress((here/(n+'-report.json.gz')).read_bytes()))
 assert not r['failed'] and not any(x['failed'] for x in r['scenarios']),n
 assert commands[n]['binaries']['target/release/deadpan-app']==identity['binaries']['target/release/deadpan-app']
e=read('export-results.json');r=e['report']
assert r['passed'] and not r['failures']
assert r['summary']['pictures_checked']==30
assert r['summary']['blocks_compared']>0
assert r['audio'] and all(w['passed'] and w['offset_status']=='verified_zero' and w['measured_offset_samples']==0 for w in r['audio'])
receipt=read('render-receipt.json')
assert receipt['destination_readback']['sha256']==sha(here/'continuous-owner-return.mp4')
assert receipt['provenance']['document_sha256']==r['document_sha256']
if args.source:
 for n,d in identity['sources'].items():assert sha(repo/n)==d,n
print('Continuous owner-return evidence verified'+(' against current source' if args.source else ''))
