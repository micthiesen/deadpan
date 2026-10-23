"""Package verified compact frozen-edge evidence. Intentionally not executed here."""
from pathlib import Path
import gzip
import hashlib
import json

SCRATCH = Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
SOURCE = Path('/tmp/deadpan-first-pause-profile-2026-10-22')
DEST = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-compact-frozen-edges')
BASE_COMMIT = 'a18ff65443aa8fd1aa5ab63fa33678c9afd88220'
INPUT_NAMES = ['Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml', '.cargo/config.toml',
               'crates/deadpan-app/src/navigation/registry/baseline-routing.txt', 'docs/COMMANDS.md']
EXCLUDED = ('.tar.zst', '.sqlite', '.sqlite3', '.db')
EXCLUDED_DIRS = {'next-profile-plan', 'baseline-target'}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canon(value):
    return (json.dumps(value, sort_keys=True, indent=2) + '\n').encode()


def put(name, data):
    target = DEST / name
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists() and target.read_bytes() != data:
        raise RuntimeError(f'conflicting destination file: {name}')
    target.write_bytes(data)


def retain(name, path, compress_json=False):
    if not path.is_file():
        raise RuntimeError(f'missing evidence: {path}')
    data = path.read_bytes()
    if compress_json and path.suffix == '.json' and len(data) > 100_000:
        name += '.gz'
        data = gzip.compress(data, mtime=0)
    put(name, data)


def raw_json(name, path):
    retain(name, path, compress_json=True)


def main():
    metadata_path = SOURCE / 'source.json'
    base_manifest_path = SOURCE / 'source-identities.json'
    base_inputs_path = SCRATCH / 'baseline-core-inputs.json'
    for required in (metadata_path, base_manifest_path, base_inputs_path):
        if not required.is_file():
            raise RuntimeError(f'missing immutable source receipt: {required}')
    metadata = json.loads(metadata_path.read_text())
    if metadata.get('commit') != BASE_COMMIT:
        raise RuntimeError('immutable base commit mismatch')
    base = SOURCE / 'source'
    base_ids = json.loads(base_manifest_path.read_text())
    for rel, digest in base_ids.items():
        path = base / rel
        if not path.is_file() or sha(path.read_bytes()) != digest:
            raise RuntimeError(f'base source identity mismatch: {rel}')
    base_inputs = json.loads(base_inputs_path.read_text())
    if set(base_inputs) != set(INPUT_NAMES):
        raise RuntimeError('baseline receipt must contain exactly the six known inputs')
    for rel, digest in base_inputs.items():
        path = base / rel
        if not path.is_file() or sha(path.read_bytes()) != digest:
            raise RuntimeError(f'base input identity mismatch: {rel}')

    host_path = SCRATCH / 'host.json'
    if not host_path.is_file():
        raise RuntimeError('host.json must be supplied before packaging')
    json.loads(host_path.read_text())
    put('source-base.json', canon({
        'git_commit': BASE_COMMIT,
        'source_archive_sha256': metadata.get('archive_sha256'),
        'source_metadata': 'prototype/first-pause-source.json',
        'prior_compact_prototype_evidence': 'tools/media-qualification/evidence/2026-10-23-first-pause-profile',
        'reconstruction': 'unpacked immutable source plus exact frozen phase overlays; changed Rust files are stored as .rs.txt',
    }))
    raw_json('prototype/first-pause-source.json', metadata_path)
    raw_json('prototype/first-pause-source-identities.json', base_manifest_path)
    raw_json('prototype/first-pause-inputs.json', SOURCE / 'inputs.json')

    manifests = sorted(SCRATCH.glob('*-source-identities.json'))
    if not manifests:
        raise RuntimeError('no phase source identity maps found')
    phases = [p.name.removesuffix('-source-identities.json') for p in manifests]
    phases_by_length = sorted(phases, key=len, reverse=True)
    for manifest, phase in zip(manifests, phases):
        ids = json.loads(manifest.read_text())
        overlays = SCRATCH / f'{phase}-overlays'
        full = SCRATCH / f'{phase}-source'
        for rel, wanted in ids.items():
            if not rel.endswith('.rs'):
                raise RuntimeError(f'non-Rust entry in phase map: {phase}/{rel}')
            overlay = overlays / (rel + '.txt')
            full_source = full / rel
            candidate = overlay if overlay.is_file() else full_source if full_source.is_file() else None
            if candidate is None:
                if base_ids.get(rel) != wanted:
                    raise RuntimeError(f'unreconstructable source: {phase}/{rel}')
                continue
            data = candidate.read_bytes()
            if sha(data) != wanted:
                raise RuntimeError(f'frozen source hash mismatch: {phase}/{rel}')
            if base_ids.get(rel) != wanted:
                put(f'{phase}/source-overlays/{rel}.txt', data)
        raw_json(f'{phase}/source-identities.json', manifest)

        input_path = SCRATCH / f'{phase}-inputs.json'
        if not input_path.is_file():
            input_path = SCRATCH / f'{phase}-lock.json'
        if not input_path.is_file():
            raise RuntimeError(f'missing phase inputs/lock manifest: {phase}')
        phase_inputs = json.loads(input_path.read_text())
        if set(phase_inputs) != set(INPUT_NAMES):
            raise RuntimeError(f'{phase} must retain exactly six input hashes')
        raw_json(f'{phase}/inputs.json', input_path)
        for rel, wanted in phase_inputs.items():
            if base_inputs.get(rel) == wanted:
                continue
            candidates = [SCRATCH / f'{phase}-inputs' / rel, SCRATCH / 'baseline' / rel]
            found = next((p for p in candidates if p.is_file()), None)
            if found is None or sha(found.read_bytes()) != wanted:
                raise RuntimeError(f'unreconstructable input: {phase}/{rel}')
            retain(f'{phase}/inputs/{rel}', found)

        for path in sorted(SCRATCH.iterdir()):
            if not path.is_file() or not path.name.startswith(phase):
                continue
            owner = next((p for p in phases_by_length if path.name.startswith(p)), None)
            if owner != phase or path == manifest or path == input_path:
                continue
            if path.name.endswith(EXCLUDED):
                continue
            if path.suffix in ('.log', '.txt', '.json', '.stderr', '.stdout'):
                retain(f'{phase}/receipts/{path.name}', path, compress_json=True)
        for folder in (SCRATCH / f'{phase}-evidence', SCRATCH / f'{phase}-performance'):
            if not folder.is_dir():
                continue
            for path in sorted(folder.rglob('*')):
                if not path.is_file() or any(part in EXCLUDED_DIRS or part.endswith('-extracted') for part in path.parts):
                    continue
                if path.name.endswith(EXCLUDED) or 'sqlite' in path.name.lower():
                    continue
                retain(f'{phase}/{folder.name}/{path.relative_to(folder)}', path, compress_json=True)

    # Retain only this controlled batch, never unrelated archived evidence.
    controlled = SCRATCH / 'controlled'
    if not (controlled / 'summary.json').is_file():
        raise RuntimeError('completed current-format controlled summary is required')
    packages = {}
    for path in sorted(controlled.rglob('*')):
        if not path.is_file():
            continue
        rel = path.relative_to(controlled)
        if any(part.endswith('.deadpan') for part in rel.parts):
            continue
        if path.name.endswith(EXCLUDED):
            continue
        retain(f'controlled/{rel}', path, compress_json=True)
    for package in sorted(controlled.iterdir()):
        if not package.is_dir() or not package.name.endswith('.deadpan'):
            continue
        records = {}
        for path in sorted(package.rglob('*')):
            if path.is_symlink():
                raise RuntimeError(f'unexpected controlled package symlink: {path}')
            if path.is_file():
                data = path.read_bytes()
                records[str(path.relative_to(package))] = {'bytes': len(data), 'sha256': sha(data)}
        packages[package.name] = records
    if len(packages) != 10:
        raise RuntimeError(f'expected ten fresh controlled packages, got {len(packages)}')
    put('controlled/package-file-hashes.json', canon(packages))

    # Retain global orchestration and attribution, including the packager itself.
    for path in sorted(SCRATCH.iterdir()):
        if not path.is_file() or path.name.endswith(EXCLUDED):
            continue
        if path.suffix in ('.py', '.md', '.json', '.log', '.stderr', '.stdout', '.txt'):
            retain(f'procedure/{path.name}', path, compress_json=True)
    retain('procedure/retain_evidence.py', SCRATCH / 'retain_evidence.py')
    retain('host.json', host_path)

    # Historical-codec outputs and the test-only strengthening are small,
    # reconstructable records; executable bytes remain outside the repository.
    for folder_name in ('historical-codec-probe', 'final-test-refinement'):
        folder = SCRATCH / folder_name
        if not folder.is_dir():
            continue
        for path in sorted(folder.rglob('*')):
            if not path.is_file() or path.suffix not in ('.json', '.rs', '.txt', '.md', '.stdout', '.stderr', '.py'):
                continue
            rel = str(path.relative_to(folder))
            if path.suffix == '.rs':
                rel += '.txt'
            retain(f'{folder_name}/{rel}', path, compress_json=True)

    # The media copier publishes directly to this destination separately.
    # Preserve only any already-created media metadata, never SQLite payloads.
    media = SCRATCH / 'media-evidence'
    if media.is_dir():
        for path in sorted(media.rglob('*')):
            if path.is_file() and not path.name.endswith(EXCLUDED):
                retain(f'media-evidence/{path.relative_to(media)}', path, compress_json=True)

    if any(path.suffix == '.rs' for path in DEST.rglob('*')):
        raise RuntimeError('Rust source must be retained only with .rs.txt suffix')
    inventory = {}
    for path in sorted(DEST.rglob('*')):
        if path.is_file() and path != DEST / 'inventory.json':
            data = path.read_bytes()
            inventory[str(path.relative_to(DEST))] = {'bytes': len(data), 'sha256': sha(data)}
    put('inventory.json', canon({'source_base': BASE_COMMIT, 'phases': phases, 'files': inventory}))
    final = json.loads((DEST / 'inventory.json').read_text())
    for rel, expected in final['files'].items():
        data = (DEST / rel).read_bytes()
        if len(data) != expected['bytes'] or sha(data) != expected['sha256']:
            raise RuntimeError(f'inventory verification failed: {rel}')
    print(json.dumps({'phases': len(phases), 'files': len(inventory),
                      'bytes': sum(item['bytes'] for item in inventory.values()),
                      'inventory_sha256': sha((DEST / 'inventory.json').read_bytes())}, sort_keys=True))


if __name__ == '__main__':
    main()
