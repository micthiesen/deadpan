#!/usr/bin/env python3
"""Publish one bounded evidence package; never run tests or replace evidence."""
import argparse
import json
from pathlib import Path
import runpy
import shutil
import tempfile

SCRATCH = Path(__file__).resolve().parent
HELPERS = SCRATCH / 'bundle-helpers.py'
import hashlib
helper_data = HELPERS.read_bytes()
assert hashlib.sha256(helper_data).hexdigest() == '2758fa2c837e63d41b33f38b34f728017feda8b0c05aa21499eeb40f4764df13'
H = runpy.run_path(str(HELPERS))
assert HELPERS.read_bytes() == helper_data
read = H['read_regular']
digest = H['digest']
encode = H['json_bytes']
require = H['require']
identity = H['hash_regular']

parser = argparse.ArgumentParser()
parser.add_argument('--plan', type=Path, required=True)
parser.add_argument('--approved-script-sha256', required=True)
parser.add_argument('--approved-plan-sha256', required=True)
args = parser.parse_args()
limit = 256 * 1024 * 1024
script = Path(__file__).resolve()
script_data = read(script, limit)
plan_data = read(args.plan, limit)
require(digest(script_data) == args.approved_script_sha256, 'collector changed')
require(digest(plan_data) == args.approved_plan_sha256, 'plan changed')
plan = json.loads(plan_data)
require(plan['schema_version'] == 1 and plan['raw_byte_limit'] == limit, 'invalid plan')
repo = Path(plan['repository']).resolve(strict=True)
destination = repo / H['safe_name'](plan['destination'])
require(not destination.exists() and not destination.is_symlink(), 'destination exists')
require(SCRATCH.stat().st_dev == destination.parent.stat().st_dev, 'cross-device publication')
frozen_data = read(SCRATCH / 'final-identities.json', limit)
frozen = json.loads(frozen_data)
tracked = {str(script): identity(script, limit), str(args.plan): identity(args.plan, limit)}
tracked[str(HELPERS)] = {'bytes': len(helper_data), 'sha256': digest(helper_data)}
tracked[str(SCRATCH / 'final-identities.json')] = {'bytes': len(frozen_data), 'sha256': digest(frozen_data)}
stage = Path(tempfile.mkdtemp(prefix='.camera-faces-staging-', dir=SCRATCH))
published = False
try:
    collector = H['Collector'](plan, stage)
    collector.retain('packaging/collect.py.txt', script_data, str(script))
    collector.retain('packaging/collect-plan.json', plan_data, str(args.plan))
    collector.retain('packaging/bundle-helpers.py.txt', helper_data, str(HELPERS))
    for name, expected in frozen['sources'].items():
        source = repo / H['safe_name'](name)
        require(identity(source, limit) == expected, f'final source changed: {source}')
        tracked[str(source)] = expected
        collector.retain('source/' + name + '.txt', read(source, limit), str(source))
    for name, expected in plan['post_run_sources'].items():
        source = repo / H['safe_name'](name)
        require(identity(source, limit) == expected, f'post-run source changed: {source}')
        tracked[str(source)] = expected
        collector.retain('post-run-source/' + name + '.txt', read(source, limit), str(source))
    for name, expected in plan['inputs'].items():
        source = repo / H['safe_name'](name)
        require(identity(source, limit) == expected, f'input changed: {source}')
        tracked[str(source)] = expected
        collector.retain('inputs/' + name, read(source, limit), str(source))
    for spec in plan['files']:
        source = Path(spec['source']).resolve(strict=True)
        require(source.is_relative_to(SCRATCH), f'file outside task scratch: {source}')
        data = read(source, limit)
        found = {'bytes': len(data), 'sha256': digest(data)}
        require(str(source) not in tracked or tracked[str(source)] == found,
                f'repeated input changed: {source}')
        tracked[str(source)] = found
        if spec.get('png'):
            require(data.startswith(b'\x89PNG\r\n\x1a\n'), 'invalid selected PNG')
        collector.retain(H['safe_name'](spec['path']), data, str(source), viewable_png=bool(spec.get('png')))
    for spec in plan['archives']:
        root = Path(spec['root']).resolve(strict=True)
        require(root.is_relative_to(SCRATCH), f'archive outside task scratch: {root}')
        collector.archive(spec['name'], root)
    for manifest in ['first-identities.json', 'final-identities.json']:
        rows = json.loads(read(SCRATCH / manifest, limit))
        for row in rows['binaries'].values():
            path = Path(row['executed'])
            expected = {'bytes': row['bytes'], 'sha256': row['sha256']}
            require(identity(path, limit) == expected, f'executed binary changed: {path}')
            tracked[str(path)] = expected
    collector.verify_encoded()
    collector.put('encoding-ledger.json', encode(collector.encodings))
    collector.put('archive-ledger.json', encode(collector.archives))
    collector.put('collection.json', encode({
        'schema_version': 1, 'script_sha256': digest(script_data),
        'plan_sha256': digest(plan_data), 'final_source_paths': len(frozen['sources']),
        'raw_payload_bytes': collector.raw_bytes, 'limitations': plan['limitations'],
        'claim': 'Stable retained bytes and identities; no automatic requirement certification.'}))
    inventory = {name: identity(path, limit) for name, path in H['safe_tree'](stage)}
    require(sum(row['bytes'] for row in inventory.values()) < limit, 'stored package over bound')
    require(sum(row['decoded_bytes'] for row in collector.encodings) + len(encode(inventory)) < limit,
            'decoded package over bound')
    collector.put('files.json', encode(inventory))
    require({name: identity(path, limit) for name, path in H['safe_tree'](stage) if name != 'files.json'} == inventory,
            'final inventory differs')
    for path, expected in tracked.items():
        require(identity(Path(path), limit) == expected, f'source changed while collecting: {path}')
    for root, expected in collector.witnesses.items():
        require({name: identity(path, limit) for name, path in H['safe_tree'](Path(root))} == expected,
                f'archive changed: {root}')
    H['exclusive_publish'](stage, destination)
    published = True
    print(json.dumps({'published': str(destination), 'payloads': len(inventory),
                      'bytes': sum(row['bytes'] for row in inventory.values()),
                      'inventory_sha256': digest(encode(inventory))}))
finally:
    if not published:
        shutil.rmtree(stage)
