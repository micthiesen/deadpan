"""Copy final compact audio-binding qualification evidence into the repository.

This is intentionally a packaging script. It does not run builds, tests, or
qualification steps, and must be run only after the final qualification driver
and root-owned media/performance checks have completed.
"""
from pathlib import Path
import gzip
import hashlib
import json

SCRATCH = Path('/tmp/deadpan-compact-audio-bindings-2026-10-23')
DEST = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-23-compact-audio-bindings')
EXCLUDED_SUFFIXES = ('.tar.zst', '.sqlite', '.sqlite3', '.db')
PAYLOAD_SUFFIXES = {'.py', '.json', '.log', '.stdout', '.stderr', '.txt', '.md'}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canon(value):
    return (json.dumps(value, sort_keys=True, indent=2) + '\n').encode()


def put(name, data):
    target = DEST / name
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        if target.read_bytes() != data:
            raise RuntimeError(f'conflicting destination evidence; refusing overwrite: {name}')
        return
    target.write_bytes(data)


def retain(name, source, compress=False):
    if not source.is_file():
        raise RuntimeError(f'missing required evidence: {source}')
    data = source.read_bytes()
    if compress and len(data) > 100_000 and source.suffix in {'.json', '.log', '.txt'}:
        name += '.gz'
        data = gzip.compress(data, mtime=0)
    put(name, data)


def load_json(path):
    if not path.is_file():
        raise RuntimeError(f'missing required receipt: {path}')
    return json.loads(path.read_text())


def require_exit_receipt(path):
    value = load_json(path)
    if not isinstance(value, dict) or value.get('exit') != 0:
        raise RuntimeError(f'receipt does not record exit 0: {path}')
    return value


def require_command_rows(path, expected_count=None, unchanged_key=None):
    value = load_json(path)
    if not isinstance(value, list) or not value:
        raise RuntimeError(f'expected a nonempty command receipt list: {path}')
    if expected_count is not None and len(value) != expected_count:
        raise RuntimeError(f'expected {expected_count} receipt rows in {path}, got {len(value)}')
    for row in value:
        if not isinstance(row, dict) or row.get('exit') != 0:
            raise RuntimeError(f'non-success command row in {path}')
        if unchanged_key and row.get(unchanged_key) is not True:
            raise RuntimeError(f'{unchanged_key} is not true in {path}')
    return value


def require_run_result(phase, tests, binaries):
    result = load_json(SCRATCH / f'{phase}-result.json')
    expected = {'exit': 0, 'tests': tests, 'binaries': binaries,
                'sources_inputs_executables_unchanged': True}
    for key, wanted in expected.items():
        if result.get(key) != wanted:
            raise RuntimeError(f'{phase}-result.json {key} expected {wanted!r}, got {result.get(key)!r}')
    require_exit_receipt(SCRATCH / f'{phase}-run-receipt.json')
    return result


def validate_prerequisites():
    # Each completed nextest phase needs both a passing result with exact
    # totals and the command receipt; the latter does not encode test counts.
    require_run_result('candidate-all-core-final', 1513, 69)
    require_run_result('candidate-broad-final', 3499, 109)
    require_run_result('candidate-ui', 1292, 2)

    # Keep the first qualification receipt even though it records an intended
    # failed broad-100 phase. Only the final qualification batch is gated here.
    load_json(SCRATCH / 'final-qualification-receipt.json')
    final_rows = load_json(SCRATCH / 'final-qualification-2-receipt.json')
    if not isinstance(final_rows, list) or len(final_rows) != 7 or any(
            not isinstance(row, dict) or row.get('exit') != 0 for row in final_rows):
        raise RuntimeError('final-qualification-2-receipt.json must contain exactly seven successful rows')

    require_exit_receipt(SCRATCH / 'candidate-strict-check-receipt.json')
    require_exit_receipt(SCRATCH / 'candidate-strict-format-receipt.json')
    lint_rows = require_command_rows(SCRATCH / 'ui-lint-receipt.json')
    rendered_lint = json.dumps(lint_rows).lower()
    if 'clippy' not in rendered_lint or 'fmt' not in rendered_lint:
        raise RuntimeError('ui-lint receipt must contain both clippy and format rows')
    for name in ('debug-media-build-receipt.json', 'debug-media-neutral-receipt.json',
                 'release-media-build-receipt.json', 'release-media-neutral-receipt.json',
                 'release-media-aligned-receipt.json', 'release-native-build-receipt.json',
                 'release-native-performance-receipt.json'):
        require_exit_receipt(SCRATCH / name)

    portable = load_json(SCRATCH / 'owner-portable-check' / 'receipt.json')
    if not isinstance(portable, dict) or len(portable.get('cases', [])) != 2:
        raise RuntimeError('owner-portable-check/receipt.json must contain exactly two cases')
    for case in portable['cases']:
        if not isinstance(case, dict) or not case.get('commands'):
            raise RuntimeError('portable case must contain command results')
        for command in case['commands']:
            if command.get('exit') != 0 or command.get('package_paths_and_bytes_unchanged') is not True:
                raise RuntimeError('portable command must pass and preserve package paths/bytes')

    summary_path = SCRATCH / 'controlled-performance' / 'summary.json'
    summary = load_json(summary_path)
    groups = summary.get('summary', {})
    if any(groups.get(mode, {}).get(part, {}).get('n') != 5
           for mode in ('baseline', 'candidate') for part in ('first', 'second')):
        raise RuntimeError('controlled-performance summary must report n=5 for all four groups')
    trials = load_json(SCRATCH / 'controlled-performance' / 'trials.json')
    if not isinstance(trials, list) or len(trials) != 10 or any(sum(row.get('mode') == mode for row in trials) != 5
                                            for mode in ('baseline', 'candidate')):
        raise RuntimeError('controlled-performance trials.json must contain five trials per mode')
    require_command_rows(SCRATCH / 'controlled-performance' / 'commands.json', 20)
    perf_validation = load_json(SCRATCH / 'performance-validation' / 'receipt.json')
    perf_cases = perf_validation.get('cases', []) if isinstance(perf_validation, dict) else []
    if len(perf_cases) != 5 or any(not isinstance(row, dict) or row.get('exit') != 0 or
            row.get('package_unchanged') is not True or row.get('response', {}).get('valid') is not True
            for row in perf_cases):
        raise RuntimeError('performance-validation/receipt.json must have five passing unchanged-package cases')
    portable_copied = SCRATCH / 'owner-portable-check' / 'receipt.json'
    if not portable_copied.is_file():
        raise RuntimeError('portable receipt is required')


def excluded(path):
    name = path.name
    if name.endswith(EXCLUDED_SUFFIXES):
        return True
    if path.parent == SCRATCH and any(path.name.endswith(suffix) for suffix in
                                      ('-source', '-extracted', '-inputs')):
        return True
    if any(part.endswith('.deadpan') for part in path.parts):
        return True
    # Self-captures and output from the copy operation are never source evidence.
    if name in {'inventory.json', 'retain_final_evidence.stdout', 'retain_final_evidence.stderr'}:
        return True
    return False


def copy_tree(source, prefix, suffixes=None):
    if not source.exists():
        return
    for path in sorted(source.rglob('*')):
        if path.is_symlink() or not path.is_file() or excluded(path):
            continue
        if path.suffix not in (suffixes if suffixes is not None else PAYLOAD_SUFFIXES):
            continue
        retain(f'{prefix}/{path.relative_to(source)}', path, compress=True)


def main():
    validate_prerequisites()

    # The root driver owns these real-media and portable package payloads. They
    # are already in the destination when this script runs; retain only a
    # scratch-side mirror if one exists, and never replace an existing file.
    copy_tree(SCRATCH / 'media-evidence', 'media')
    copy_tree(SCRATCH / 'owner-portable-check', 'owner-portable-check')

    # Compact phase receipts and exact source overlays. Binary archives,
    # extracted target trees and duplicate input trees are deliberately omitted.
    for path in sorted(SCRATCH.iterdir()):
        if not path.is_file() or excluded(path) or path.suffix not in PAYLOAD_SUFFIXES:
            continue
        retain(f'procedure/{path.name}', path, compress=True)
    for overlay in sorted(SCRATCH.glob('*-overlays')):
        if not overlay.is_dir():
            continue
        phase = overlay.name.removesuffix('-overlays')
        for path in sorted(overlay.rglob('*.rs.txt')):
            if path.is_file():
                retain(f'{phase}/source-overlays/{path.relative_to(overlay)}', path)

    # Keep the old-codec witness and its initial-toolchain receipt, including
    # failed stderr/stdout and test sources, while excluding any executable.
    for folder in ('old-binding-codec-witness', 'old-binding-codec-witness-initial-toolchain'):
        copy_tree(SCRATCH / folder, folder)

    # Controlled performance is retained as raw text/JSON records, excluding
    # generated .deadpan package trees themselves.
    for folder in ('controlled-performance', 'controlled', 'performance-validation',
                   'release-native-performance', 'release-native'):
        copy_tree(SCRATCH / folder, folder)
    copy_tree(SCRATCH / 'candidate-broad-final-evidence', 'candidate-broad-final-evidence')
    copy_tree(SCRATCH / 'candidate-broad-evidence', 'candidate-broad-evidence')

    # Record exactly what this packager did and intentionally left out.
    script_data = Path(__file__).read_bytes()
    put('procedure/retain_final_evidence.py', script_data)
    metadata = {
        'packager_sha256': sha(script_data),
        'destination': str(DEST),
        'intentionally_excluded': [
            'binary build outputs and executable payloads',
            '*.tar.zst archives', '*-source folders', '*-extracted folders',
            '*-inputs duplicate trees', 'generated *.deadpan benchmark packages',
            'live tool output and self-capture files',
        ],
        'failed_phases': 'retained when represented by eligible top-level receipts/logs or witness files',
    }
    put('procedure/packager-metadata.json', canon(metadata))

    # Inventory includes existing media payloads, excludes itself, and is
    # independently checked against the exact resulting path set and bytes.
    inventory_path = DEST / 'inventory.json'
    expected_paths = sorted(str(p.relative_to(DEST)) for p in DEST.rglob('*')
                             if p.is_file() and p != inventory_path)
    rows = {}
    for rel in expected_paths:
        data = (DEST / rel).read_bytes()
        rows[rel] = {'bytes': len(data), 'sha256': sha(data)}
    inventory = canon({'files': rows})
    put('inventory.json', inventory)
    final = json.loads(inventory_path.read_text())
    actual_paths = sorted(str(p.relative_to(DEST)) for p in DEST.rglob('*')
                          if p.is_file() and p != inventory_path)
    if actual_paths != sorted(final['files']):
        raise RuntimeError('inventory path set differs from destination payload set')
    for rel, item in final['files'].items():
        data = (DEST / rel).read_bytes()
        if len(data) != item['bytes'] or sha(data) != item['sha256']:
            raise RuntimeError(f'inventory verification failed: {rel}')
    print(json.dumps({'files': len(rows), 'bytes': sum(x['bytes'] for x in rows.values()),
                      'inventory_sha256': sha(inventory)}, sort_keys=True))


if __name__ == '__main__':
    main()
