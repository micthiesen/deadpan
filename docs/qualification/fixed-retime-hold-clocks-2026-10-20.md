# Hold sound clocks inside fixed Retime windows, 2026-10-20

This work retains physical audio clocks when changing an ordinary Hold's
duration moves content inside an affine Retime window whose output duration
stays fixed. The Retime recipe and outer placements do not change. It advances
DP-07 without completing the requirement.

Base `93f2b4d6`, Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1 and the
qualified FFmpeg 8.0.3 prefix. No schema, primitive or native keybinding change.

## Reproduced phase reset

The minimized 30000/1001 fps timeline is a three-frame silent lead followed by
a four-frame FollowSpeed Retime selecting input `[0,4)`. Its child is a
two-frame silent Hold followed by a two-frame natural-rate Source, with 44.1 kHz
audio beginning at source sample 3100. There are no prior timing bindings.

Growing the Hold to three frames keeps the root at seven frames while moving
the visible Source entry from frame 5 to frame 6. `B(5) = 8008` and
`B(6) = roundEven(9609.6) = 9610`. Recomputing the Source clock at the new
entry adds `147/400` of a source sample. The old implementation fails at new
sample 9610: actual stereo `[-0.10523279,-0.10523279]`, expected
`[-0.09149733,-0.09149733]`. An independent literal resampling recipe verifies
the old first 256 samples before any edit. The failing executed test binary
has SHA-256 `f0f39567397dfe09a5fdc005ff4012989b9062ad685be1aca03046b64e0c8ed0`.

## Capture boundary

The duration-specific collector follows moved Sequence suffixes and compact
Repeat interiors. It stops ripple collection at the first fixed affine Retime,
while checking every higher ancestor for a processing clock boundary. Current
capture machinery projects the old physical entries through exact affine
windows; it does not assume one rounded root translation. Outside siblings and
outer Repeat plays receive no new movement steps. The complete old timing
layout remains available for attached-sound journal identity.

The ordinary root-ripple and dedicated clocked-Retime paths remain separate.
Preserve/Shift ancestors retain their previous behavior and are not newly
qualified here. Generated-provider fallback, Tail clamp, current support and
silent-owner filtering remain in their existing order.

## Core, PCM and history

All six focused core tests pass. Independent review identified a dormant
Repeat-default case that could capture unchanged owned branches. Validated
override counts now prove that the default has no live use without expanding
plays, including when that Repeat is above the fixed window. The regression
performs 258 alternating resizes while preserving the entire binding state and
each exact inverse. The durable store test also passes preview, commit, reopen,
Undo/Redo, invalid shrink and a proven late Compound refusal without writes.
Other core checks cover nested nonunity windows with exact rational origins,
cropped entries, initially unbound and partially bound owners, default and
owned Repeat interiors, unchanged exterior reanchors, no-ops and invalid
selected windows. A million-by-million nested Repeat retains the same node
count, at most one timing table and less than 50 KB of serialized bindings;
this is a compactness check rather than a latency measurement.

All four final canonical PCM tests pass. They check literal old 44.1 kHz
phases at 30000/1001 fps through grow, shrink, nonzero nested affine windows,
default/owned plays and RoomTone gaps. Every current sample and suppression
flag, unaffected exterior audio, exact inverse, JSON round-trip and alternate
read chunking is checked. These deterministic provider tests are separate from
real-media export qualification.

## Oracle failures retained

The first PCM matrix verifies the corrected independent entry phase, then
fails comparisons near cropped ends. The source resampler uses the current
structural crop before filtering. Comparing that finite crop with an old longer
crop changes the filter context; one expanded assertion fails before resizing.
The oracle audit retains exact first differences, finite radii and current crop
bounds. Final tests keep literal phase checks and use independently specified
current crop context for full-allocation comparisons, without alignment or
tolerance changes.

The next PCM round passes the minimized grow and shrink cases. Nested affine
and owned-Repeat cases fail only at a final slot beyond their retained physical
allocation. For the nested case, current local support `[0,2)` on the old
origin `15/4` and slope `3/4` gives historical root `[15/4,21/4)`, samples
`[6006,8408)`, length 2,402. The live allocation `[7207,9610)` has 2,403 slots;
sample 9609 maps to excluded old endpoint 8408 and must be suppressed zero.
For owned outer play 0, the old interval `[17618,19219)` has 1,601 samples,
while the live interval `[19219,20821)` has 1,602. Its final slot is likewise
exhausted. Outer play 1 has a longer historical allocation than its live
allocation and must not acquire a terminal zero. These origin-based endpoint
rules remain unchanged. The final tests explicitly check both exhausted zeros
and the unsuppressed last sample of outer play 1. Independent review confirmed
the rational allocations against `AudioBound::raw_domain`,
`DomainSignalTransfer` and `RootSignalTransfer`; production DSP was unchanged.

## Broad checks and emitted media

Formatting and strict Clippy pass. All 2,635 selected core, audio, store and
CLI macro/scope/completion tests pass in 109.976 seconds, with 724 skipped by
the retained filter and no warnings or leak diagnostics. The focused six core,
four PCM and one store checks also pass separately. The command scripts and
failed oracle rounds are retained.

Both real-media grow/shrink release exports pass. Each keeps the root at 27
frames and the ordinary Edit/FollowSpeed input window `[0,8)` at eight output
frames, while changing the Hold from three frames to five or one. Every
picture is checked against literal Original ordinals, freeze or Background:
54 pictures pass in total. Repeated freeze and Background pictures leave 18/14
index-neighbor comparisons unobservable in grow/shrink; their pixels and
expected provenance are still checked. The interior Source starts at old frame 20 and moves
to 22 or 18; the exterior Source stays at 24. Independent nonzero 256-sample
authored-bus comparisons at each literal start plus 1,554 samples retain old
PCM within `1e-6`, away from finite crop boundaries. These media fixtures use
the qualified Original's actual audio rate; the separate canonical tests
establish the 44.1 kHz phase arithmetic.

Both full-file emitted audio windows pass at measured offset zero with no
uncovered samples, low-SNR blocks, level or silence flags. Four audio blocks
are compared. Minimum SNR is 36.808 dB for grow and 48.426 dB for shrink;
maximum block level difference is 0.0101 dB. The encoder declares 2,048 samples
of priming through the approved timing metadata. Verification uses no event
alignment, packet dropping or relaxed offset bound. The final release test
passes in 5.43 seconds after its build.

The first release fixture fails before rendering or resizing. Its convenience
splits produced Partition wrappers; copying only their root recipes left child
references to deleted Source nodes. Insert correctly refused that incomplete
tree. The fixture now clones complete fragments with fresh descendant IDs and
clones the exterior suffix separately. Source and Partition fields, the four
initially unbound raw Source owners and every verification assertion remain
unchanged. The failed request, log, source identities and executable hashes are
retained. Production and the 2,635-test source set did not change; the fixture
receives a separate formatting/strict-lint check and release rerun.
That formatting and strict-lint check passes in 9.89 seconds; the successful
release rerun above uses the corrected complete fragments. Independent review
also corrected its earlier Source-root assumption and found no further issue.

## Retained record and limits

[Evidence](../../tools/media-qualification/evidence/2026-10-20-fixed-retime-hold-clocks)
retains scripts, source and executable identities, failed and final logs, exact
oracle audits, the failed Insert request, independent review, both MP4s and
complete verification reports. `inventory.json` hashes every retained file.
No new UI, keybinding, schema, media decoder or export implementation is added.
Preserve/Shift processing ancestors, general speed-stage updates and the
remaining Section 8 work stay open under DP-07. This qualification does not
establish physical input, acoustic judgment or new latency bounds.
