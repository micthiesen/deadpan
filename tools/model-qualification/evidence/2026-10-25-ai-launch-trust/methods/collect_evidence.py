from pathlib import Path
import hashlib
import json
import shutil

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-ai-runtime-trust-2026-10-25')
out = repo / 'tools/model-qualification/evidence/2026-10-25-ai-launch-trust'
out.mkdir(exist_ok=False)


def copy(source, relative):
    target = out / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)


for p in sorted(scratch.glob('ai-trust-*')):
    if p.is_file() and p.suffix in ('.json', '.stdout', '.stderr'):
        if p.suffix == '.json':
            assert 'exit_code' in json.loads(p.read_text()), p
        copy(p, Path('runs') / p.name)

for p in sorted(scratch.iterdir()):
    if p.is_file() and (p.suffix == '.py' or p.name in (
        'source-files.json', 'removed-files.json', 'setup.json',
        'canonical-fixture.json', 'fixture-motion-correction.json',
        'inventory-measurement.json', 'README.md',
    )):
        copy(p, Path('methods') / p.name)

for directory in sorted(scratch.iterdir()):
    if directory.is_dir() and (
        directory.name.startswith(('cases-', 'negative-', 'canonical-', 'schema-validation-'))
    ):
        for p in sorted(directory.iterdir()):
            if p.is_file() and p.suffix in ('.json', '.stdout', '.stderr'):
                copy(p, Path(directory.name) / p.name)

for name in ('bundle-local-first', 'bundle-adhoc-first', 'bundle-without-first'):
    resources = scratch / name / 'Deadpan.app/Contents/Resources'
    copy(resources / 'build-provenance.json', Path(name) / 'build-provenance.json')
    copy(resources / 'Notices/sbom.cdx.json', Path(name) / 'sbom.cdx.json')
    copy(scratch / name / 'Deadpan.app.SHA256SUMS', Path(name) / 'SHA256SUMS')
copy(scratch / 'bundle-local-first/Deadpan.app/Contents/Resources/ai-runtime-inventory.json',
     Path('ai-runtime-inventory.json'))

for run in ('ai-trust-bundle-verify-first', 'ai-trust-without-verify-first'):
    lines = (scratch / (run + '.stdout')).read_text().splitlines()
    kept = [line.removeprefix('verify: kept ') for line in lines if line.startswith('verify: kept ')]
    assert len(kept) == 1, run
    for p in sorted((Path(kept[0]) / 'work/Exports').iterdir()):
        if p.is_file():
            copy(p, Path(run) / p.name)
for p in sorted((scratch / 'canonical-finish-first with spaces/Exports').iterdir()):
    if p.is_file():
        copy(p, Path('accepted-export') / p.name)

sources = json.loads((scratch / 'source-files.json').read_text())
(out / 'code-inputs.json').write_text(json.dumps({
    'base_revision': '8ad64f85',
    'scope': 'Final complete CLI/xtask files and root Cargo inputs. Per-run source_before/source_after retain exact earlier input sets and hashes.',
    'files': {name: {'bytes': (repo / name).stat().st_size,
                     'sha256': hashlib.sha256((repo / name).read_bytes()).hexdigest()}
              for name in sources}
}, indent=2) + '\n')
(out / 'review.json').write_text(json.dumps({
    'scope': 'Independent general, security and lifecycle reviews of AI inventory, launch integration and frozen bundle construction.',
    'applied': [
        'Update stale privacy audit to the interruptible launch calls and cancellation/deadline arguments.',
        'Bound writer/UI-side runtime identity discovery to 64 KiB with nonblocking non-following regular-file reads; oversized/symlink tests fail against the previous read.',
        'Route every Mach-O magic variant into the strict parser; unsupported images cannot become ordinary inventory data.'
    ],
    'remaining_findings': [],
    'limits': [
        'No containment claim against concurrent same-user host replacement.',
        'Not a complete product gate, real-person quality corpus, physical hardware or latency qualification.'
    ]
}, indent=2) + '\n')
(out / 'README.md').write_text('''# Private AI launch-trust evidence, 2026-10-25

Primary Mac: Apple M5 Max, macOS 26.5.2, Rust 1.97.1. Some run timestamps are
October 26 UTC, still October 25 in America/Vancouver. See
docs/qualification/private-ai-launch-trust-2026-10-25.md.

The original formatter/shared-module and unsupported mkfifo test compilation
failures remain. The broad continuation ran 1,010 tests: 1,009 passed and the
old privacy-source assertion failed, with one nextest pipe-leak warning.
The corrected strict-lint/192-test continuation passes with two pipe-leak
warnings. Serial execution of thirteen relevant tests passes without warnings.
No failed original run is relabeled a fresh full-tree pass.

The local certificate and ad hoc bundles carry the same canonical compiled AI
inventory pin. The disabled-AI bundle explicitly carries no runtime/pin. Real
stale-host --no-build publication refuses before direct Python smoke execution.
General relocated bundle checks pass for the local and disabled-AI builds.

The negative suite records 150 operations, 61 expected refusals, zero timeouts,
both hosts' trust errors, real Check refusals and a tampered-worker inference
host-I/O refusal with unchanged authored state. Validly resealed UUID mutations
preserve ABI. Structural cases retain their unsealed shape and are never
credited as validly resealed substitutions. Restored hosts admit their runtime.

Both real model Check paths pass. Two newly invented interior-Hold generations
complete inference but fail production quality validation: Still entry motion
and Moderate unconditioned-exit lighting. Their failed outputs remain. The
established Oct 8 K9/E8 definition-edge fixture then reaches Ready and is
explicitly accepted. Its first export attempt lacked a destination directory;
the continuation exports with an empty HOME and IP networking denied, then
Undo restores the exact fallback document except revision identity. The first
full-history invocation used the unsupported --full flag; the correctly formed
project validate command subsequently passes. Neither correction regenerates
or substitutes a candidate. Corrected reproducer scripts are labeled by saved
earlier versions; per-command records retain the executed script hash.

The setup clones and hashes the already accepted 31-file, 36 GB model pack;
no model download, duplicate physical import or license bypass occurs. Models,
runtime executables/libraries, private keys, keychains and large mutation
backups are excluded. Public certificate fingerprints and file digests identify
the run and are not credentials. Small synthesized/accepted export movies and
their verification receipts are retained.

Generation times are observations under uncontrolled cache/load and occasional
concurrent build/check work. They establish neither latency targets nor general
motion, face, mouth or real-person quality. Clean/second-Mac and physical owner
checks retain their existing Section 29.1 scope.
''')
copy(Path(__file__), Path('methods/collect_evidence.py'))
files = sorted(p for p in out.rglob('*') if p.is_file())
assert not any(p.suffix in ('.dylib', '.bin', '.ckpt', '.p12', '.key', '.whl', '.backup') for p in files)
assert max(p.stat().st_size for p in files) < 10 * 1024 * 1024
inventory = {'schema': 'deadpan-ai-launch-trust-evidence-1', 'files': [
    {'path': p.relative_to(out).as_posix(), 'bytes': p.stat().st_size,
     'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in files]}
(out / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
print(json.dumps({'files': len(files), 'bytes': sum(p.stat().st_size for p in files),
                  'inventory_sha256': hashlib.sha256((out / 'inventory.json').read_bytes()).hexdigest()}))
