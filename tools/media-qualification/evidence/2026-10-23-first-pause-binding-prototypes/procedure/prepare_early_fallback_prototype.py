#!/usr/bin/env python3
"""Prepare a guarded early-fallback prototype source tree; no build or run."""
from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
from pathlib import Path

ROOT = Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
PLAN = ROOT / 'next-profile-plan'
CANDIDATE_SOURCE = PLAN / 'prepared-candidate-source'
CANDIDATE_IDENTITIES = ROOT / 'candidate-all-core-final-source-identities.json'
PROTOTYPE_SOURCE = PLAN / 'early-fallback-prototype-source'
PROTOTYPE_TARGET = PLAN / 'early-fallback-prototype-target'
OVERLAY_TEXT = PLAN / 'early-fallback-prototype-overlay.rs.txt'
FULL_HASHES = PLAN / 'early-fallback-prototype-source-hashes.json'
RECEIPT = PLAN / 'early-fallback-prototype-guard-receipt.json'
AUDIT = PLAN / 'full-slice-early-fallback-audit.md'
CORE_FILE = 'crates/deadpan-core/src/audio_reference.rs'

ANCHOR = '''        if durations.len() != document.nodes().len() {
            return Ok(None);
        }
'''
INSERTION = '''        // Disposable prototype, justified by the full-slice audit: when at
        // least N-1 distinct aliases are required, the slice cannot be smaller.
        // See `full-slice-early-fallback-audit.md` for the proof and limits.
        if required.len() >= document.nodes().len().saturating_sub(1) {
            return Ok(None);
        }
'''


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def rust_paths(root: Path) -> list[str]:
    result = subprocess.run(
        ['rg', '--files', '-g', '*.rs'], cwd=root, check=True,
        capture_output=True, text=True,
    )
    return sorted(result.stdout.splitlines())


def rust_hashes(root: Path) -> dict[str, str]:
    return {relative: sha256_file(root / relative) for relative in rust_paths(root)}


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2) + '\n')


def main() -> None:
    if not CANDIDATE_SOURCE.is_dir() or not CANDIDATE_IDENTITIES.is_file() or not AUDIT.is_file():
        raise SystemExit('immutable candidate source, identity guard, or sibling audit is missing')
    for path in (PROTOTYPE_SOURCE, PROTOTYPE_TARGET, OVERLAY_TEXT, FULL_HASHES, RECEIPT):
        if path.exists():
            raise SystemExit(f'prototype destination already exists: {path}')

    expected = json.loads(CANDIDATE_IDENTITIES.read_text())
    candidate_before = rust_hashes(CANDIDATE_SOURCE)
    if candidate_before != expected:
        raise SystemExit('prepared candidate Rust hashes do not match the final identity manifest')
    candidate_manifest_sha256 = sha256_file(CANDIDATE_IDENTITIES)

    def ignore_source_build_outputs(directory: str, names: list[str]) -> set[str]:
        root = Path(directory).resolve() == CANDIDATE_SOURCE.resolve()
        return {name for name in names if name == '.git' or (name == 'target' and root)}

    shutil.copytree(CANDIDATE_SOURCE, PROTOTYPE_SOURCE, ignore=ignore_source_build_outputs)
    PROTOTYPE_TARGET.mkdir()
    prototype_file = PROTOTYPE_SOURCE / CORE_FILE
    before_text = prototype_file.read_text()
    if before_text.count(ANCHOR) != 1:
        raise SystemExit(f'early-fallback anchor must occur once; found {before_text.count(ANCHOR)}')
    before_sha256 = sha256_file(prototype_file)
    prototype_file.write_text(before_text.replace(ANCHOR, ANCHOR + INSERTION, 1))
    after_sha256 = sha256_file(prototype_file)

    overlay_bytes = prototype_file.read_bytes()
    OVERLAY_TEXT.write_bytes(overlay_bytes)
    candidate_after = rust_hashes(CANDIDATE_SOURCE)
    if candidate_after != candidate_before:
        raise SystemExit('immutable candidate source changed during prototype preparation')
    prototype_hashes = rust_hashes(PROTOTYPE_SOURCE)
    changed = [
        relative for relative in sorted(set(expected) | set(prototype_hashes))
        if expected.get(relative) != prototype_hashes.get(relative)
    ]
    if changed != [CORE_FILE]:
        raise SystemExit(f'prototype must change exactly {CORE_FILE}; changed {changed}')
    if set(expected) != set(prototype_hashes):
        raise SystemExit('prototype source did not preserve the complete candidate Rust path set')

    write_json(FULL_HASHES, {
        'candidate_manifest': str(CANDIDATE_IDENTITIES),
        'candidate_manifest_sha256': candidate_manifest_sha256,
        'candidate': candidate_before,
        'prototype': prototype_hashes,
    })
    receipt = {
        'preparation_only': True,
        'script_executed': True,
        'cargo_invoked': False,
        'git_invoked': False,
        'runtime_or_benchmark_invoked': False,
        'source_candidate': str(CANDIDATE_SOURCE),
        'source_candidate_immutable': True,
        'candidate_identity_manifest_sha256': candidate_manifest_sha256,
        'candidate_rust_file_count': len(candidate_before),
        'prototype_source': str(PROTOTYPE_SOURCE),
        'future_target_directory': str(PROTOTYPE_TARGET),
        'future_target_directory_empty': True,
        'prototype_overlay_file': str(OVERLAY_TEXT),
        'overlay_sha256': sha256_file(OVERLAY_TEXT),
        'guard_file': CORE_FILE,
        'guard_position': 'after precharge, derived-clocks fallback, durations acquisition, and duration-count check',
        'guard_expression': 'required.len() >= document.nodes().len().saturating_sub(1)',
        'guard_action': 'return Ok(None), preserving the caller complete-capture fallback',
        'audit': str(AUDIT),
        'audit_sha256': sha256_file(AUDIT),
        'before_sha256': before_sha256,
        'after_sha256': after_sha256,
        'changed_rust_paths': changed,
        'prototype_rust_file_count': len(prototype_hashes),
        'full_source_hashes': str(FULL_HASHES),
        'core_schema_version': 57,
        'store_schema_version': 86,
        'version_rationale': 'pure internal prototype; wire format and persistence schema are unchanged',
        'full_capture_checks_preserved': True,
        'no_other_production_or_test_edits': True,
    }
    write_json(RECEIPT, receipt)
    print(json.dumps({
        'prototype_source': str(PROTOTYPE_SOURCE),
        'future_target_directory': str(PROTOTYPE_TARGET),
        'overlay': str(OVERLAY_TEXT),
        'candidate_rust_files': len(candidate_before),
        'changed_rust_paths': changed,
        'cargo_invoked': False,
    }, indent=2))


if __name__ == '__main__':
    main()
