"""Independent read-only O archive/source audit. Run only after Root confirms build completion."""
from pathlib import Path, PurePosixPath
from collections import Counter, defaultdict
import argparse
import gc
import gzip
import hashlib
import json
import os
import re
import zlib

CHUNK = 1024 * 1024
MAX_METADATA = 8 * CHUNK
MAX_RECORDS = 20000
SUFFIXES = {'.json', '.jsonl', '.py', '.stdout', '.stderr', '.txt', '.md', '.rs', '.patch', '.diff'}
SKIP = {'main', 'before-main', 'target', '__pycache__', '.git', 'payloads'}
MAGIC = (b'\x7fELF', b'MZ', b'\xfe\xed\xfa\xce', b'\xce\xfa\xed\xfe',
         b'\xfe\xed\xfa\xcf', b'\xcf\xfa\xed\xfe', b'\xca\xfe\xba\xbe')
BASE = Path('/tmp/deadpan-O-publication-finalization-2026-11-01')
HOME = Path('/tmp/deadpan-stage-block-policy-admission-2026-11-01')
OUT = Path(__file__).resolve().parent
ROOTS = {'main': Path('/Users/michael/Code/deadpan'),
         'scoped': Path('/tmp/deadpan-query-groundwork-2026-10-26/scoped')}
DOCS = {'docs/spec/AGENT_HANDOFF.md', 'docs/REQUIREMENTS.md',
        'docs/design/projected-policy-source-admission.md',
        'docs/qualification/stage-block-policy-admission-2026-11-01.md'}
FIXTURES = {'crates/deadpan-audio/tests/fixtures/stage-block-policy-baseline-v1/corpus.json',
            'crates/deadpan-audio/tests/fixtures/stage-block-policy-baseline-v1/README.md'}
MIRROR_REL = Path('tools/media-qualification/evidence/2026-11-01-stage-block-policy-admission')


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def digest_stream(stream):
    digest = hashlib.sha256()
    size = 0
    head = b''
    while block := stream.read(CHUNK):
        if len(head) < 4:
            head = (head + block)[:4]
        size += len(block)
        digest.update(block)
    return {'bytes': size, 'sha256': digest.hexdigest()}, head


def fingerprint(path):
    with path.open('rb') as stream:
        return digest_stream(stream)[0]


def pin(path):
    return {'path': str(path), **fingerprint(path)}


def signature(path):
    stat = path.stat()
    return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns


def metadata(path):
    with path.open('rb') as stream:
        raw = stream.read(MAX_METADATA + 1)
    check(len(raw) <= MAX_METADATA, 'metadata limit: ' + str(path))
    return json.loads(raw), {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}


def safe(name):
    check(type(name) is str, 'path type')
    path = PurePosixPath(name)
    check(path.parts and not path.is_absolute() and '..' not in path.parts, 'unsafe path ' + name)
    check(path.as_posix() == name, 'noncanonical path ' + name)
    return name


def same_json(left, right):
    return json.dumps(left, sort_keys=True, separators=(',', ':')) == json.dumps(right, sort_keys=True, separators=(',', ':'))


def integer(value, positive=False):
    check(type(value) is int and value >= (1 if positive else 0), 'integer field')


def enumerate_inputs(config):
    inputs = {}
    exclusions = []
    for directory in config['directories']:
        root = Path(directory['path'])
        check(root.is_dir() and not root.is_symlink(), 'selected root')
        check(not OUT.is_relative_to(root.resolve()), 'auditor output selected: hash cycle')
        prefix = safe(directory['prefix'].strip('/'))
        for parent, dirs, names in os.walk(root, followlinks=False):
            for name in sorted(dirs):
                if name in SKIP:
                    exclusions.append({'path': str(Path(parent) / name), 'reason': 'excluded_directory_' + name})
            dirs[:] = sorted(name for name in dirs if name not in SKIP)
            for leaf in sorted(names):
                source = Path(parent) / leaf
                relative = source.relative_to(root)
                journal = any(part.endswith('.journal') for part in relative.parts[:-1])
                if source.suffix not in SUFFIXES and not journal:
                    exclusions.append({'path': str(source), **fingerprint(source),
                                       'reason': 'native_binary_or_non_evidence_suffix'})
                    continue
                name = safe(prefix + '/' + relative.as_posix())
                check(name not in inputs, 'duplicate directory alias ' + name)
                check(source.is_file() and not source.is_symlink(), 'regular selected file')
                inputs[name] = source
    for row in config.get('files', []):
        name = safe(row['name'])
        check(name not in inputs, 'duplicate explicit alias ' + name)
        inputs[name] = Path(row['path'])
    owned, _ = metadata(Path(config['owned_code_paths']))
    check(type(owned) is list and len(owned) == len(set(owned)) == 17, '17 owned source paths')
    for relative in owned:
        name = safe('shipping-source/' + safe(relative))
        source = Path(config['scope_root']) / relative
        check(name not in inputs or inputs[name].resolve() == source.resolve(), 'shipping collision')
        inputs[name] = source
    check(len(inputs) <= MAX_RECORDS, 'record limit')
    for name, source in inputs.items():
        check(source.is_file() and not source.is_symlink(), 'selected file ' + name)
        journal = any(part.endswith('.journal') for part in PurePosixPath(name).parts[:-1])
        check(source.suffix in SUFFIXES or journal, 'unsupported input ' + name)
        with source.open('rb') as stream:
            head = stream.read(4)
        check(not any(head.startswith(magic) for magic in MAGIC), 'native executable input ' + name)
    return inputs, owned, exclusions


def stored_and_expanded(path, row):
    packed_hash = hashlib.sha256()
    expanded_hash = hashlib.sha256()
    packed_size = expanded_size = 0
    first = b''
    decoder = zlib.decompressobj(31)
    with path.open('rb') as stream:
        header = stream.read(10)
        check(header == b'\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\xff', 'canonical gzip header ' + str(path))
        stream.seek(0)
        while block := stream.read(CHUNK):
            packed_size += len(block)
            packed_hash.update(block)
            check(not decoder.eof, 'trailing gzip member/data')
            pending = block
            while True:
                expanded = decoder.decompress(pending, CHUNK)
                pending = decoder.unconsumed_tail
                if len(first) < 4:
                    first = (first + expanded)[:4]
                expanded_size += len(expanded)
                expanded_hash.update(expanded)
                check(not decoder.unused_data, 'extra gzip member/trailing bytes')
                if decoder.eof or (not pending and len(expanded) < CHUNK):
                    break
        check(decoder.eof and not decoder.unconsumed_tail, 'truncated gzip stream')
    check((packed_size, packed_hash.hexdigest()) == (row['stored_bytes'], row['stored_sha256']), 'stored digest')
    check((expanded_size, expanded_hash.hexdigest()) == (row['bytes'], row['sha256']), 'expanded digest')
    check(not any(first.startswith(magic) for magic in MAGIC), 'expanded native executable')
    return expanded_size


def reference(row):
    check(set(row) == {'schema', 'ordinal', 'file', 'kind', 'bytes', 'sha256'}, 'reference fields')
    check(row['schema'] == 'stage-block-policy-evidence-ref-v1', 'reference schema')
    integer(row['ordinal'], True)
    integer(row['bytes'], True)
    check(row['file'] == f"case-{row['ordinal']:08}.json", 'case leaf')
    check(row['kind'] in ('callback', 'live_use'), 'case kind')
    check(type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'case hash')


def body_references(value):
    found = {}
    def visit(node):
        if isinstance(node, dict):
            if node.get('schema') == 'stage-block-policy-evidence-ref-v1':
                reference(node)
                check(node['ordinal'] not in found, 'duplicate body reference')
                found[node['ordinal']] = node
            else:
                for child in node.values():
                    visit(child)
        elif isinstance(node, list):
            for child in node:
                visit(child)
    for key, child in value.items():
        if key != 'evidence_journal':
            visit(child)
    return [found[key] for key in sorted(found)]


def canonical_readback(body, stream):
    digest = hashlib.sha256()
    size = 0
    fragments = []
    buffered = 0
    for fragment in json.JSONEncoder(sort_keys=True, separators=(',', ':')).iterencode(body):
        fragments.append(fragment)
        buffered += len(fragment)
        if buffered >= 65536:
            block = ''.join(fragments).encode()
            check(stream.read(len(block)) == block, 'canonical bytes')
            digest.update(block)
            size += len(block)
            fragments = []
            buffered = 0
    block = (''.join(fragments) + '\n').encode()
    check(stream.read(len(block)) == block and not stream.read(1), 'canonical end')
    digest.update(block)
    return {'bytes': size + len(block), 'sha256': digest.hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', type=Path, default=BASE)
    parser.add_argument('--output', type=Path, default=OUT / 'actual-audit.json')
    parser.add_argument('--require-mirrors', action='store_true')
    args = parser.parse_args()
    base = args.base.resolve()
    archive = base / 'archive'
    check(not args.output.exists(), 'create-new audit output')
    config_path = base / 'configuration.json'
    freeze_path = base / 'directory-freeze.json'
    config, config_pin = metadata(config_path)
    freeze, freeze_pin = metadata(freeze_path)
    proof, proof_pin = metadata(base / 'archive-build-proof.json')
    inventory, inventory_pin = metadata(archive / 'inventory.json')
    check(config['status'] == 'final' and proof['all_stored_and_expanded_hashes_verified']
          and proof['input_signatures_and_final_digests_verified'], 'actual completed build required')
    check(proof['payload_io_chunk_bytes'] == CHUNK, 'stream convention')
    check(inventory['schema'] == 'deadpan-qualified-gzip-evidence-v1', 'inventory schema')
    check(inventory_pin == {'bytes': proof['inventory_bytes'], 'sha256': proof['inventory_sha256']}, 'inventory pin')
    check(inventory['configuration'] == config_pin and inventory['scope'] == config['scope']
          and inventory['base_revision'] == config['base_revision'] == '52ac739bb447262223ba6f0e50563de3d4faab04', 'config/scope/base')
    inputs, owned, exclusions = enumerate_inputs(config)
    discovered = {str(p) for p in Path('/tmp').iterdir() if p.is_dir()
                  and p.name.startswith('deadpan-stage-block-policy') and '2026-11-01' in p.name}
    check(discovered == {d['path'] for d in config['directories']}, 'no omitted current O evidence directories')
    rows = inventory['records']
    check(len(rows) <= MAX_RECORDS and len(rows) == len({r['path'] for r in rows}) == proof['records'], 'unique aliases')
    byname = {safe(row['path']): row for row in rows}
    check(set(inputs) == set(byname), 'exact configured archive membership')
    freeze_name = safe(config['membership_freeze_record'])
    check(inputs[freeze_name].resolve() == freeze_path.resolve(), 'freeze alias')
    check(inventory['membership_freeze'] == {'path': freeze_name, **freeze_pin}, 'freeze inventory pin')
    check(freeze['schema'] == 'O-final-input-membership-freeze-v1' and freeze['all_input_writers_finished'] is True, 'finished freeze')
    check(Path(freeze['configuration']['path']).resolve() == config_path.resolve()
          and {k: freeze['configuration'][k] for k in ('bytes', 'sha256')} == config_pin, 'freeze/config')
    frozen = {row['path']: row for row in freeze['records']}
    check(len(frozen) == len(freeze['records']) and set(frozen) == set(inputs) - {freeze_name}, 'freeze membership')
    before = {name: signature(path) for name, path in inputs.items()}
    for number, (name, source) in enumerate(sorted(inputs.items()), 1):
        row = byname[name]
        for field in ('bytes', 'stored_bytes'):
            integer(row[field], field == 'stored_bytes')
        for field in ('sha256', 'stored_sha256'):
            check(re.fullmatch('[0-9a-f]{64}', row[field]), 'inventory digest syntax')
        check(fingerprint(source) == {k: row[k] for k in ('bytes', 'sha256')}, 'physical source alias ' + name)
        if name in frozen:
            frozen_row = frozen[name]
            check(Path(frozen_row['source']).resolve() == source.resolve(), 'frozen source identity')
            check({k: frozen_row[k] for k in ('bytes', 'sha256')} == {k: row[k] for k in ('bytes', 'sha256')}, 'frozen bytes')
        if number % 250 == 0:
            print('source aliases', number, flush=True)
    diff_names = [name for name in inputs if name.endswith('.diff')]
    check(len(diff_names) == 11, 'all11 reviewed diff evidence files retained')
    shipping = {name.removeprefix('shipping-source/') for name in byname if name.startswith('shipping-source/')}
    check(shipping == set(owned) | FIXTURES | DOCS and len(shipping) == 23, '17 code + 2 fixture + 4 docs')
    check(not any(str(path).startswith('/Users/michael/Code/deadpan/') for path in inputs.values()), 'no Main whole source input')
    check(not any(path.suffix == '.orig' for path in inputs.values()), 'no local engine salt payload')
    stored = {}
    for row in rows:
        expected = 'payloads/' + row['sha256'] + '.gz'
        check(safe(row['stored_path']) == expected, 'content-addressed path')
        previous = stored.setdefault(expected, row)
        check(all(previous[k] == row[k] for k in ('bytes', 'sha256', 'stored_bytes', 'stored_sha256')), 'equal physical aliases')
    check(len(stored) == proof['unique_payloads'], 'unique blob count')
    check(sum(r['bytes'] for r in rows) == proof['expanded_bytes_with_aliases'], 'all logical bytes')
    check(sum(r['stored_bytes'] for r in stored.values()) == proof['stored_bytes'], 'stored bytes')
    physical = {p.relative_to(archive).as_posix() for p in archive.rglob('*') if p.is_file()}
    check(physical == set(stored) | {'inventory.json'}, 'exact physical archive leaves')
    packed_before = {name: signature(archive / name) for name in physical}
    expanded_unique = 0
    for number, (name, row) in enumerate(sorted(stored.items()), 1):
        path = archive / name
        check(not path.is_symlink(), 'stored symlink')
        expanded_unique += stored_and_expanded(path, row)
        if number % 100 == 0:
            print('stored/expanded payloads', number, flush=True)

    def archived_stream(name):
        return gzip.open(archive / byname[name]['stored_path'], 'rb')

    def archived_json(name):
        with archived_stream(name) as stream:
            raw = stream.read(MAX_METADATA + 1)
        check(len(raw) <= MAX_METADATA, 'archived metadata bound')
        return json.loads(raw)

    aliases = defaultdict(list)
    for name, path in inputs.items():
        aliases[path.resolve()].append(name)

    def source_name(path):
        names = aliases[Path(path).resolve()]
        check(names, 'authority missing from archive: ' + str(path))
        return names[0]

    def authority(row):
        name = source_name(row['path'])
        got = byname[name]
        for field in ('bytes', 'sha256'):
            if field in row:
                check(got[field] == row[field], 'archived authority ' + str(row['path']))
        return name

    acceptance_name = config['acceptance_record']
    acceptance = archived_json(acceptance_name)
    check(acceptance['acceptance_verdict'] == 'GO', 'runtime GO')
    check(inventory['acceptance'] == {'path': acceptance_name, 'sha256': byname[acceptance_name]['sha256']}, 'inventory acceptance')
    check(Path(proof['acceptance']['path']).resolve() == inputs[acceptance_name].resolve()
          and {k: proof['acceptance'][k] for k in ('bytes', 'sha256')} ==
          {k: byname[acceptance_name][k] for k in ('bytes', 'sha256')}, 'build acceptance')
    check(archived_json(source_name(config_path)) == config, 'archived config bytes')
    check(archived_json(freeze_name) == freeze, 'archived freeze bytes')
    for row in acceptance['authorities'].values():
        authority(row)
    semantic_name = authority(acceptance['authorities']['after_semantics_keys_work_lifetime'])
    semantic = archived_json(semantic_name)
    check(semantic['acceptance_verdict'] == 'GO' and semantic['findings'] == [], 'final semantic GO')
    runtime_review_name = source_name(Path('/tmp/deadpan-stage-block-policy-runtime-acceptance-review-2026-11-01/review.json'))
    runtime_review = archived_json(runtime_review_name)
    check(runtime_review['verdict'] == 'GO_BOUNDED_RUNTIME_ACCEPTANCE_ONLY'
          and runtime_review['findings'] == [], 'independent runtime acceptance GO')
    doc_review_name = source_name(Path('/tmp/deadpan-stage-block-policy-documentation-review-2026-11-01/v3-final-runtime/review.json'))
    doc_review = archived_json(doc_review_name)
    check(doc_review['verdict'] == 'GO_BOUNDED_RUNTIME_DOCUMENTATION_SUBJECT_TO_ACTUAL_ARCHIVE_RELEASE_GATES'
          and doc_review['findings'] == [], 'four-doc independent GO')
    doc_pins = archived_json(source_name(HOME / 'documentation-pins.json'))
    check(set(doc_pins) == DOCS, 'exact four documentation pins')
    for relative, expected_pin in doc_pins.items():
        check({k: byname['shipping-source/' + relative][k] for k in ('bytes', 'sha256')} == expected_pin, 'approved doc byte pins')
    actual_crypto = archived_json(authority(acceptance['authorities']['actual_crypto']))
    check(actual_crypto['acceptance_verdict'] == 'GO_ACTUAL_HASH_REPLAY_ONLY' and len(actual_crypto['executions']) == 8, 'actual crypto')
    recovered = {}
    for execution in actual_crypto['executions']:
        raw = bytes.fromhex(execution['input_hex'])
        check(len(raw) == execution['input_bytes'] and hashlib.sha256(raw).hexdigest() == execution['input_sha256'], 'exact crypto input recovery')
        check(execution['tool_before'] == execution['tool_after'] and execution['exit_code'] == 0
              and execution['actual_stdout'].strip() == execution['expected_digest']
              and execution['actual_stderr'] == '', 'actual per-invocation outcome')
        if execution['source'] is not None:
            expected = {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
            check(execution['source_before'] == execution['source_after'] == expected, 'payload source pins')
            check(Path(execution['source']).read_bytes() == raw, 'actual excluded payload recoverable')
            recovered[Path(execution['source']).resolve()] = expected
    check(len(recovered) == 6, 'six complete payload files recoverable verbatim')
    check(actual_crypto['alias_counts'] == {'actual_complete_aliases': 3648, 'actual_prefix_only_aliases': 1248}, 'crypto complete/prefix multiplicity')
    exclusion_name = source_name(base / 'input-exclusions-and-recoverability.json')
    exclusion_report = archived_json(exclusion_name)
    check(sorted(exclusion_report['exclusions'], key=lambda r: r['path']) == sorted(exclusions, key=lambda r: r['path']), 'all exclusions enumerated exactly')
    skipped_main = [r for r in exclusions if r['reason'] == 'excluded_directory_main']
    omitted_main_bytes = 0
    for row in skipped_main:
        for path in Path(row['path']).rglob('*'):
            if path.is_file():
                check(path.suffix == '.bin' and path.resolve() in recovered, 'no missing Main text evidence or unrecoverable payload')
                omitted_main_bytes += path.stat().st_size
    check(len(skipped_main) == 1 and omitted_main_bytes == 20390, 'documented two derived Main payloads only')

    journals = []
    cached_bodies = {}
    captures = config['final_journals']
    check(len(captures) == 4 and {(r['variant'], r['repeat']) for r in captures}
          == {(v, n) for v in ROOTS for n in (1, 2)}, 'four capture labels')
    check(all(type(r['repeat']) is int for r in captures), 'repeat integer')
    check(len({r['body_record'] for r in captures}) == 4
          and len({inputs[r['body_record']].resolve() for r in captures}) == 4, 'four independent body paths')
    for capture in captures:
        name = capture['body_record']
        row = byname[name]
        variant, repeat = capture['variant'], capture['repeat']
        complete_name = name + '.journal/COMPLETE.json'
        complete = archived_json(complete_name)
        check(set(complete) == {'schema', 'status', 'body', 'journal'}
              and complete['schema'] == 'stage-block-policy-evidence-publication-v1'
              and complete['status'] == 'complete', 'COMPLETE envelope')
        check(same_json(complete['body'], {k: row[k] for k in ('bytes', 'sha256')}), 'COMPLETE raw body')
        manifest = complete['journal']
        if row['sha256'] not in cached_bodies:
            with archived_stream(name) as stream:
                body = json.load(stream)
            check(body['schema'] == 'stage-block-policy-after-journal-v1', 'body schema')
            check(same_json(body['evidence_journal'], manifest), 'body/COMPLETE manifest')
            refs = body_references(body)
            provenance = {key: body.pop(key) for key in ('variant', 'source_sha256', 'published_checkpoint')}
            canonical_name = name.removesuffix('-body.json') + '-canonical.json'
            with archived_stream(canonical_name) as stream:
                canonical = canonical_readback(body, stream)
            check(canonical == {k: byname[canonical_name][k] for k in ('bytes', 'sha256')}, 'literal strip-only3 canonical')
            cached_bodies[row['sha256']] = {'manifest': manifest, 'refs': refs, 'provenance': provenance, 'canonical': canonical}
            del body
            gc.collect()
        cached = cached_bodies[row['sha256']]
        check(same_json(cached['manifest'], manifest), 'repeated body manifest')
        check(cached['provenance']['variant'] == ('main-inherited' if variant == 'main' else 'scoped-published'), 'variant')
        check(cached['provenance']['published_checkpoint'] == config['base_revision'], 'body checkpoint')
        label = f'stage-block-policy-after-capture{repeat}-{variant}'
        source_manifest_name = source_name(HOME / (label + '-source-manifest.json'))
        check(cached['provenance']['source_sha256'] == byname[source_manifest_name]['sha256'], 'body/source-manifest binding')
        check(manifest['schema'] == 'stage-block-policy-evidence-journal-v1' and manifest['status'] == 'complete'
              and manifest['root_convention'] == 'append .journal to raw body filepath', 'journal schema')
        catalog = manifest['files']
        integer(manifest['actual_cases']); integer(manifest['total_case_bytes'])
        check(manifest['actual_cases'] == len(catalog) == 1214 and same_json(cached['refs'], catalog), 'all body refs/catalog')
        index_name = name + '.journal/index.jsonl'
        check(same_json(manifest['index'], {'file': 'index.jsonl', **{k: byname[index_name][k] for k in ('bytes', 'sha256')}}), 'index pin')
        kinds = Counter()
        with archived_stream(index_name) as stream:
            for ordinal, ref in enumerate(catalog, 1):
                reference(ref)
                check(ref['ordinal'] == ordinal, 'catalog ordinal')
                line = stream.readline(MAX_METADATA + 1)
                check(len(line) <= MAX_METADATA and same_json(json.loads(line), ref), 'exact index rows')
                case_name = name + '.journal/' + ref['file']
                case_row = byname[case_name]
                check({k: case_row[k] for k in ('bytes', 'sha256')} == {k: ref[k] for k in ('bytes', 'sha256')}, 'full case bytes retained')
                kinds[ref['kind']] += 1
            check(not stream.read(1), 'extra index row')
        check(kinds == {'callback': 1176, 'live_use': 38}, 'case kind multiplicity')
        check(manifest['total_case_bytes'] == sum(ref['bytes'] for ref in catalog), 'case byte total')
        expected = {name + '.journal/' + ref['file'] for ref in catalog} | {index_name, complete_name}
        actual = {alias for alias in byname if alias.startswith(name + '.journal/')}
        check(actual == expected, 'no omitted/extra completed journal records')
        physical_root = Path(str(inputs[name]) + '.journal')
        check({p.name for p in physical_root.iterdir()} == {r['file'] for r in catalog} | {'index.jsonl', 'COMPLETE.json'}, 'physical completed journal closure')
        journals.append({'body_record': name, 'variant': variant, 'repeat': repeat,
                         'cases': len(catalog), 'kinds': dict(kinds), 'total_case_bytes': manifest['total_case_bytes']})
    summaries = [{k: row[k] for k in ('body_record', 'variant', 'repeat', 'cases', 'total_case_bytes')} for row in journals]
    check(summaries == inventory['final_journals'] == proof['final_journals'] == freeze['final_journals'], 'final capture summaries')
    check(sum(j['cases'] for j in journals) == 4856, 'all raw aliases preserved')

    matrix = archived_json(authority(acceptance['authorities']['matrix']))
    totals = {variant: Counter(targets=0, passed=0, failed=0, ignored=0, filtered=0) for variant in ROOTS}
    for target in matrix:
        receipt_name = source_name(HOME / (target['label'] + '.json'))
        receipt = archived_json(receipt_name)
        check(receipt['exit_code'] == 0 and receipt['sources_unchanged'] and receipt['binaries_unchanged'], 'matrix guarded success')
        for ext in ('stdout', 'stderr'):
            stream_name = source_name(HOME / (target['label'] + '.' + ext))
            check({k: byname[stream_name][k] for k in ('bytes', 'sha256')} == receipt['streams'][ext], 'matrix stream')
        with archived_stream(source_name(HOME / (target['label'] + '.stdout'))) as stream:
            text = stream.read(MAX_METADATA + 1).decode()
        check(len(text.encode()) <= MAX_METADATA, 'stdout bound')
        matches = re.findall(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;', text)
        check(len(matches) == 1, 'one actual target summary')
        status, passed, failed, ignored, measured, filtered = matches[0]
        check(status == 'ok' and int(failed) == 0 and int(measured) == 0, 'actual harness status')
        counts = dict(zip(('passed', 'failed', 'ignored', 'filtered'), map(int, (passed, failed, ignored, filtered))))
        check(all(target[k] == counts[k] for k in counts), 'actual stdout counts')
        totals[target['variant']]['targets'] += 1
        totals[target['variant']].update(counts)
    expected = {'main': {'targets': 21, 'passed': 1179, 'failed': 0, 'ignored': 42, 'filtered': 7},
                'scoped': {'targets': 21, 'passed': 1173, 'failed': 0, 'ignored': 42, 'filtered': 0}}
    check({k: dict(v) for k, v in totals.items()} == expected, 'actual all42 matrix totals')
    current_maps = {}
    artifact_rows = archived_json(authority(acceptance['authorities']['physical_artifacts']))['artifacts']
    check(len(artifact_rows) == 50, '50 actual Cargo pairs')
    for row in artifact_rows:
        expected_pin = {k: row[k] for k in ('bytes', 'sha256')}
        check(fingerprint(Path(row['original'])) == expected_pin == fingerprint(Path(row['copy'])), 'current original/copy artifact')
    for variant, root in ROOTS.items():
        build = archived_json(source_name(HOME / f'stage-block-policy-after-build3-{variant}.json'))
        source_map = build['source_after']
        check(len(source_map) == (3012 if variant == 'main' else 2696), 'current map count')
        for relative, expected_pin in source_map.items():
            check(fingerprint(root / relative) == expected_pin, 'current guarded source ' + variant + '/' + relative)
        current_maps[variant] = len(source_map)
        for relative in shipping:
            check(fingerprint(root / relative) == {k: byname['shipping-source/' + relative][k] for k in ('bytes', 'sha256')}, 'curated source/docs equal ' + variant + '/' + relative)
    oracle_proof = archived_json(authority(acceptance['authorities']['current_source_and_frozen43']))
    check(oracle_proof['owned17_equal'] and oracle_proof['immutable43_unchanged_in_both_roots'], '43 source proof')
    # The archived final proof records every oracle with original pins; tolerate only the actual documented root layout.
    oracles = oracle_proof['oracles']
    if isinstance(oracles, dict):
        oracle_rows = list(oracles.values())
    else:
        oracle_rows = oracles
    checked_oracles = set()
    for row in oracle_rows:
        if isinstance(row, dict) and 'path' in row and 'sha256' in row:
            relative = row['path']
            for root in ROOTS.values():
                path = root / relative
                check(path.is_file(), 'oracle path ' + relative)
                got = fingerprint(path)
                check(got['sha256'] == row['sha256'] and ('bytes' not in row or got['bytes'] == row['bytes']), 'oracle bytes')
            checked_oracles.add(relative)
    check(len(checked_oracles) == 43, 'all43 oracle rows independently read')
    scoped_build = archived_json(source_name(HOME / 'stage-block-policy-after-build3-scoped.json'))
    salts = [p for p in scoped_build['source_after'] if p.endswith('.orig')]
    check(len(salts) == 5, 'five local engine salts')
    failed = []
    for path in sorted(HOME.glob('*.json')):
        if path.stat().st_size > MAX_METADATA or any(x in path.name for x in ('-body', '-canonical', '-source-manifest')):
            continue
        try:
            value, _ = metadata(path)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and 'command' in value and value.get('exit_code', 0) != 0:
            name = source_name(path)
            for ext, expected_pin in value.get('streams', {}).items():
                stream_name = source_name(path.with_suffix('.' + ext))
                check({k: byname[stream_name][k] for k in ('bytes', 'sha256')} == expected_pin, 'retained failed stream')
            failed.append({'path': name, 'exit_code': value['exit_code']})
    check(failed and any(r['exit_code'] < 0 for r in failed), 'failed and interrupted actual history retained')
    authored_links = []
    candidate = Path('/tmp/deadpan-stage-block-policy-documentation-candidate-v3-2026-11-01')
    for fragment, relative in [('handoff-section.md', 'docs/spec/AGENT_HANDOFF.md'),
                               ('requirements-section.md', 'docs/REQUIREMENTS.md'),
                               ('design-section.md', 'docs/design/projected-policy-source-admission.md'),
                               ('qualification.md', 'docs/qualification/stage-block-policy-admission-2026-11-01.md')]:
        fragment_name = source_name(candidate / fragment)
        with archived_stream(fragment_name) as stream:
            text = stream.read(MAX_METADATA + 1).decode()
        with archived_stream('shipping-source/' + relative) as stream:
            actual_doc = stream.read(MAX_METADATA + 1).decode()
        check(text.strip() in actual_doc, 'approved final fragment literal in shipping doc')
        for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)', text):
            if '://' in target or target.startswith('#'):
                continue
            clean = target.split('#', 1)[0]
            for variant, root in ROOTS.items():
                path = ((root / relative).parent / clean).resolve()
                if path == (root / MIRROR_REL / 'inventory.json').resolve():
                    check(inventory_pin['sha256'] == proof['inventory_sha256'], 'intended inventory link exact primary')
                    if path.exists():
                        check(fingerprint(path) == inventory_pin, 'existing inventory mirror link')
                    authored_links.append({'root': variant, 'document': relative, 'link': target,
                                           'primary_inventory_verified': True, 'mirror_present': path.exists()})
                else:
                    check(path.is_file(), 'supported qualification link missing ' + str(path))
                    authored_links.append({'root': variant, 'document': relative, 'link': target, **fingerprint(path)})
    mirrors = []
    for variant, root in ROOTS.items():
        destination = root / MIRROR_REL
        if not destination.exists():
            check(not args.require_mirrors, 'mirror required ' + variant)
            mirrors.append({'root': variant, 'present': False, 'certified': False})
            continue
        present = {p.relative_to(destination).as_posix() for p in destination.rglob('*') if p.is_file()}
        check(present == physical | {'archive-build-proof.json'}, 'mirror exact primary/build supplement')
        for leaf in physical:
            check(not (destination / leaf).is_symlink() and fingerprint(destination / leaf) == fingerprint(archive / leaf), 'mirror bytes ' + variant + '/' + leaf)
        check(fingerprint(destination / 'archive-build-proof.json') == proof_pin, 'mirror build proof')
        mirrors.append({'root': variant, 'present': True, 'certified': True, 'files': len(present)})
    final_inputs, _, _ = enumerate_inputs(config)
    check(final_inputs == inputs, 'late selected membership change')
    check(all(signature(path) == before[name] for name, path in inputs.items()), 'late source mutation')
    check(all(signature(archive / name) == packed_before[name] for name in physical), 'late archive mutation')
    check(metadata(config_path)[1] == config_pin and metadata(freeze_path)[1] == freeze_pin
          and metadata(archive / 'inventory.json')[1] == inventory_pin, 'late authority mutation')
    result = {'schema': 'O-independent-final-archive-audit-v1',
        'verdict': 'GO_BOUNDED_ARCHIVE_REFERENCE_AND_CURATED_SOURCE', 'findings': [],
        'configuration': pin(config_path), 'freeze': pin(freeze_path), 'inventory': pin(archive / 'inventory.json'),
        'build_proof': pin(base / 'archive-build-proof.json'),
        'counts': {'records': len(rows), 'unique_payloads': len(stored), 'frozen_records': len(frozen),
                   'stored_bytes': proof['stored_bytes'], 'expanded_bytes_with_aliases': proof['expanded_bytes_with_aliases'],
                   'expanded_unique_payload_bytes': expanded_unique, 'diff_records': len(diff_names)},
        'all_source_aliases_frozen_and_rehashed': True, 'all_unique_stored_and_expanded_streams_independently_checked': True,
        'canonical_single_member_gzip_headers_and_crc_valid': True,
        'all_four_complete_body_canonical_catalog_index_case_reference_chains_checked': journals,
        'actual_matrix_totals_from_archived_stdout': {k: dict(v) for k, v in totals.items()},
        'current_guarded_source_maps': current_maps, 'curated_shipping_paths': sorted(shipping),
        'all43_oracles_current_exact_both_roots': True, 'all50_original_and_copy_artifacts_current_exact': True,
        'exclusion_records': len(exclusions), 'six_excluded_crypto_payloads_fully_recoverable_from_archived_hex': len(recovered),
        'excluded_local_engine_salts': salts, 'retained_failed_guarded_runs': failed, 'mirrors': mirrors,
        'runtime_and_four_doc_independent_go_archived': [runtime_review_name, doc_review_name],
        'supported_authored_document_links': authored_links,
        'limits': ['Archive/source/reference inspection only; no new native/compiler/helper/test/Git execution.',
                   'Full captured native/key semantics use their exact archived independent GO authorities; no new cryptographic implementation.',
                   'One unique raw body is parsed at a time for canonical/reference closure; compact manifest/reference metadata is reused only under independently verified exact full-body SHA. No aggregate RSS bound.',
                   'Absent mirrors are explicitly not certified; final gate/publication/remote tree remain Root-owned separate supplements.',
                   'Scope source is authoritative; no inherited Main WIP, native executable bytes or five local .orig salts are published.']}
    with args.output.open('x') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print(json.dumps({'verdict': result['verdict'], 'counts': result['counts'], 'report': pin(args.output)}), flush=True)


if __name__ == '__main__':
    main()
