"""Compact completed acceptance text; never execute qualification commands."""
from pathlib import Path, PurePosixPath
import ast
import gzip
import hashlib
import io
import json
import re
import tarfile

ROOT = Path('/Users/michael/Code/deadpan/tools/model-qualification/evidence/2026-10-08-extension-native')


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def load(path):
    raw = path.read_bytes()
    return json.loads(gzip.decompress(raw) if path.suffix == '.gz' else raw)


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def tar_bytes(entries):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode='w', format=tarfile.USTAR_FORMAT) as archive:
        for name, raw in sorted(entries.items()):
            info = tarfile.TarInfo(name)
            info.size = len(raw)
            info.mode = 0o644
            info.uid = info.gid = info.mtime = 0
            info.uname = info.gname = ''
            archive.addfile(info, io.BytesIO(raw))
    result = io.BytesIO()
    with gzip.GzipFile(filename='', mode='wb', fileobj=result, mtime=0, compresslevel=9) as compressed:
        compressed.write(output.getvalue())
    return result.getvalue()


def read_tar(raw):
    # Never extract to disk; reject links, duplicate names and traversal.
    result = {}
    with tarfile.open(fileobj=io.BytesIO(gzip.decompress(raw)), mode='r:') as archive:
        names = []
        for member in archive.getmembers():
            assert member.isfile() and not member.issym() and not member.islnk()
            path = PurePosixPath(member.name)
            assert len(path.parts) == 1 and path.name == member.name and not path.is_absolute()
            assert member.name not in result
            assert member.mode == 0o644 and member.uid == member.gid == member.mtime == 0
            assert member.uname == member.gname == ''
            raw_member = archive.extractfile(member).read()
            assert len(raw_member) == member.size
            names.append(member.name)
            result[member.name] = raw_member
        assert names == sorted(names)
    return result


PATTERNS = {
    'private-key': re.compile(r'-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----'),
    'github-token': re.compile(r'\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})\b'),
    'provider-token': re.compile(r'\b(?:sk-(?:proj-)?[A-Za-z0-9_-]{24,}|hf_[A-Za-z0-9]{30,}|xox[baprs]-[A-Za-z0-9-]{20,})\b'),
    'aws-key': re.compile(r'\b(?:AKIA|ASIA)[A-Z0-9]{16}\b'),
    'bearer-value': re.compile(r'(?i)authorization["\s:=]+bearer\s+[A-Za-z0-9._~-]{20,}'),
    'credential-assignment': re.compile(r'(?i)["\s](?:api_key|access_token|client_secret|password)["\s]*[:=]["\s]*[A-Za-z0-9/+_=.-]{20,}'),
}


def validate_text(name, raw, counts):
    value = raw.decode('utf-8')
    assert '\x00' not in value, name
    for rule, pattern in PATTERNS.items():
        assert not pattern.search(value), f'credential marker: {rule} in {name}'
    counts['text_payloads'] += 1
    if name.endswith('.json'):
        json.loads(value)
        counts['json_payloads'] += 1
    elif name.endswith('.stdout') and value.lstrip().startswith(('{', '[')):
        try:
            json.loads(value)
        except json.JSONDecodeError:
            for line in value.splitlines():
                if line.strip():
                    json.loads(line)
        counts['json_or_jsonl_stdout_payloads'] += 1
    elif name.endswith('.py'):
        ast.parse(value, filename=name)
        counts['python_ast_payloads'] += 1


def validate_all():
    counts = dict.fromkeys(['text_payloads', 'json_payloads', 'json_or_jsonl_stdout_payloads',
                           'python_ast_payloads', 'gzip_files', 'tar_archives', 'tar_members'], 0)
    for path in sorted(ROOT.rglob('*')):
        assert not path.is_symlink(), path
        if not path.is_file() or path.name == 'manifest.json':
            continue
        name = path.relative_to(ROOT).as_posix()
        raw = path.read_bytes()
        if name.endswith('.tar.gz'):
            counts['gzip_files'] += 1
            counts['tar_archives'] += 1
            for entry, payload in read_tar(raw).items():
                counts['tar_members'] += 1
                validate_text(name + '/' + entry, payload, counts)
        else:
            if name.endswith('.gz'):
                counts['gzip_files'] += 1
                raw = gzip.decompress(raw)
                name = name[:-3]
            validate_text(name, raw, counts)
    return counts


def main():
    before_files = [p for p in ROOT.rglob('*') if p.is_file()]
    before = {'files': len(before_files), 'bytes': sum(p.stat().st_size for p in before_files)}
    production_hashes = {p.relative_to(ROOT).as_posix(): sha(p.read_bytes())
                         for p in (ROOT / 'production').rglob('*') if p.is_file()}
    inventory = load(ROOT / 'inventory.json')
    native_ax = Path(__file__).resolve().parent / 'native-ax.json'
    native_ax_raw = native_ax.read_bytes()
    native_ax_record = json.loads(native_ax_raw)
    assert native_ax_record['after_document_matches_retained_acceptance_receipt']
    with (ROOT / 'native-ax.json').open('xb') as output:
        output.write(native_ax_raw)
    inventory['copied'].append({'retained': 'native-ax.json', 'source': str(native_ax),
        'source_bytes': len(native_ax_raw), 'source_sha256': sha(native_ax_raw), 'encoding': 'verbatim'})
    case_names = [case['case'] for case in load(ROOT / 'cases.json')]
    assert len(case_names) == len(set(case_names)) == 8
    old_entries = {entry['retained']: entry for entry in inventory['copied']}
    assert len(old_entries) == len(inventory['copied'])
    archives, remove = [], []
    for case in case_names:
        folder = ROOT / 'acceptance' / case
        paths = sorted(folder.iterdir())
        assert len(paths) == 85
        entries, mappings = {}, []
        for path in paths:
            assert path.is_file() and not path.is_symlink() and path.suffix == '.gz'
            relative = path.relative_to(ROOT).as_posix()
            original = old_entries[relative]
            assert original['encoding'] == 'gzip'
            raw = gzip.decompress(path.read_bytes())
            assert len(raw) == original['source_bytes'] and sha(raw) == original['source_sha256']
            assert Path(original['source']).read_bytes() == raw
            name = path.name[:-3]
            entries[name] = raw
            mappings.append({'entry': name, 'source': original['source'],
                             'source_bytes': len(raw), 'source_sha256': sha(raw),
                             'replaced_retained': relative})
            remove.append((path, sha(path.read_bytes()), raw))
        encoded = tar_bytes(entries)
        assert tar_bytes(entries) == encoded, 'archive construction must be deterministic'
        archive = ROOT / 'acceptance' / (case + '.tar.gz')
        with archive.open('xb') as stream:
            stream.write(encoded)
        reopened = read_tar(archive.read_bytes())
        assert reopened == entries
        for mapping in mappings:
            assert sha(reopened[mapping['entry']]) == mapping['source_sha256']
        archives.append({'archive': archive.relative_to(ROOT).as_posix(),
                         'bytes': len(encoded), 'sha256': sha(encoded),
                         'entries': mappings})
    assert len(remove) == 680

    # Review every old payload and the newly created archives before deleting.
    validate_all()
    save(ROOT / 'acceptance/archive-inventory.json', {
        'schema_version': 1,
        'encoding': 'USTAR sorted regular files; uid/gid/mtime 0; mode 0644; empty owner names; gzip filename empty, mtime 0, level 9',
        'roundtrip': 'Every member equals both the prior gzip payload and the original scratch file; all raw SHA-256 and lengths verified before deletion.',
        'archive_count': 8, 'entry_count': 680, 'archives': archives,
    })
    removed_names = {p.relative_to(ROOT).as_posix() for p, _, _ in remove}
    inventory['copied'] = [entry for entry in inventory['copied'] if entry['retained'] not in removed_names]
    inventory['archives'] = [{k: v for k, v in archive.items() if k != 'entries'} |
                             {'entry_count': len(archive['entries']), 'inventory': 'acceptance/archive-inventory.json'}
                             for archive in archives]

    accepted = load(ROOT / 'acceptance/summary.json.gz')
    assert accepted['passed'] and accepted['complete_batch'] and accepted['qualified_count'] == 8
    assert accepted['cli_sha256'] == accepted['cli_sha256_after']
    results = []
    for case in accepted['cases']:
        assert case['passed'] and len(case['steps']) == 27 and all(s['exit_code'] == 0 for s in case['steps'])
        assert case['receipt']['contains_generated_pictures']
        checks = case['denials']
        for key in ['source-project', 'ai-runtime', 'default-model-root', 'development-model-cache']:
            assert checks[key]['positive']['readable'] and not checks[key]['negative']['readable']
            assert checks[key]['negative']['errno'] in [1, 13]
        assert checks['outbound-ip']['positive']['connected'] and not checks['outbound-ip']['negative']['connected']
        results.append({k: case[k] for k in ['case', 'passed', 'before_revision', 'accepted_revision',
                        'undo_revision', 'redo_revision', 'picture_frames', 'silent_audio', 'receipt']} |
                       {'successful_steps': len(case['steps']), 'denied': sorted(checks),
                        'archive': f"acceptance/{case['case']['case']}.tar.gz"})
        movie = Path(case['receipt']['movie'])
        data = movie.read_bytes()
        assert len(data) == case['receipt']['movie_bytes'] and sha(data) == case['receipt']['movie_sha256']
        inventory['local_only'].append({'path': str(movie), 'bytes': len(data), 'sha256': sha(data),
            'retained': False, 'reason': 'Verified emitted movie; text publication receipt and report are retained.'})
    save(ROOT / 'acceptance/results.json', {
        'derived_from': 'acceptance/summary.json.gz',
        'source_sha256': sha(gzip.decompress((ROOT / 'acceptance/summary.json.gz').read_bytes())),
        'passed': True, 'complete_batch': True, 'qualified_count': 8,
        'cli_sha256': accepted['cli_sha256'], 'cases': results,
    })

    # Keep generated movies local, with independently checked SHA-256 identities.
    for case in load(ROOT / 'production/measurement-summary.json')['cases']:
        for role in ['native', 'sampled']:
            reference = case['objects'][role]
            path = Path(inventory['scratch_root']) / case['case'] / 'project.deadpan/Media/Generated' / ('blake3-' + reference['content']['digest'])
            raw = path.read_bytes()
            assert len(raw) == reference['byte_length']
            inventory['local_only'].append({'path': str(path), 'bytes': len(raw), 'sha256': sha(raw),
                'declared_blake3': reference['content']['digest'], 'retained': False,
                'reason': f'Production {role} master; exact qualification and oracle receipts retained.'})

    script_target = ROOT / 'scripts/compact-final-evidence.py'
    script_raw = Path(__file__).read_bytes()
    script_target.write_bytes(script_raw)
    inventory['copied'].append({'retained': script_target.relative_to(ROOT).as_posix(),
        'source': str(Path(__file__).resolve()), 'source_bytes': len(script_raw),
        'source_sha256': sha(script_raw), 'encoding': 'verbatim'})
    inventory['copied'].sort(key=lambda entry: entry['retained'])
    inventory['script_output_status'] = 'All eight production Ready results, exact pixel-oracle results, and acceptance/offline-export outputs are complete and retained. Scripts and earlier failure evidence remain exact copies.'
    inventory['intentionally_not_retained'] = [
        'Complete app bundles, model weights, caches, replay media and private signing keys',
        'Generated native/sampled masters and emitted movies: local paths, byte counts and hashes retained',
        'Large full per-frame replay traces and source patches: local paths, byte counts and hashes retained',
        'bakeoff-recipe.txt, warm-runtime-recipe.txt, readonly-export-design.md: separate next-slice research',
    ]
    inventory['derived'] = ['README.md', 'acceptance/results.json', 'acceptance/archive-inventory.json', 'validation.json']
    inventory['review'] = 'Retained payloads reviewed for task relevance; UTF-8, JSON/JSONL, Python AST and credential-marker validation passed. No private key, model or media file copied.'
    save(ROOT / 'inventory.json', inventory)

    # Delete only the exact collector-generated per-case gzip copies whose
    # archive bytes were already reopened, checked and inventoried above.
    for path, compressed_hash, raw in remove:
        assert sha(path.read_bytes()) == compressed_hash and gzip.decompress(path.read_bytes()) == raw
        path.unlink()
    for case in case_names:
        (ROOT / 'acceptance' / case).rmdir()
    assert production_hashes == {p.relative_to(ROOT).as_posix(): sha(p.read_bytes())
                                 for p in (ROOT / 'production').rglob('*') if p.is_file()}
    for entry in inventory['copied']:
        raw = (ROOT / entry['retained']).read_bytes()
        if entry['encoding'] == 'gzip':
            raw = gzip.decompress(raw)
        else:
            assert entry['encoding'] == 'verbatim'
        assert len(raw) == entry['source_bytes'] and sha(raw) == entry['source_sha256']
        assert Path(entry['source']).read_bytes() == raw
    counts = validate_all()
    save(ROOT / 'validation.json', {
        'schema_version': 1, 'passed': True,
        'archive_count': 8, 'archive_members_verified': 680,
        'redundant_case_gzip_files_removed': 680,
        'archives_reconstructed_twice_identically': True,
        'original_copies_verified': len(inventory['copied']),
        'production_files_unchanged': len(production_hashes),
        'payload_counts_before_this_report': counts,
        'credential_patterns_checked': sorted(PATTERNS),
        'credential_findings': [],
        'review_scope': 'Task-specific qualification payloads and scripts; paths and lifecycle cancellation tokens are provenance, not external credentials.',
        'execution': 'File processing and syntax checks only; no qualification script, Cargo, native app, model or media helper executed.',
    })
    counts = validate_all()
    manifest = {p.relative_to(ROOT).as_posix(): {'bytes': p.stat().st_size, 'sha256': sha(p.read_bytes())}
                for p in sorted(ROOT.rglob('*')) if p.is_file() and p.name != 'manifest.json'}
    save(ROOT / 'manifest.json', manifest)
    assert load(ROOT / 'manifest.json') == manifest
    assert set(manifest) == {p.relative_to(ROOT).as_posix() for p in ROOT.rglob('*') if p.is_file() and p.name != 'manifest.json'}
    after_files = [p for p in ROOT.rglob('*') if p.is_file()]
    print(json.dumps({'before': before, 'after': {'files': len(after_files), 'bytes': sum(p.stat().st_size for p in after_files)},
                      'archive_members': len(remove), 'validation': counts}, indent=2))


if __name__ == '__main__':
    main()
