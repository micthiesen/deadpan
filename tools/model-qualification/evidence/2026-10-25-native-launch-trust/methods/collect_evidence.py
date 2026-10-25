from pathlib import Path
import hashlib
import json
import shutil

root = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-native-trust-2026-10-25')
out = root / 'tools/model-qualification/evidence/2026-10-25-native-launch-trust'
out.mkdir(exist_ok=False)


def copy(source, relative):
    target = out / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)


for p in scratch.glob('native-trust-*'):
    if p.is_file() and p.suffix in ('.json', '.stdout', '.stderr'):
        if p.suffix == '.json':
            value = json.loads(p.read_text())
            assert 'exit_code' in value, p
        copy(p, Path('runs') / p.name)
for name in ('run.py', 'qualify_bundle.py', 'qualify_bundle.v1.py', 'qualify_bundle.v2.py',
             'qualify_bundle.v3.py', 'qualify_bundle.v4.py', 'source-files.json',
             'local-signature-format.json', 'local-signature-format-second.json',
             'local-keychain-selection.json'):
    copy(scratch / name, Path('methods') / name)
for directory in sorted(scratch.glob('cases-* with spaces')):
    for p in directory.iterdir():
        if p.is_file() and p.suffix in ('.json', '.stdout', '.stderr'):
            copy(p, Path(directory.name) / p.name)
for name in ('bundle-local-second', 'bundle-adhoc-first'):
    resources = scratch / name / 'Deadpan.app/Contents/Resources'
    for source, target in [('build-provenance.json', 'build-provenance.json'),
                           ('Notices/sbom.cdx.json', 'sbom.cdx.json')]:
        copy(resources / source, Path(name) / target)
    copy(scratch / name / 'Deadpan.app.SHA256SUMS', Path(name) / 'SHA256SUMS')
verify_text = (scratch / 'native-trust-bundle-verify-first.stdout').read_text()
verify = Path(verify_text.split('verify: kept ')[-1].strip())
for p in (verify / 'work/Exports').iterdir():
    if p.is_file():
        copy(p, Path('bundle-verification') / p.name)
sources = json.loads((scratch / 'source-files.json').read_text())
(out / 'code-inputs.json').write_text(json.dumps({
    'base_revision': '8b418a82',
    'scope': 'Final CLI/xtask source inputs, including fixtures; exact source and binary identities are retained per run.',
    'files': {name: {'bytes': (root / name).stat().st_size,
                     'sha256': hashlib.sha256((root / name).read_bytes()).hexdigest()}
              for name in sources}
}, indent=2) + '\n')
(out / 'review.json').write_text(json.dumps({
    'scope': 'Current native helper admission, bundle construction and call sites; independent general/security/lifecycle reviews.',
    'applied': [
        'One shared admission deadline and cooperative cancellation through reads/hashes/system signature commands.',
        'Checked process-group cleanup, leader fallback and sole reaping across every signature-command outcome.',
        'Runtime admission errors remain host failures rather than candidate-output failures.',
        'Cheap fallible packaged identity prevents downloader/AI discovery and retained launch values from downgrading after marker loss.',
        'Structural XML marker extraction rejects the reviewed comment-only bypass.'
    ],
    'open': [
        'Private bundled AI Python/worker/imported-code tree still lacks a compiled inventory binding and app-seal admission before Python exec; DP-22.3 remains partial.'
    ],
    'limitations': [
        'No containment claim against a concurrent same-user writer who can replace the host itself.',
        'No fresh full-product gate or clean/second-Mac claim.'
    ]
}, indent=2) + '\n')
(out / 'README.md').write_text('''# Native launch-trust evidence, 2026-10-25

Primary Mac: Apple M5 Max, macOS 26.5.2, Rust 1.97.1.
See docs/qualification/native-launch-trust-2026-10-25.md.

The original broad check retains 961 passes/one privacy-allowlist failure.
The revised lifecycle checks pass 203 selected tests. A later XML-parser lint
failure remains, followed by the passing 147-test final continuation.
Each invocation retains source/binary hashes, selected environment, output and
exit status. The original broad invocation is not relabeled a full-tree pass.

The certificate-name build was stopped after stalled signing. The successful
local bundle uses the exact public certificate fingerprint; the ad hoc
no-build bundle validates the same compiled image identities independently.
Four failed driver attempts remain: missing shot analysis, isolated-home
signing identity lookup, explicit-keychain lookup under that home, then a
signing identity timeout. The final driver refreshes the existing owner
identity through its canonical dotfiles signer before fixture signing. Signing
uses the owner's HOME; every tested host uses the isolated HOME. All command
environments and expected refusals are recorded. A timeout never counts as a
successful refusal. Version 1 was reconstructed and byte-hash verified against
its retained invocation; later driver versions are exact saved copies.

Final local tests: 169 operations, 34 expected refusals, zero timed-out
operations. Three workers and all six FFmpeg libraries were substituted and
validly re-signed. Library substitutions change only LC_UUID, preserving ABI.
Both hosts refuse missing/comment-only markers, escaped hosts and signer
mismatch. Actual transcription, tracking and face/saliency proposals execute;
the complete authored document remains byte-identical. These synthesized-input
workflow checks do not establish general recognition/tracking accuracy.

Runtime binaries, model weights, private keys, keychain data and installed
models are excluded. Public certificate fingerprints and image digests are
identities, not secrets. The source project/media is the existing synthetic
word-refinement fixture. No private AI generation is rerun here.

The private AI runtime's prelaunch compiled-inventory gap remains explicitly
open. These native-worker checks do not close DP-22 or any delivery gate.
''')
copy(Path(__file__), Path('methods/collect_evidence.py'))
files = sorted(p for p in out.rglob('*') if p.is_file())
assert not any(p.suffix in ('.dylib', '.bin', '.ckpt', '.p12', '.key', '.whl') for p in files)
inventory = {'schema': 'deadpan-native-launch-trust-evidence-1', 'files': [
    {'path': p.relative_to(out).as_posix(), 'bytes': p.stat().st_size,
     'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in files]}
(out / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
print(json.dumps({'files': len(files), 'bytes': sum(p.stat().st_size for p in files),
                  'inventory_sha256': hashlib.sha256((out / 'inventory.json').read_bytes()).hexdigest()}))
