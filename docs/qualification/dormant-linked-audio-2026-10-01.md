# Dormant linked audio, 2026-10-01

An Original slice entirely before or after measured audio now retains that audio
stream, its complete affine mapping and linked intent. An explicit empty selected
interval produces silence. Extending the interval can reveal audio without
reconstructing it from asset metadata or changing its sample phase.
The implementation base is `11f3404f2bf11b623f1bbdf568cd449f2ddd5a16`.
The [retained evidence](../../tools/media-qualification/evidence/2026-10-01-dormant-linked-audio/README.md)
includes failed attempts, command records and source manifests.

See [Original moments](../SOURCE_MOMENTS.md) and
[source audio timing](../SOURCE_AUDIO_MAPPING.md). This is a prerequisite for
Trim. It adds no native Trim mode or boundary-edit command and completes no
requirement or release gate.

## Representation and lookup

Equal `SelectedPlacement` endpoints retain a Source with no audible or filter
support. The full mapping still has positive duration, bounded endpoints and a
separate sample offset. Missing `Source.audio` remains intentionally absent.
Placed sound events continue to require positive support. Dormant Source audio
has no resolvable source-coordinate boundary.

Moment derivation clamps an empty selection to the nearest measured audio
boundary. Audio before the picture uses its mapped end; audio after the picture
uses its mapped start. Touching intervals remain silent, including when audio
falls inside the integer beat's rounded picture tail.

Live and frozen audio plans return silence without constructing an empty audible
range or requesting source samples. Captured timing bindings remain present;
selection growth uses the retained phase mapping. Core schema 36, database 45
and audio context schema 5 record the new meaning. Supported older document,
command, patch and audio-context readers keep their positive-support grammar.
Unused development databases 39 through 44 reject before writer acquisition or
migration backup. Existing database 1 through 38 adapters remain.

## Regression evidence

The new PCM tests fail against the prior code at empty-selection validation.
After the change they verify bitwise positive-zero PCM and zero provider reads
for empty support at full-map start, interior and end, with ordinary and captured
clocks. Raw and edge-faded stage reads are covered. The absent-audio control
cannot create audio through a mapping edit.

At 30000/1001 fps with a three-sample offset, expanding the selection to local
frames `[1/5,3/5)` reveals output samples `[323,964)`. An independent oracle uses
source filter support `[650,1290)`, source position `3244/5` at output sample 323
and unit sample step. Two different read partitions match the expected synthetic
fixture PCM. Undo restores no-read silence and the full mapping remains unchanged.

The storage regression uses a retained H.264/AAC MP4 whose audio occupies an
interior portion of its four-second video. Before the change, it fails because a
silent moment discards `Source.audio`. The fixture generator, input hashes,
FFmpeg build details and measured stream/frame records are retained beside it.
The qualified audio interval is `[43076,88217)` at 44100 Hz. Raw AAC decoder
blocks include padded samples beyond that endpoint; those samples do not extend
the qualified stream.

## Corrected test setup

- The first storage command used a short name with `--exact` and selected zero
  tests. The corrected fully qualified name runs the intended regression.
- The first synthetic fixture used a QuickTime MOV container outside the native
  admission policy. A normally muxed H.264/AAC MP4 replaced it.
- The first PCM oracle requested blocks outside its authored output interval.
  It now fills the surrounding output with zeros and renders only `[323,964)`.
- The initial storage endpoint assertion used raw AAC padding rather than the
  qualified terminal duration. It now checks the retained measured endpoint.
- A new plan test used a nonexistent pitch-policy name. It was corrected to
  the existing `FollowSpeed` policy before that test compiled.
- The first affected-crate run reached an obsolete assertion that context
  version 5 must be rejected. The unknown-version case now checks version 6;
  version 5 is the current format introduced by this change.
- The next run reached an old frozen-layout test that rejected all empty Source
  placements. It now accepts valid dormant support and still rejects reversed
  intervals and empty points outside signed frame bounds.

## Automated verification

Rust 1.97.1 and locked dependencies verify **2,169 distinct unit/integration
tests** across core, plan, media, audio, store and CLI, with none failed or ignored
in the completed coverage. The corrected broad run verifies 698 audio/CLI tests
before reaching the obsolete frozen-layout assertion. The final remaining-crate
run verifies all 1,471 core/plan/media/store tests plus both compile-fail
documentation tests. Its duration is 634.60 seconds. Earlier failed attempts
remain in the evidence and are excluded from the completed-test count.

This includes 49 legacy mapping/layout cases, three independent PCM regressions,
exact anchor and frozen-context tests, measured moment derivation and the new
storage regression. Both pre-audio and post-audio slices retain their linked
context through preview, commit, close/reopen, Undo and Redo without changing
asset inventory or receipt count. Independent product and test-correction reviews
have no remaining findings.

Workspace formatting and strict workspace/all-target Clippy with the native UI
harness enabled pass. Clippy completed in 736.49 seconds. The final checked
source manifest is
`16f529a6f784c365dbf998754b5d702ef1e8ab1216e0dd8476bc1d7b86e94d1f`.
Checks ran on an Apple M5 Max with 128 GiB RAM, macOS 26.5.2 (25F84).
The final process check at 11:40 UTC found no running Deadpan app.

## Scope

No native app was opened for this increment. There are no GUI, physical-input,
performance, model or release claims. Extending before a Source's physical local
zero still needs exact retained-clock translation. Framing and audio-treatment
owner clocks also need preservation before the full Trim workflow can change
those domains safely.
