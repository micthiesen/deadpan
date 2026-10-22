# Mixed All plays/This play Hold duration through one fixed Retime

Mixed body authority now crosses exactly one fixed, unclocked Edit Retime
below every enclosing Repeat, with Sequence ancestry otherwise. Literal Default
choices remain shared; only explicit Plays needing ownership are isolated.
The existing SetScopedHoldDuration command, semantic context, ordinary reducer
and actual isolation proofs remain authoritative. No wire or schema change.

Selected batches, Retime-owned gaps, additional Retimes, Retimes above or
between Repeats, Partition, derived clocks and unsupported root/attached sounds
retain their guards. Ordinary mixed owned-gap and dormant paths stay available.

Evidence: [reports, exact sources, emitted media and owner packages](../../tools/media-qualification/evidence/2026-10-22-mixed-fixed-retime-hold-duration).
Host: Mac17,7, M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1, locked dependencies.

## Old-code control

Base `310c1b7b1c433bec7d87f60fc389a35694d4edb7`. Both mixed directions pass
public Rename isolation, mapped raw SetHoldDuration, every actual occurrence's
picture identity and exact geometry, unchanged templates, JSON and inverse
checks before the sole final positive native-availability assertion fails.
Default/Play needs five nodes; Play/Default needs six. Each has one actual
isolation step and no marks. The valid runtime failure takes 0.015 seconds.

All 1,951 baseline Rust files are frozen. A Git-blob audit against the base
confirms only the new witness and its registration differ: all 1,949 other
base Rust files match. Executed app test SHA-256:
`54b41e2b230f5a2b3270f2c5721ce96b90d7002f6f6fa3e7bb19b2fea27eadd8`.
The silent 42f fixture selects Outer 2/3 and Inner 2/2. Hold growth 2f to 4f
changes exact visibility [45/2,24) to [45/2,51/2), sampled [22,24) to [22,25).
Default/Play changes three uses; Play/Default changes two.

An uncompiled draft's proof count was corrected to one, and checked i128-to-i64
picture conversion was fixed before this valid runtime witness. The original
draft, notes and compile failure remain identifiable in the evidence.

## Structure, audio and durable history

Core cases cover both directions under all three pitch policies, exact pools,
zero-allocation owned edits, dormant Default definitions at either level,
cropped intrinsic starts, sampled disappearance and atomic refusals. Sparse
ordinal 1 occupies physical position 2 after ordinals 40/41; an earlier owned
outer body lasts 7f. Its visible interval is [41,43), then [41,44). A live
representative checks geometry without replacing the literal authoring target.
The raw command rechecks visibility, dormancy and fixed project extent after
the resize, independently of native capture.

The nine-stage Preserve audio fixture enters middle/middle at frame 16, exactly
25,625.6 root mix samples, so input PointCeil is 25,626. Every one of 11,212
input cells and suppression values matches a literal 44.1 kHz phase oracle
for all nine actual processor instances, including finite exhaustion and
chunk partitions. Default/Play affects stage starts 4/16/28; Play/Default
affects 12/16/20. Complete unrelated RoundEven output intervals and the exterior
owner remain unchanged, and total output stays 40f. Fresh pools are 5/6 nodes,
one proof step, then zero for the owned setter.

The 108f Store fixture retains an independently owned inner sibling. Fresh
mixed pools are 4/6 nodes, one proof step, then zero for staged setters.
Accepted-media shortening preserves the admitted provider; extension beyond
coverage commits fallback and preparation. The original request and immutable
accepted-artifact controls remain distinct from the new scope. Saved calls
commit one Compound; stale/late failures, Undo/Redo and reopen preserve complete
content and durable records. Model inference is outside these synthetic
accepted-media cases.

## Verification and retained failures

All 48 focused tests pass in 53.798s, with 5,236 skipped. All 3,515 broader
tests pass in 176.747s, with four slow tests and 1,769 skipped. This includes
206 affected normal app checks: 205 app-binary cases and one headless case.
All 208 UI-feature checks pass in 15.372s: 207 app-binary cases and one
headless case, with 1,060 skipped. Changed-package strict checks, UI all-target
Clippy, formatting and whitespace checks pass.

The initial all-target checks caught a PlayOverrides membership API mismatch
and two references to a renamed private app helper. Both were corrected before
the first focused execution. Three independent source reviews covered general
correctness, authority/timing and test quality; the helper references were also
independently flagged. No review finding remains outstanding.

The broader run retains one captured-output marker in an existing keymap test;
the UI run retains one in an existing exact-mark transition test. Their exact
pre-run hashed binaries pass separately with captured EOF in 0.016102s and
0.011370s. Causes remain unknown; no production workaround was added.
All 153 broader and two UI executable hashes were checked before and after
their runs. All 1,956 frozen Rust source hashes remain unchanged.

## Native release and emitted files

Debug native replay passes 115 workflow checks plus Kestrel over 1,002 frames.
Both release runs pass 115 workflow checks plus Kestrel, over 1,049 visual
and 1,278 performance frames. They cover routed Enter prefill, both mixed
scope explanations, exact fresh pools, owned setters, recording, named Run,
explicitly reselected dot, complete history content, delayed continuation,
reopen and cleanup. Root inspected both final 960x640 captures, one per direction.

Release app SHA-256:
`0c3c28bf6ade991c3e338d25a825d72406c2e708ad3f9d7a826965a56482c9ed`.
Quiet state p99 is 1.622291ms over 332 samples; commit p99/max is 79.919625ms
over 11 samples; picture completion p99/max is 8.648666ms over 91 samples.
No measured operation failed or timed out. The initial/final viewport is
1280x820 and editing uses 960x640. Cache, power and thermal state are not fully
controlled, so this does not close general performance or packaging gates.

Each run retains 65 second-retry frames and five runs of consecutive retries
with distinct causes. Visual runs also retain the intermediate-capture allowance
warning. No retry or capture limit was raised; the harness reports no ignored
paint or repeated identical-retry failure.

Eight release export cases pass in 47.869s, covering both directions, unit
and affine stages, fresh growth 3f to 5f, and owned growth 3f to 4f followed
by shortening to 1f (unit) or 2f (affine). Fixed outputs remain 91f/73f.
The literal composed picture oracle checks all 656 pictures. Unit cases
compare nonzero 256-sample suffix probes at exact old/new origins for every
affected and unrelated stage plus the exterior, within 1e-6. Affine cases
combine full emitted-file checks with the independent input-phase suite.

All 1,050,652 presented audio samples are covered by 20 complete windows at
zero measured offset, without uncovered samples, flags, failed pictures/windows
or offset-unobservable windows. Neighbor comparisons retain 708 observable
and 588 unobservable results for repeated/frozen pictures.

The affine Default/Play growth file has a 12.007609dB minimum local-block SNR
at sample 108,480; that complete window measures 35.962360dB. It passes the
existing, unchanged 1.5dB block limit, with zero flagged low-SNR blocks.
These measurements remain explicit; acoustic quality and full mastering stay open.

Release CLI SHA-256:
`8662a466109f877d5ae819cf59d30691705b822bc0784d434a2fd884bee3c076`.
Three owner archives retain the two 91f unit-growth directions and the 73f
affine Default/Play growth case. Each archive's nine package files were safely
extracted and rehashed. Full project validation returns valid=true with
24/25/24 nodes, and doctor --project succeeds while retaining its partial and
unimplemented capability lists. Archive identities and all actual MP4s are retained.

Five source stages retain full manifests; byte-identical stages share an archive.
Private progress-artifact receipts and future-task research are excluded.
An independent evidence audit verified the initial 146 inventory entries, both
source archives, execution identities, all eight MP4s, captures and owner packages,
and cross-checked these claims against raw reports without findings. Root appended
the corrected audit receipt and rehashed all 147 final inventory entries.

DP-06 and DP-07 remain partial. No requirement or gate is newly complete.
Physical input, VoiceOver speech and listening are recorded in
[To verify (owner)](../REQUIREMENTS.md#to-verify-owner). Selected fixed-Retime
batches, wider selectors, processing clocks, Partition and sound lifecycles
remain open.
