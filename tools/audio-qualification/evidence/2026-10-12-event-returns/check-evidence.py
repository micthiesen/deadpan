#!/usr/bin/env python3
"""Validate retained event-return evidence and optional current source."""
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
for n in ['event-policy','wet-debug','sound-debug','full-tests','clippy-final','clippy-replay-final','release','release-replay-final','export','wet-replay','tail-replay','sound-replay','audio-replay','eq-replay','fmt-final','whitespace-final']:
 assert commands[n]['exit']==0,n
for n in ['event-initial','event-oracle','clippy-initial','registry-regen','wet-replay-initial','sound-replay-initial','wet-replay-heading','sound-replay-role']:
 assert commands[n]['exit']!=0,n
full=log('full-tests')
assert 'FAILED' not in full
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed',full)))==3962
assert 'event_return_enters_the_parent_return_without_self_feedback ... ok' in full
assert 'copied_nested_event_returns_keep_old_history_across_recopy ... ok' in full
assert 'event_return_is_excluded_from_its_own_allocated_tail_tap ... ok' in full
assert commands['full-tests']['binaries']
assert 'continuous_event_return_preserves_preview_and_verified_export ... ok' in log('export')
identity=read('identity.json')
for n in ['wet-replay','tail-replay','sound-replay','audio-replay','eq-replay']:
 r=json.loads(gzip.decompress((here/(n+'-report.json.gz')).read_bytes()))
 assert not r['failed'] and not any(x['failed'] for x in r['scenarios']),n
 assert commands[n]['binaries']['target/release/deadpan-app']==identity['binaries']['target/release/deadpan-app']
e=read('export-results.json');r=e['report']
assert r['passed'] and not r['failures']
assert r['summary']['pictures_checked']==30
assert r['summary']['blocks_compared']>0
assert r['audio'] and all(w['passed'] and w['offset_status']=='verified_zero' and w['measured_offset_samples']==0 for w in r['audio'])
receipt=read('render-receipt.json')
assert receipt['destination_readback']['sha256']==sha(here/'continuous-event-return.mp4')
assert receipt['provenance']['document_sha256']==r['document_sha256']
if args.source:
 for n,d in identity['sources'].items():assert sha(repo/n)==d,n
print('Independent sound-return evidence verified'+(' against current source' if args.source else ''))
