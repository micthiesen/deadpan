#!/usr/bin/env python3
"""Verify retained treatment evidence and optional current source identity."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re

parser=argparse.ArgumentParser()
parser.add_argument('--source',action='store_true')
args=parser.parse_args()
here=Path(__file__).resolve().parent
root=here.parents[3]
for name,digest in json.loads((here/'manifest.json').read_text()).items():
    assert hashlib.sha256((here/name).read_bytes()).hexdigest()==digest,name
records=json.loads((here/'commands.json').read_text())
assert all(record['exit_code']==0 for record in records)
assert {record['name'] for record in records}=={
    'fmt','tests','clippy','whitespace','final-fmt','final-owner-tests',
    'final-fmt-check','final-whitespace',
}
tests=gzip.decompress((here/'tests.log.gz').read_bytes()).decode()
assert sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',tests)))==935
final=gzip.decompress((here/'final-owner-tests.log.gz').read_bytes()).decode()
assert 'test result: ok. 7 passed; 0 failed; 0 ignored;' in final
if args.source:
    for name,digest in json.loads((here/'identity.json').read_text())['sources'].items():
        assert hashlib.sha256((root/name).read_bytes()).hexdigest()==digest,name
print('Retained treatment evidence verified'+(' against current source' if args.source else ''))
