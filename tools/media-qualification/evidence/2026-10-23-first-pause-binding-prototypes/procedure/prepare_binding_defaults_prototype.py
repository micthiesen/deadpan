#!/usr/bin/env python3
"""Prepare a disposable binding-default wire prototype; never build or benchmark."""
from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
from pathlib import Path

ROOT = Path('/tmp/deadpan-compact-frozen-edges-2026-10-22')
PLAN = ROOT / 'next-profile-plan'
CANDIDATE = PLAN / 'prepared-candidate-source'
IDENTITIES = ROOT / 'candidate-all-core-final-source-identities.json'
INPUTS = ROOT / 'candidate-all-core-final-inputs.json'
SOURCE = PLAN / 'binding-defaults-prototype-source'
TARGET = PLAN / 'binding-defaults-prototype-target'
OVERLAY = PLAN / 'binding-defaults-prototype-overlay.rs.txt'
HASHES = PLAN / 'binding-defaults-prototype-source-hashes.json'
RECEIPT = PLAN / 'binding-defaults-prototype-receipt.json'
CORE = 'crates/deadpan-core/src/audio_binding.rs'


def sha_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def rust_paths(root: Path) -> list[str]:
    result = subprocess.run(['rg', '--files', '-g', '*.rs'], cwd=root,
                            check=True, capture_output=True, text=True)
    return sorted(result.stdout.splitlines())


def rust_hashes(root: Path) -> dict[str, str]:
    return {rel: sha_file(root / rel) for rel in rust_paths(root)}


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'{label} anchor must occur exactly once, found {count}')
    return text.replace(old, new, 1)


def main() -> None:
    if not CANDIDATE.is_dir() or not IDENTITIES.is_file() or not INPUTS.is_file():
        raise SystemExit('prepared candidate source or exact source/input manifests missing')
    for path in (SOURCE, TARGET, OVERLAY, HASHES, RECEIPT):
        if path.exists():
            raise SystemExit(f'prototype destination already exists: {path}')

    expected = json.loads(IDENTITIES.read_text())
    before = rust_hashes(CANDIDATE)
    if before != expected:
        raise SystemExit('candidate Rust hashes do not match final identity manifest')
    inputs = json.loads(INPUTS.read_text())
    if len(inputs) != 6:
        raise SystemExit('expected exactly six frozen build inputs')
    for rel, digest in inputs.items():
        path = CANDIDATE / rel
        if not path.is_file() or sha_file(path) != digest:
            raise SystemExit(f'frozen candidate input mismatch: {rel}')

    def ignore_outputs(directory: str, names: list[str]) -> set[str]:
        at_root = Path(directory).resolve() == CANDIDATE.resolve()
        return {name for name in names if name == '.git' or (at_root and name == 'target')}

    shutil.copytree(CANDIDATE, SOURCE, ignore=ignore_outputs)
    TARGET.mkdir()
    target_file = SOURCE / CORE
    original = target_file.read_text()
    transformed = original

    # Exact default for this one enum case; the tagged nondefault root variants
    # remain represented and validated by Serde as before.
    # Serde attributes attach to the root field only, after the helper functions
    # are introduced below. The enum case is represented by a precise predicate.
    transformed = replace_once(
        transformed,
        '    pub root: AudioClockRoot,',
        '    #[serde(default = "default_audio_clock_root", skip_serializing_if = "is_project_root_round_even")]\n    pub root: AudioClockRoot,',
        'AudioReferenceClock.root',
    )
    # Add helpers immediately after the enum without changing its variants.
    enum_end = '''pub enum AudioClockRoot {
    ProjectRootRoundEven,
    PreserveInputPointCeil { stage: NodeId },
    DefinitionPointCeil { root: NodeId },
    GapDefinitionPointCeil { repeat: NodeId },
}
'''
    helpers = '''
fn default_audio_clock_root() -> AudioClockRoot {
    AudioClockRoot::ProjectRootRoundEven
}
fn is_project_root_round_even(root: &AudioClockRoot) -> bool {
    matches!(root, AudioClockRoot::ProjectRootRoundEven)
}
'''
    transformed = replace_once(transformed, enum_end, enum_end + helpers, 'AudioClockRoot enum body')

    transformed = replace_once(
        transformed,
        '    #[serde(deserialize_with = "path_vec")]\n    pub arguments: Vec<AudioRepeatArgument>,',
        '    #[serde(default, deserialize_with = "path_vec")]\n    pub arguments: Vec<AudioRepeatArgument>,',
        'arguments empty default',
    )
    transformed = replace_once(
        transformed,
        '    #[serde(deserialize_with = "path_vec")]\n    pub births: Vec<AudioBirthClause>,',
        '    #[serde(default, deserialize_with = "path_vec")]\n    pub births: Vec<AudioBirthClause>,',
        'births empty default',
    )
    transformed = replace_once(
        transformed,
        'pub struct AudioLocalPhase {\n    pub constant: ExactRatio,',
        'pub struct AudioLocalPhase {\n    #[serde(default = "zero_audio_phase", skip_serializing_if = "is_zero_audio_phase")]\n    pub constant: ExactRatio,',
        'phase constant zero default',
    )
    transformed = replace_once(
        transformed,
        'pub struct AudioResume {\n    pub local_boundary: ExactRatio,',
        'pub struct AudioResume {\n    #[serde(default = "zero_audio_phase", skip_serializing_if = "is_zero_audio_phase")]\n    pub local_boundary: ExactRatio,',
        'resume local boundary zero default',
    )
    transformed = replace_once(
        transformed,
        'pub struct OwnedAudioBinding {\n    pub lattice: AudioPlacementTemplate,\n    pub resume: Option<AudioResume>,',
        'pub struct OwnedAudioBinding {\n    pub lattice: AudioPlacementTemplate,\n    #[serde(default, skip_serializing_if = "Option::is_none")]\n    pub resume: Option<AudioResume>,',
        'resume absent default',
    )
    transformed = replace_once(
        transformed,
        'fn zero_reference_local_offset() -> ExactRatio {',
        '''fn zero_audio_phase() -> ExactRatio { ExactRatio::ZERO }
fn is_zero_audio_phase(value: &ExactRatio) -> bool { *value == ExactRatio::ZERO }

fn zero_reference_local_offset() -> ExactRatio {''',
        'exact zero helper definitions',
    )

    target_file.write_text(transformed)
    overlay_bytes = target_file.read_bytes()
    OVERLAY.write_bytes(overlay_bytes)
    after = rust_hashes(SOURCE)
    candidate_after = rust_hashes(CANDIDATE)
    if candidate_after != before:
        raise SystemExit('immutable prepared candidate changed during copy/preparation')
    changed = sorted(rel for rel in set(before) | set(after) if before.get(rel) != after.get(rel))
    if changed != [CORE] or set(before) != set(after):
        raise SystemExit(f'prototype must change only {CORE}, found {changed}')

    write_json(HASHES, {
        'candidate_manifest': str(IDENTITIES),
        'candidate_manifest_sha256': sha_file(IDENTITIES),
        'candidate_rust_hashes': before,
        'prototype_rust_hashes': after,
        'candidate_inputs': inputs,
    })
    write_json(RECEIPT, {
        'experimental_nonshipping': True,
        'preparation_only': True,
        'script_executed': True,
        'cargo_rustc_benchmarks_git_invoked': False,
        'candidate_source': str(CANDIDATE),
        'candidate_source_unchanged': True,
        'prototype_source': str(SOURCE),
        'future_target_directory': str(TARGET),
        'overlay_file': str(OVERLAY),
        'overlay_sha256': sha_file(OVERLAY),
        'changed_rust_paths': changed,
        'rust_file_count': len(after),
        'frozen_input_count': len(inputs),
        'audit': str(PLAN / 'default-binding-wire-audit.md'),
        'omissions': [
            'AudioPlacementTemplate.arguments empty vector, with path_vec retained for present values',
            'AudioPlacementTemplate.births empty vector, with path_vec retained for present values',
            'AudioReferenceClock.root exact ProjectRootRoundEven only',
            'OwnedAudioBinding.resume absent Option',
            'AudioResume.local_boundary exact zero',
            'AudioLocalPhase.constant exact zero',
        ],
        'nondefault_data_and_validation_preserved': True,
        'legacy_audio_binding_modules_unchanged': True,
        'core_schema_version': 57,
        'store_schema_version': 86,
        'version_rationale': 'disposable internal codec experiment; no shipping version decision',
    })
    print(json.dumps({'source': str(SOURCE), 'target': str(TARGET),
                      'overlay': str(OVERLAY), 'changed_rust_paths': changed,
                      'rust_file_count': len(after), 'cargo_invoked': False}, indent=2))


if __name__ == '__main__':
    main()
