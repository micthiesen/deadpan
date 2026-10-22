# Fixed Retime output-owned sounds through Hold edits, 2026-10-21

An independent sound authored on a fixed Retime enters after that stage. This
increment permits an ordinary interior `SetHoldDuration` when that event's
owner/output contract stays unchanged. The changed Original/RoomTone operand
is not a dependency of that independent output voice. The supported family
retains the exact owner, output extent, input mapping, pitch, purpose,
treatments, placement and explicit ancestry. It does not admit an omitted
outer processor or reinterpret an input-owned sound as output-owned.

The existing full-subtree sound-clock proof remains strict. A separate bounded
owner-aware proof can stop at the proven independent output owner and maps
only that owner plus required placement ancestry. Six consumers use the same
distinction: reducer transport, durable binding validation, planned sound
reads and saved-slice clock/rename paths, backed by the shared frozen-layout
proof. Complete scope proofs remain shared; owner-specific output proofs use
separate cache identity so they cannot admit a sibling's changed input clock.
This adds no schema tag, DSP, encoder or UI behavior.

The reducer selects this narrower dependency only when the resized Hold is
inside the affected processing owner. Moving an unchanged stage by resizing
an outside lead Hold retains its existing grouped capture. An interior edit
preserves a fresh unchanged placement without inventing a sound journal; an
existing journal retains historical phase and frozen tables, narrowing scope
only with a checked placement map. Current source admission, suppression and
owner/ancestor treatments still apply after retained-source preparation.

## Valid failing baseline

The old baseline is source revision `072f0d82`. Six positive witnesses first
prove valid factories, explicit measured-clock source spans and +37 mix-sample
offset, unchanged-edit admission, successful no-event grow/shrink controls,
fixed output contracts, exact inverse/JSON and unchanged input. The old full
proof genuinely rejects the altered child subtree. Only then do the witnesses
require semantic application with the independent output attachment.

Old production fails all six positives while its changed descendant-owned
sound refusal control passes: seven tests, six failures, one pass, 567 skipped,
exit 100. Executed binary SHA-256:
`0ce77dde3a67df13925e3deefaff8b277c75872ce356430ef09571c236f089bf`.
The exact old witness source directory, baseline production hashes and complete
runtime output are retained. An earlier witness compilation exited 101 because
`ExactRatio` does not implement ordinary ordering operators; the test was
corrected to use `compare_integer` before this actual runtime baseline.

## Review and initial corrections

Fresh general, proof/authority/work-bound and verification reviewers found no
production issues. Scope follow-up found no issue with Hold-containing
processor eligibility or outside-move capture preservation. Verification
strengthening revokes only the event source after warming its cache while the
Original remains admitted, requiring the event's exact `SourceUnavailable`
failure. Review outcomes and dispositions are retained.

The first implemented focused run passed ten of twelve tests and failed two.
Its executed binary SHA-256 is
`be72697af5825287937d4098dca2f78a7f28d5609f5e65f6470898d7e4de40cc`.
One failure exposed output-local journal narrowing during an outside lead-Hold
move; limiting the narrower routing to the processor containing the resized
Hold preserves the original grouped journal. A new outside-move case keeps
that distinction explicit. The work-sharing fixture also attempted 65 live
events, exceeding the existing 64-event limit. It now uses 63 child-owned plus
one Sequence-owned event. Its 63 × 2,000-node traversal still exceeds a
100,000-work budget if complete proofs are incorrectly recomputed per owner.
The proposed review change to cross the 65-event boundary was dismissed because
that existing admission cap is intentional, not the behavior under test.

The first real store selection failed both tests during fixture setup, before
the target Hold edit: `OriginalMedia(InvalidRecord("source path must be
absolute, bounded, and have no parent traversal"))`. The event fixture path
contained the literal `../../` from its manifest-relative construction. The
store fixture now canonicalizes that real file before public retention; the
CLI fixture applies the same normalization before retention and oracle reads.
Canonicalization failures propagate normally. Source-path validation remains
unchanged; no admission bypass was introduced. The initial two-test failure
log, executed binary identity and pre-correction sources are retained.

The next store run passed the late child-owned atomic refusal but failed the
positive test's demand for a nonempty stored reanchor vector: one pass, one
failure, 56 skipped in 0.264 seconds. The actual lattice and inverse checks had
already passed. That expectation was incorrect: the established compact
representation omits an allocation-entry reanchor when it cannot change any
resolved clock or phase. This unsplit local0 entry has no Repeat or nontrivial
resume, so retaining a redundant vector entry is not a semantic requirement.
The test now proves the live body-relative suffix moved to6/4 while its retained
Preserve-input lattice stays at origin5, grid origin1 and exact first sample6407.
Its effective resolved entry/phase must match the baseline. This strengthens
the observable clock requirement while allowing the existing exact compaction;
the production implementation is unchanged. The failed source is pinned before
this correction, together with the failed log and binary identity.

The first PCM selection failed all six during fixture attachment because the
separate event asset lacked the required qualification field. The fixture now
records the actual measured WAV's content SHA and an admitted catalog
qualification identity; the provider independently verifies the real WAV bytes
and index before exposing event PCM. Formatting the digest with `LowerHex`
then failed compilation (exit101), so the fixture uses explicit two-digit hex
per byte. These were test setup/compiler corrections, not relaxed source
admission.

The next PCM selection passed three and failed three in 2.691 seconds. Its
journaled samples already matched; the failing assumption was an all-false
suppression mask at the fractional stage/suffix seam. Historically frame3..7
allocates [4805,11211), 6,406 samples. The real lead ripple moves the stage to
frame4..8, allocating [6406,12813), 6,407 samples. Retaining the captured 6,406
physical samples exhausts that voice at12812, so [12812,12813) is suppressed.
This is finite retained-output support, not a Silence RoomTone policy. The
independent event [6443,6552) is unaffected. The corrected test derives this
complete mask from the literal frame geometry and applies it before/after the
interior Hold edit, to cold reads and current mixed/nonlinear component
comparisons. No production behavior changed. The failing logs, source IDs,
available failed-source snapshots and fractional-seam binary are retained.

Final formatting and strict changed-package Clippy pass. The focused core
selection passes all thirteen tests in 0.144 seconds, all six PCM cases pass
in 3.107 seconds, and both real-package store cases pass in 0.575 seconds.
The complete core/plan selection passes all 1,726 tests in 3.742 seconds.
The complete audio/store selection passes all 1,404 tests in 108.409 seconds,
with two slow cases and no skips.
Executed focused test SHA-256 identities:

- final-core: `94863968eb208f26366220365e2a8f9e78f96c33e1e2b9b128c20effa79bd3f0`.
- final-pcm: `7b485315549c4258a58c7548b5e0cee4e0e3d597184bf06025cc30b4a5dd03f8`.
- final-store: `de8b04a0d277e87ec5a54b09f700f94904dbb04c49ab94258621d37e7c0956a5`.

The final test-only nonlinear refusal witness uses valid requests of at most
256 samples. Root caught an oversized request before execution and replaced
it with the bounded walk plus a cold single-sample refusal at 8236. No
production guard or meaningful-support policy was weakened.

## Exact phase and current audio policy

The 30000/1001 project uses exactly 8008/5 mix samples per frame. Its body is
Source3 + RoomTone Hold2 + Source4, selected through Preserve input [1,8) to a
fixed four-frame output. An ordinary Hold edit changes the body to ten frames
on growth to3, or eight frames on shrink to1. The stage's output and root
picture duration stay fixed.

The event is independently sourced at 44.1 kHz with offset +37 in the 48 kHz
mix clock. A 100-native-sample event [3100,3200) has root support [4842,4951)
when its owner starts at frame3. A genuine public lead-Hold 3→4 ripple moves
that support to [6443,6552) and creates the retained root-ripple journal. Its
first native phase remains `2480147/800`, advancing `147/160` native samples
per mix sample. Recomputing from the new ideal start would instead produce
`1239853/400`, which is deliberately the wrong control.

PCM tests separate event-only signal from the changing Original/RoomTone
component without changing either Hold to Silence or muting an ancestor.
They compare retained phase/support, boundary neighbors and cold reads, then
check the current nonzero mixed bus against its current physical component
plus the independent event. Current nonlinear owner treatments are checked in
literal supported windows against the current shared sum. They do not demand
that the whole master remain unchanged when its physical input changed. Descendant-owned events
keep their changed-input refusal.

All six final PCM tests pass. The retained nonlinear limitation is explicit:
both pre-edit and post-edit full-bus walks fail at chunk 8192 because sample 8236
is nonzero while its owner query declares [8236,8237) inactive. The exact
Unsupported error is checked again with a cold one-sample request. Numerical
shared-owner saturation and exact masks pass for all other literal samples,
including regular, irregular and cold reads. This is not complete nonlinear
master qualification; repairing that pre-existing fractional owner-support
hole remains owed under the full development goal. The separate exhausted
physical point [12812,12813) contains zero PCM. No complete nonlinear master
or nonlinear encoded export is claimed here.

## Durable package verification

Two tests register real measured Original and 44.1-kHz event media, then use
public commands for stage creation, attachment, root ripple and staged
Hold2→3→1 edits. Fresh and journaled cases require preview/commit parity,
read-only previews, preserved event recipe and journal, retained physical
input lattice/reanchor, exact inverse, durable Undo/Redo, reopen and
`validate_full`. A separate two-step Compound prefix (grow, add a child-owned
event) must preview successfully before its third shrink refuses. Failed
preview/commit must preserve snapshot, register bank, revisions, transaction
steps, history counts and absence of the uncommitted leaf revision.

Both final store tests pass against real managed media and SQLite packages.
The success case verifies fresh and journaled growth/shrink, semantic physical
clocks, preview/commit parity, inverse, Undo/Redo, reopen and full validation.
The late-failure case verifies both preview and commit remain atomic.

## Complete encoded output

The public CLI fixture strengthens emitted-file observability with a longer
event [3100,8005), still at 44.1 kHz and offset +37 mix samples. It retains a
genuine preceding root-ripple journal. Literal event support [6443,11782)
contains 5,339 mix samples and uses the same retained phase `2480147/800` and
step `147/160`. A direct qualified-source resampler recipe supplies the
independent event oracle; it does not read event allocation or phase from the
edited plan. The wrong newly rounded phase must differ by more than 0.001.
Every event sample and three neighbors on both boundaries are compared before
growth, after growth and after shrink with maximum error below 0.000002.

The measured Original's chosen audio passage is actually checked to be zero
over the whole no-event bus, while its moving pictures and RoomTone recipes
remain real. No mute treatment, Silence replacement or allowance creates
event isolation. This supports a complete emitted event comparison; separate
PCM tests establish behavior with nonzero current physical inputs.

The final expected picture sequence is four background lead frames, then
Original13 / background / Original13 / Original15 through the fixed stage,
then Original12 /13 /14. It is exactly eleven pictures and 17,618 presented
samples. Canonical Render plus `verify-export --every 1` must compare all
pictures and two contiguous complete audio halves [0,8809), [8809,17618), both
containing independent event signal. Every window must report `verified_zero`,
measured offset0, no flags and no low-SNR blocks; the summary must have zero
nonzero offsets. Both emitted edit metadata and the decoded endpoint must
equal 17,618 samples. No offset tolerance, event alignment, packet dropping or
verification bypass is authorized by this fixture.

The corrected release test passes in 7.055 seconds. All eleven pictures and
all 17,618 presented samples pass in the two contiguous signal windows at
measured offset zero, with no flags or low-SNR blocks. Thirteen audio blocks
are compared; minimum block SNR is 30.970 dB and minimum luma PSNR is 55.579 dB.
The independent event oracle has exactly zero maximum error before growth,
after growth and after shrink. The deliberately wrong phase differs by
1.1442310810089111. Release CLI SHA-256: `fe41efd48eba3d5a9b2f2e0de1ffd64f5e090a5942b72d8e4b1ae20c33d21826`; test binary: `61cae3db0fa5e274c7fb71d1cca40b2d174fba08b6cb63cbb1ab95e60f335993`. Emitted MP4: `2f3593aab37f632a65b1748d2056c6393d42e9f486e499b275c7c1148f92b5e2`, 9504 bytes.

The first strict-Reject oracle compilation also exited101 because
ExactRatio.floor returns an infallible i128; removing its erroneous trailing
question mark preserved every endpoint and containment assertion. The failed
compiler output and source are retained.

The first actual release test failed before Render, in 5.659 seconds (zero of
one passed, 46 skipped). Its fixture construction and event PCM oracle had
completed, but the generic preview picture helper assumed `hold_adjacent`.
This fixture deliberately uses raw `SourceVideoMapping::FitBeat`, whose policy
is `Reject`. Its first executed release CLI SHA-256 was
`fe41efd48eba3d5a9b2f2e0de1ffd64f5e090a5942b72d8e4b1ae20c33d21826`;
test binary SHA-256 was
`e0dd28d606aeaf25e531a41b6612db2b5fac7acc11d7383d995d0cb3893ea09b`.
The old generic adjacent-hold assertion remains unchanged. The corrected
fixture-specific oracle requires `reject`, the exact1/30000 source clock and
every source sample strictly inside its half-open selection, then derives its
ordinal without clamping. All eleven literal picture expectations, every
encoded provenance comparison and complete audio verification remain required.
First-run source snapshots, identities and failure output are retained.

Measured media identities:

- `cfr-bframes.mp4`: `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
- `pcm-mono-44100.wav`: `b95b3debd2c83cf2590fddfaa98276c23c3b383b720fd96ac52c2c00c668318b`.

## Scope and retained evidence

Native Hold-duration editing under a Retime ancestor remains unavailable at its
current UI timing gate. This slice does not claim that native path, a GUI
replay or new controls. It does not qualify arbitrary nested processors,
changed stage rate/window/output extent, arbitrary sound ownership transport,
physical device output or listening. No DP requirement or release gate becomes
complete from this slice.

Retained evidence is in
`tools/media-qualification/evidence/2026-10-21-fixed-retime-output-sound`.
Its 99 files total 445,981 bytes, plus the SHA-256 inventory. Every entry was rehashed after collection.
Reports, exact old witnesses, final verified sources, commands, reviews,
phase oracles and media/executable identities are retained; raw media,
projects and executables are omitted. Checks ran on Apple M5 Max, Mac17,7,
128 GiB, macOS26.5.2 (25F84), Rust/Cargo1.97.1. Cargo.lock remains
`1e39d18ae4de6c7b898edad39eb7104235fb3ffe3af84521ca94df2b6c25b71b`.
