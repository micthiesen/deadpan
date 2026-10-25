# Native launch-trust evidence, 2026-10-25

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
