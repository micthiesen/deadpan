#!/usr/bin/env python3
"""Check retained owner-input evidence, optionally against the current source."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re

parser = argparse.ArgumentParser()
parser.add_argument('--source', action='store_true')
args = parser.parse_args()
here = Path(__file__).resolve().parent
root = here.parents[3]
manifest = json.loads((here / 'manifest.json').read_text())
for name, digest in manifest.items():
    assert hashlib.sha256((here / name).read_bytes()).hexdigest() == digest, name
records = json.loads((here / 'commands.json').read_text())
assert all(record['exit_code'] == 0 for record in records)
assert {record['name'] for record in records} == {
    'fmt', 'tests', 'clippy', 'whitespace', 'final-fmt', 'final-owner-tests',
    'final-fmt-check', 'final-whitespace',
}
tests = gzip.decompress((here / 'tests.log.gz').read_bytes()).decode()
assert sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', tests))) == 932
owner_tests = gzip.decompress((here / 'final-owner-tests.log.gz').read_bytes()).decode()
assert 'test result: ok. 6 passed; 0 failed; 0 ignored;' in owner_tests
if args.source:
    identity = json.loads((here / 'identity.json').read_text())
    for name, digest in identity['sources'].items():
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == digest, name
print('Owner-input evidence verified' + (' against current source' if args.source else ''))
