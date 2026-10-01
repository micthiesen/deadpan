# Exact picture selection context, 2026-10-01

New Original moments retain the complete measured video span and its affine
mapping, with an independent exact visible selection. This provides source
context for later Trim work while preserving the selected pictures, including
the held final frame in a rounded timeline tail. Beat duration, linked audio
placement and source cadence are unchanged. The implementation base is
`dbd89f1897f7fd92427849e56bb1ef2b1f59a273`.
The [retained evidence](../../tools/media-qualification/evidence/2026-10-01-selected-video-context/README.md)
includes every attempt, source manifests and the test-count calculation.

See [picture timing](../SOURCE_VIDEO_MAPPING.md) and
[Original moments](../SOURCE_MOMENTS.md) for the behavior contract. This increment
adds no Trim command or native control. DP-02, DP-05 and DP-20 remain partial.

## Exact selection and shared lookup

`SelectedPlacement` retains full-span start and duration separately from its
half-open visible window. `ExactSourceSpan` preserves fractional source ticks
and rejects mixed clocks or empty/reversed intervals. The picture plan projects
the selected endpoints through the unchanged affine map. Endpoint holding uses
only PTS intervals intersecting that selection; a frame starting exactly at its
end is excluded. Inverse source anchors reject hidden context and retain exact
selected endpoints.

The shared lookup validates the complete retained context against the measured
index before applying the smaller window. Even a directly constructed public
`Picture` cannot use a valid small selection to admit an incomplete context,
escape its retained span or change source clocks. Ratio comparisons use bounded
continued fractions to avoid overflowing cross products at extreme values.

New tests cover independent expected VFR ordinals, positive and negative PTS
origins, fractional source ticks, a 1.5-frame selection rounded to two frames,
nested Retime/Repeat plans, exact inverse anchors and forged public pictures.
Real-media moment tests compare full context and selected endpoints with measured
CFR, offset and VFR indexes. Storage tests exercise preview, commit, reopen,
Undo and Redo with the full retained asset span.

## Format policy and review

Core schema 35/database 44 use the user's approved development format break.
Unused databases 39 through 43 reject before writable open or migration backup.
Existing adapters for databases 1 through 38 remain supported.

Independent review found that old schema adapters aliased the live video mapping
and Source node types. Those aliases could have admitted the new selection
variant into old documents or commands. A frozen mapping and Source vocabulary
now covers document, subtree, setter, occurrence, import, splice and patch paths.
Thirty-two legacy-schema cases exercise valid controls, unknown/null/escaped
fields, forward and inverse patches, and refusal to project current selections.
The final product review and historical-fixture correction review have no
remaining findings.

## Corrected checks

The retained attempts include five failures:

- The first core/plan build found an omitted `selection: None` in the audio-only
  compiled Source constructor. The constructor now declares that absence.
- The first formatting check found module ordering in `lib.rs`; scoped rustfmt
  corrected it.
- The broad workspace run reached an obsolete historical-fixture expectation
  after 2,626 passing tests. The archived fixture remains core 34/database 43 and
  is byte-identical. Its pure command/transaction checks use an explicitly
  test-only current document header. Separate checks now prove that the real
  historical package rejects read, write and migration without rewriting
  authored rows or creating a backup. No runtime migration was added.
- Strict Clippy reported a 432-byte Source variant versus the 192-byte Repeat
  variant after adding the exact ratios. `NodeKind` now has one documented
  `expect(clippy::large_enum_variant)`: Source recipes intentionally stay inline
  instead of adding a separate allocation per ordinary video beat. This changes
  neither runtime behavior nor serialization. No memory or speed improvement is
  claimed.
- The following Clippy run found redundant clock storage in the private
  compiled Source selection. The plan now retains only its exact start/end ticks;
  the validated stream span supplies their common clock when constructing the
  public `ExactSourceSpan`. This reduces the private layout without allocation
  or another lint exception. All 270 plan tests pass after this change. Independent
  review confirms that the public picture selection is unchanged.

## Automated verification

Checks use Rust 1.97.1, locked dependencies and the qualified FFmpeg prefix
`/tmp/deadpan-ui-ffmpeg/prefix` on Apple M5 Max with 128 GiB memory and macOS
26.5.2 (25F84).

The broad UI-feature workspace command exited 101 at the historical storage
fixture described above. Its preceding 2,552 non-storage tests passed. The
corrected complete storage run passed all 404 unit/integration tests and its one
compile-fail documentation test. Together these runs verify **2,956 distinct
workspace unit/integration tests**, with none remaining failed or ignored.
This includes all seven selected-picture plan tests, all 32 legacy grammar cases
and 448 app/headless tests with the UI harness feature. The complete workspace
documentation run passes both compile-fail tests, with none failed or ignored.
Final workspace/all-target Clippy with the UI harness passes `-D warnings`, and
workspace formatting passes. The final Clippy run takes 561.11 seconds.

The broad run started with source manifest
`46f37ba08f700b88acef80e2670d169d52fb89f0a52df192a2a04d7bfd7e1e5f`.
The corrected storage and workspace documentation runs use
`f794bd369e66e18f13f3570a09929cc3c7e51e77fe5e8632cb027c684e8e7354`.
Only `crates/deadpan-core/src/lib.rs` module ordering and the historical storage
test correction changed between those manifests. The final plan tests, formatting
and strict Clippy use
`d081b1e12c0e847c85a050e4b52af9e36e4734624b78ae9fbb9a8af2223ae4e6`,
adding only the reviewed `NodeKind` lint expectation and compact private plan
selection described above. Previously passing unrelated suites were not repeated.
The archived range fixture's SHA-256 remains
`ea862010de94871087d282da55f982c0b967abb9f06d688af64f1dd7a5de6aca`.

Debug linking retains the existing `__eh_frame` size warning.

## Scope

There are no native UI changes, and no native app was opened for this increment.
Prior GUI, physical-input, performance and release measurements are not extended
by these tests. Native `,v`, In/Out/Slip/Roll, ripple versus overwrite, visible
handle clamping, outgoing/incoming pictures and candidate waveforms remain open.
Existing Source/Partition promotion also needs explicit framing-owner and audio
phase semantics before Trim can change those domains safely. Extending before a
Source's local zero needs an exact translation to its retained audio clock.
Moments with no audio overlap still omit audio; future extension must retain
dormant linked intent separately from intentionally absent audio.
