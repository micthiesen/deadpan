# Native phoneme proposals

This library assembles the frozen alpha0.6 phoneme graph in memory, computes
16 kHz mono log-Mel features and runs ONNX Runtime 1.31.0's CPU provider. It
does not align text, interpret a project or accept a proposal. The analysis
crate owns dictionary, CTC, confidence and context-guard policy.

The original 57,562,900-byte checkpoint is verified once as an owned bounded
buffer. `assembly.rs` checks its SHA256, decompresses a pinned zero-weight
template with a strict length bound, and copies only compiled tensor ranges.
Every tensor has checked shape, bounds, non-overlap and SHA256; each destination
must be zero. The completed graph has its own compiled SHA256. It is never
persisted. No ZIP reader, pickle loader, model code, Python interpreter,
runtime recipe, model download or custom operation is used.

`assets/assembly.json` records the reviewed fixed range derivation. It is an
audit source, never runtime input. `src/patches.rs` is the compiled range list.
The template contains graph structure and zero raw tensor buffers. Hann and
Mel assets are mathematical preprocessing constants, not learned parameters.
The original global statistics remain in the verified model pack and are
admitted by exact original JSON hash and finite 80-element validation.

Build with an explicit `DEADPAN_ORT_PREFIX` containing the qualified headers
and `lib/libonnxruntime.dylib`. The directly linked dependency is visible to
the bundle dependency audit. No `dlopen`, framework provider, accelerator,
network runtime or fallback is selected. Sessions use four intra-op threads,
one inter-op thread and sequential execution; telemetry, profiling and worker
spinning are disabled. A joined native watcher terminates the active Run when
the shared cancellation flag or absolute deadline fires. Native construction
and individual kernels have cooperative deadlines; the outer worker owns the
hard process deadline. Other operating systems return `Unsupported`.

The port does not establish phonetic accuracy. The previous evaluation failed;
the guarded results remain manually accepted approximate suggestions. Native
feature and emission parity must be measured before claiming qualification.

The `deadpan-align` binary implements `deadpan_jobs::refinement` protocol 1,
one inference attempt per supervised process. It accepts verified checkpoint
and original-statistics paths plus a contained raw PCM artifact. It receives
no transcript or dictionary. The only successful output is a hashed raw
little-endian float32 `[T,41]` tensor, plus exact input/output counts and
compiled runtime provenance. The host must snapshot and validate that output
before applying its independent CTC and guard policy. Reads are nonblocking
until regular-file checks, bounded and hashed in controlled chunks; workspace
paths use retained descriptors and reject symbolic links. Matching request,
attempt and token are required for cancellation. EOF or malformed control
input cancels the attempt, and every phase uses one absolute deadline.

Development parity tests use `DEADPAN_ALIGN_REFERENCE_ROOT` pointing to the
retained `native-reference` directory, and `DEADPAN_ALIGN_CHECKPOINT` pointing
to the original checkpoint. Without those explicit opt-ins, no private fixture
or model is opened and no test downloads anything.

## Third-party attribution

The model architecture and original data come from Huang Ruizhe's `audio`
repository, commit `a4749fe14f17b19c071fe5b20005575ec0509ca3`, with its retained
BSD-2-Clause license (Facebook Inc., Soumith Chintala) in
`assets/AUTHOR-LICENSE`. The executable contains only the
zero-weight graph and mathematical Hann/Mel constants; original checkpoint
and statistics licensing belongs to the separately installed data pack.

ONNX Runtime is Microsoft Corporation's MIT-licensed runtime, pinned to
1.31.0. Headers are from official commit
`402979ac1bef89dcd55add41eaa7277c407599eb`. Release packaging must retain its
license and third-party notices with the qualified library. RustFFT 6.4.1 is
MIT OR Apache-2.0; flate2 is MIT OR Apache-2.0. Cargo.lock and the application
SBOM record their exact dependency closure.
