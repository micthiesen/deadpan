#!/usr/bin/env python3
"""Retain a successful run.py output directory, including actual host identity."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('run_directory', type=Path)
args = parser.parse_args()
out = Path(__file__).resolve().parent
records = json.loads((args.run_directory/'commands.json').read_text())
assert [record['name'] for record in records] == ['fmt', 'tests', 'clippy', 'whitespace']
assert all(record['exit_code'] == 0 for record in records)
shutil.copyfile(args.run_directory/'commands.json', out/'commands.json')
identity = json.loads((args.run_directory/'identity.json').read_text())
identity['hardware'] = subprocess.check_output(['sysctl','-n','hw.model','hw.memsize','hw.physicalcpu','machdep.cpu.brand_string'], text=True)
(out/'identity.json').write_text(json.dumps(identity, indent=2)+'\n')
for record in records:
    name = record['name']+'.log'
    (out/(name+'.gz')).write_bytes(gzip.compress((args.run_directory/name).read_bytes(), mtime=0))
files = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(out.iterdir()) if p.is_file() and p.name != 'manifest.json'}
(out/'manifest.json').write_text(json.dumps(files, indent=2)+'\n')
print('Retained ordered-owner EQ evidence')
