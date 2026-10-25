# Ranged live Follow creep, 2026-10-25

Ordinary ranged live Follow creep is implemented and locally verified through
Core, Plan, App and focused CLI checks, release native replay, actual emitted
media and independent audit. The final inspector paint assertions also run in
an optimized dev build. This does not close DP-08, a delivery gate, or the full
development goal.

## Authored behavior

An ordinary selected Source, Hold, Sequence, Repeat or Retime with one fixed
Follow accepts Keep-center creep over a contained Edit range. The target keeps
resolving against each picture's exact Original time, before, during and after
the scale window. Both original live/fallback scale bases remain before In.
Explicit `from=` applies only at In; each branch then interpolates to its own
terminal scale and holds it after Out. A full-length Visual range retains its
explicit window. The authoring clock is the current owner's output duration;
later structural edits retain that clock through the existing framing mapping.

`FollowScaleTransition.range` is an optional closed `FollowScaleRange` with
normalized exact `start`, `end` and `from_factor`. Endpoints share the existing
one-million denominator bound. Validation checks both branches at their old,
range-start and terminal scales. Exact comparisons select boundaries; interiors
use the existing Q32 round-even scale contract. The window consumes one extra
framing record in typed and raw preflight admission. Absent windows retain the
existing whole-owner bytes and behavior. Core 58 / SQLite 87 remain current;
older readers refuse the additive field, with no migration claim.

Explicit target sampling, ranged step/off on a Follow, and replacement of an
existing animated path retain their existing refusals. Repeat play ranges also
remain refused; explicit whole-owner Play authoring remains available. No new
occurrence authority, time, provider, target or audio mutation is introduced.

Native commands retain the existing session/revision/selection/picture capture.
Recording and dot save only semantic Zoom Keep intent and resolve each new
invocation's owner, range and two scale bases. Camera preserves the complete
window through unchanged Enter, Escape, target changes and proportional scale
adjustment, validating the range-start endpoint as well as the terminal one.
Labels show the prefix and ramp scales and a percentage of framing duration.

## Current evidence

Base: `f31d3347c38566d882de8e3903d17c224c52c6e6`. Scratch:
`/tmp/deadpan-ranged-follow-2026-10-25`. The first snapshot pins thirty-six
changed Rust inputs and three workspace configuration files. The second differs
only in one CLI test's active-selection cursor setup. Each invocation retains
its actual input hashes, raw streams, exit and elapsed time.

The physical host is Mac17,7, Apple M5 Max, 128 GiB, eighteen logical CPUs,
macOS 26.5.2 build 25F84, `kern.hv_vmm_present=0`. Dependencies are locked and
the toolchain is Rust 1.97.1. Dev tests use optimization level 2 with assertions.

- All 1,584 Core tests pass, 0 skipped: 4.252 seconds execution, 501.824 seconds
  including compilation. The run pins 351 Core/configuration inputs, unchanged.
- The integrated run executes 1,830 selected tests: 1,829 pass, including two
  nextest leak warnings; one CLI Hold test fails. App has 1,322 ordinary passes
  and two warning passes; Plan has 488 passes; CLI framing has seventeen passes
  and that one failure. Fifty-seven tests are skipped by the stated selection
  or their existing ignore policy. This invocation remains failed.
- The failing new Hold preview case supplied an extending backward Visual
  range whose head was frame 120 but kept cursor 123. Production correctly
  refused it. The fixture now supplies cursor 120 for that active range. The
  exact Hold test and both warning cases pass a serial recheck: three passed,
  no leak warnings, 0.972 seconds execution, 164.713 seconds with compilation.
  This establishes latest per-case success, not one green integrated invocation.
- The first recheck never ran tests: Root mistyped the qualified FFmpeg prefix
  (`dav1b` instead of `dav1d`), and native build scripts refused the missing
  include directory. Exit 101 and its streams are retained separately.
- Strict changed-package formatting and Clippy, including all targets and the
  App UI-harness feature, pass before and after the fixture correction:
  61.686 and 22.863 seconds. The command reference writer intentionally exits
  101 after regeneration; its reviewed diff changes only creep help, and the
  ordinary reference/routing tests pass in the integrated run.

Three fresh read-only reviewers cover general correctness, exact numeric/raw
admission, and native/macro/verification behavior. They report no concrete code
defects. General review requested rendered Hold/Repeat/Retime window coverage;
Plan tests across both curves now cover all five owner kinds, Repeat gaps
and resets, fractional Retime target points, fallback, picture identity/context,
audio spans and duration. Follow-up review accepts those tests and identifies
one contradictory help sentence; it now explicitly describes fixed-center
creeps when saying the end pose holds.

## Native and emitted-file verification

The first release build passes in 280.283 seconds. Its actual zoom replay fails
one new assertion after 119 passing checks: complete ranged framing is restored
by Redo, but the harness incorrectly expects the old Visual selection too.
The existing revision-bound selection contract clears that temporary selection
on history navigation. The corrected check requires the complete framing,
absent expired selection and unchanged Original/Edit cursors. Production
history behavior is unchanged.

Root inspects the actual failed-run `zoom-121.png` and finds the combined
framing description elides the saved window in the narrow inspector. The
production layout now separates Follows, Scale before, Creep and Window rows.
The second release build passes in 267.574 seconds; its actual zoom replay
passes all 296 checks plus the Kestrel audit in 14.132 seconds. Root inspects
the saved-window capture and confirms all values fit. Release App SHA256 is
`cdc58560ee427753c2deb4dbbc61d33497157b57128a4e22ceff077431b279e8`.

Independent layout review identifies a paint-test gap: a global text search
could accept another copy of the window string. The final harness requires
exact unelided label/value text, complete clipping/occlusion checks, coordinates
inside the inspector, and alignment in the same row. Only this instrumentation
changes after the release replay. Its optimized-dev build passes in 23.452
seconds and native zoom passes all 296 checks plus Kestrel in 16.931 seconds.
Root inspects the final capture. Optimized-dev App SHA256 is
`ce42d15a1bde722f7944757205f99a3e3665eedd87edf11a09f3361eac21736a`.
The final snapshot pins thirty-seven changed Rust files plus three configuration
files; no later production Core/CLI or inspector behavior changes are claimed.
Final strict checking passes in 22.702 seconds, with all forty inputs stable.
Five focused final App tests pass in 0.036 seconds execution, 26.318 seconds
including compilation, with 1,330 skipped by selection or existing ignore:
ranged labels, ranged-start bounds, Camera preservation, generated command
reference and production routing.

The actual release export test passes in 5.282 seconds, including Render and
independent decoding. Its test executable and four CLI/helper executables are
hashed before and after, unchanged. The new fixture is authored through the
shared semantic CLI and exported after Undo/Redo and a verified portable copy.
It uses a measured 120-picture counter Original and manually authored target
samples/correction, not tracker output or evidence of tracking accuracy.

An independent read-only audit recomputes all 120 exact provider/pose checks
from the authoritative document and a separate integer Q32 oracle. Full
192,192 stereo sample frames of authored/limited f32 bits and 192,192 limiter f64 values
equal the pre-creep baseline. PCM is finite and nonzero; limiter gain is unity.
All eight selected authored/history SQL tables equal the original package:
seven revisions, four history entries and one transaction step. Head revision
is `20d751cd-b530-40eb-88b5-adf400c7b285`.

The emitted file passes all 120 fidelity checks. Neighbor comparisons are
228 observable and ten unobservable, rather than 120 independent visual-order
witnesses. The full audio window reports four signal and 396 silent blocks,
no low-SNR/level/sound-in-silence failures, and three uniquely observable zero-
offset witnesses starting at 0, 45,056 and 188,096. No sample shifting, trimming,
event alignment or tolerance change is used. Minimum signal-block SNR is
29.5346867 dB; maximum level difference is 0.0438176 dB. Encoder priming metadata
is retained under the existing timing contract.

Movie: 128,965 bytes, SHA256
`913636778317acaae5cce254ccc9effa12dce203d3bfc2decf436f586ad43b40`.
Verification report SHA256:
`7daeca712f06440881d1ba5ab07ae30982612b698f46a22ecfab5a0de791473f`.
Independent audit JSON SHA256:
`86be959d4fa54b3c97aa65da8eaa76ded6193f89b6e9011f9e3f5793fd5aa362`.
That audit used ephemeral Python stdin; no retained verifier script or standalone
invocation receipt is claimed. Two preliminary schema-inspection errors did
not alter product evidence; the completed audit and finite-bit check pass.

No detector quality, physical display, listening, second-Mac, full packaging or
overall latency claim follows. Repeat play ranges, other target controls and
the remaining normative product scope stay open. The implementation commit and
progress milestone150 are still pending.

## Retained evidence and publication audit

The immutable [inventory](../../tools/media-qualification/evidence/2026-10-25-ranged-follow-creep/files.json)
has SHA256 `ce1f352010fbb5b77af9afa4733b1c030ba7fa2ec0ffe82870d94601fb58c539`.
It maps 274 logical inputs to 138 deduplicated payload objects plus three
metadata files, 141 files total. Stored bytes are 5,140,421; raw and gzip-decoded
bytes are each 84,166,968, including all metadata. Each remains below 256 MiB.
Decoded size measures gzip expansion, not MP4/PNG raster expansion.

The collection retains all four source phases, all eighteen terminal receipt/
stdout/stderr triples, real media and original/portable project files, complete
canonical PCM and pose evidence, all three full native JSON reports and four
selected actual captures. Native replay project trees, duplicate HTML and other
PNG sequences are explicitly excluded. Executable bytes and mutable current
docs are not retained; actual executable hashes are guarded. Seven nonauthored
omissions cover three zero-byte runtime locks and two empty-WAL/disposable-SHM
pairs, 65,536 bytes total. No authored media or database content is omitted.

Plan SHA256 `a6bfe19b294428a1cc5dd548390966520dd443bdd029dec631fc5bee0ab6c5d2`;
collector SHA256 `a435a0ee061a871ce9ae107dfc29572bfd7e0c2450955a0ed01b121f31b1f118`.
The actual collector passes in 1.638 seconds with source inputs stable and full
tree/byte readback. Eleven dry-run guard tests cover path traversal, absolute
and bare-Rust paths, nonblocking symlink/FIFO refusal, approvals, missing/changed
inputs, duplicate aliases, mutation after capture and metadata-inclusive caps.
The 64-KiB cap injection is test-only; production remains 256 MiB.

A separate [read-only byte audit](../../tools/media-qualification/evidence/2026-10-25-ranged-follow-creep-audit.json)
passes in 0.474 seconds. It independently verifies every stored/decoded payload,
complete alias/source maps, current forty inputs, sixty live source/binary
guards, native outcome states and all caps. Result SHA256:
`42b9659650db0a379777faf98c407074fa0e29d1d0d7f8edd4d857a4dcd20786`.
Its actual verifier source, run receipt, collection receipts and guard proofs
are retained as nine standalone files, 49,348 bytes, outside the immutable
collection. This audit executes no collector or product commands.
