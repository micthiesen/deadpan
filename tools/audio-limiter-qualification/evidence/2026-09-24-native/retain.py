"""Collect frozen evidence once, without overwrites or executing any DSP."""
import gzip
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parent
SCRATCH = Path('/tmp/deadpan-master-20260924')
REPO = Path('/Users/michael/Code/deadpan')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    archive_path = ROOT / 'experiments.tar.gz'
    manifest_path = ROOT / 'retention.json'
    assert not archive_path.exists() and not manifest_path.exists(), 'refuse overwrite'
    content, inventory = {}, {}
    excluded = []
    excluded_files = {}

    def excluded_identity(path):
        data = path.read_bytes()
        excluded_files[str(path)] = {
            'bytes': len(data), 'sha256': sha(data),
            'embedded_rustc_paths': sorted({value.decode() for value in re.findall(rb'/rustc/[0-9a-f]{40}', data)}),
        }

    def add(name, source, expected=None):
        path = PurePosixPath(name)
        assert not path.is_absolute() and '..' not in path.parts
        assert str(path) == name and name not in content
        assert source.is_file() and not source.is_symlink(), source
        data = source.read_bytes()
        digest = sha(data)
        if expected is not None:
            assert digest == expected, source
        content[name] = data
        inventory[name] = {'source': str(source), 'bytes': len(data), 'sha256': digest}

    def tree(base):
        source = SCRATCH / base
        assert source.is_dir(), source
        entries = sorted(source.rglob('*'))
        assert entries
        for path in entries:
            if not path.is_file():
                continue
            parts = path.relative_to(source).parts
            if ({'target', 'bin', '__pycache__'}.intersection(parts)
                    or any(part.endswith('.dSYM') for part in parts)
                    or path.suffix in {'.o', '.pyc'}
                    or path.name in {'finite_peak_abi_probe', 'finite-peak-abi-probe', 'deadpan-native-limiter-audit'}):
                excluded.append(str(path))
                if 'bin' in parts or path.name in {'finite_peak_abi_probe', 'finite-peak-abi-probe', 'deadpan-native-limiter-audit'}:
                    excluded_identity(path)
                continue
            add(base + '/' + path.relative_to(source).as_posix(), path)

    for base in ['candidate', 'kernel-timing', 'kernel-timing-root-profile',
                 'pipeline-timing', 'pipeline-timing-cached', 'design-review',
                 'native-review', 'bus-cache', 'sanitizer-final', 'native-audit-rust1971']:
        tree(base)
    add('generate_coefficients.py', SCRATCH / 'generate_coefficients.py')
    add('compiler-provenance-review.md', SCRATCH / 'compiler-provenance-review.md')
    for name in ['bench.rs', 'fft_detector.cpp']:
        add('fft-map/' + name, SCRATCH / 'fft-map' / name)

    native_root = SCRATCH / 'native-audit'
    native_inventory = json.loads((native_root / 'retention-inventory.json').read_text())
    native_binary = native_root / native_inventory['authoritative_run'] / 'deadpan-native-limiter-audit'
    excluded_identity(native_binary)
    assert excluded_files[str(native_binary)]['sha256'] == native_inventory['binary_sha256']
    add('native-audit/retention-inventory.json', native_root / 'retention-inventory.json')
    for entry in native_inventory['files']:
        path = PurePosixPath(entry['path'])
        assert not path.is_absolute() and '..' not in path.parts
        assert 'target' not in path.parts and path.name != 'deadpan-native-limiter-audit'
        source = native_root / entry['path']
        assert source.stat().st_size == entry['bytes']
        add('native-audit/' + entry['path'], source, entry['sha256'])

    # Every workspace manifest is retained so path dependencies can inherit the
    # exact workspace configuration. No build products or repository artifacts.
    roots = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo', 'crates', 'native']
    names = subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--', *roots],
        cwd=REPO).decode().split('\0')
    source_names = sorted({name for name in names if name and (REPO / name).is_file()})
    assert len(source_names) > 100
    for name in source_names:
        add('final-source/' + name, REPO / name)

    # Preflight all original references before publishing either archive file.
    assert sum(len(data) for data in content.values()) < 256 * 1024 * 1024
    for name, record in inventory.items():
        assert sha(Path(record['source']).read_bytes()) == record['sha256'], name

    with archive_path.open('xb') as raw:
        with gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0, compresslevel=9) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as archive:
                for name in sorted(content):
                    info = tarfile.TarInfo(name)
                    info.size = len(content[name])
                    info.mode = 0o644
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ''
                    archive.addfile(info, io.BytesIO(content[name]))

    # A changed source is a failed seal, not permission to silently take new bytes.
    for name, record in inventory.items():
        assert sha(Path(record['source']).read_bytes()) == record['sha256'], name
    manifest = {
        'schema_version': 1,
        'repository_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO).decode().strip(),
        'scope': 'Retained finite-limiter experiments, compiled output audit, exact bus-cache pipeline evidence and final source snapshot; no new DSP execution',
        'archive_sha256': sha(archive_path.read_bytes()),
        'archive_bytes': archive_path.stat().st_size,
        'uncompressed_file_bytes': sum(len(data) for data in content.values()),
        'deterministic_archive': 'Sorted regular PAX tar files, mode0644 uid/gid/mtime0, gzip filename empty and mtime0',
        'omitted': ['Cargo targets, executable binaries, object files and Python caches',
                    'FFT mapping executables, object files and dSYM bundle',
                    'full workspace gate and native GUI evidence, retained separately by root'],
        'excluded_paths': sorted(excluded),
        'excluded_file_identities': excluded_files,
        'original_native_executable_sha256': native_inventory['binary_sha256'],
        'pinned_native_executable_sha256': json.loads(content['native-audit-rust1971/provenance-1790247077077703000/environment.json'])['binary_sha256'],
        'files': inventory,
    }
    with manifest_path.open('x') as out:
        json.dump(manifest, out, indent=2)
        out.write('\n')
    print(json.dumps({'files': len(content), 'archive_bytes': manifest['archive_bytes'],
                      'sha256': manifest['archive_sha256']}))


if __name__ == '__main__':
    main()
