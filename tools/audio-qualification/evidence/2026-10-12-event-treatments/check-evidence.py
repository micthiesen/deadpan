#!/usr/bin/env python3
"""Validate retained event-treatment evidence and optional current source."""
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
for n in ['event-tests-initial','clippy-second','full-tests-remaining-fixed','clippy-final','clippy-replay-final','release','release-final','release-replay-final','export-fixed','headless-sound','wet-replay','wet-replay-final','tail-replay','sound-replay-final','audio-replay','eq-replay','fmt-final','fmt-replay-final','whitespace-final']:
 assert commands[n]['exit']==0,n
for n in ['clippy-initial','registry-regenerate','commands-regenerate','full-tests-initial','full-tests-remaining','full-tests-remaining-final','export','sound-replay']:
 assert commands[n]['exit']!=0,n
initial=log('full-tests-initial').split('Running unittests src/lib.rs (target/debug/deps/deadpan_core-')[0]
remaining=log('full-tests-remaining-fixed')
assert 'FAILED' not in initial and 'FAILED' not in remaining
counts=read('test-counts.json')
assert counts['failed']==0 and counts['passed']>=3969
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',initial+remaining)))==counts['passed']
assert 'root_and_attached_treatments_match_scalar_order_and_cold_queries ... ok' in initial
assert 'independent_envelope_keeps_fractional_onset_clock_through_a_root_move ... ok' in initial
assert 'independent_treatments_share_the_document_record_budget ... ok' in remaining
assert 'treated_event_preserves_preview_and_verified_export ... ok' in log('export-fixed')
identity=read('identity.json')
for n in ['wet-replay-final','tail-replay','sound-replay-final','audio-replay','eq-replay']:
 r=json.loads(gzip.decompress((here/(n+'-report.json.gz')).read_bytes()))
 assert not r['failed'] and not any(x['failed'] for x in r['scenarios']),n
 assert commands[n]['binaries']['target/release/deadpan-app']==identity['binaries']['target/release/deadpan-app']
e=read('export-results.json');r=e['report']
assert r['passed'] and not r['failures']
assert r['summary']['pictures_checked']==30
assert r['summary']['blocks_compared']>0
assert r['audio'] and all(w['passed'] and w['offset_status']=='verified_zero' and w['measured_offset_samples']==0 for w in r['audio'])
receipt=read('render-receipt.json')
assert receipt['destination_readback']['sha256']==sha(here/'treated-event.mp4')
assert receipt['provenance']['document_sha256']==r['document_sha256']
if args.source:
 for n,d in identity['sources'].items():assert sha(repo/n)==d,n
print('Independent sound-treatment evidence verified'+(' against current source' if args.source else ''))
