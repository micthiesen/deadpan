# Exact Source windows and Slip, 2026-10-01

Implementation base: `1ea3e4f4b70a805992a56455f8db8d95bf401a7e`.
Core schema 39/database 48 add [exact editorial windows](../SOURCE_EDIT_WINDOWS.md)
and a bounded [atomic Source Slip command](../SOURCE_SLIP.md).

## Focused verification

Corrected focused runs pass 47 core integration tests, three checked-window
value/property tests, 11 indexed-picture tests, 33 media tests, 44 storage tests,
one headless Slip test, six retained-clock PCM tests, four dormant-audio tests and
three audition-range tests. Counts describe separate targets; they are not added
to the full workspace count.

The picture witnesses use signed-origin VFR intervals, literal PTS and ordinal
tables, both handle clamps, exact anchor relocation, rounded tails and retained
Source/live ancestor camera clocks. These are plan/index checks. Storage and
headless witnesses use actual registered MP4 receipts and verify atomic history,
fresh Undo/Redo IDs, reopen, stale/no-op handling, corrupted admission, forced
rollback and shared writer dispatch.

The PCM witnesses read the real 48 kHz stereo WAV through a test provider with
an explicit speaker matrix. Synthetic video defines the shared clock; it does
not provide measured video admission. Independent phase, filter-support and
gain/mute arithmetic covers both Slip directions, six captured/moved/resumed/
rebased variants, dormant activation, symbolic resumes and chronological
reanchors. An independent root sound stays on its original clock. Every PCM
comparison is exact, including reversed cold chunks and inverse restoration.
Production audition uses qualified AAC fixtures for PCM comparison. The plain
44.1 kHz WAV retains exact timing and an explicit `UnsupportedLayout` refusal;
the test does not infer speaker layout from channel count.

## Corrections retained in evidence

Original failed command logs remain beside corrected runs:

- The core sound fixture incorrectly refit its full retained span; it now uses
  its required natural-rate 117-frame duration.
- Integration-test module paths and the CLI helper import needed explicit paths.
- The media test's expected ordinals omitted frame-center sampling after its
  negative A/V origin; the corrected expectations include the exact arithmetic.
- PCM oracles initially allowed interpolation outside selected source support.
  Their independently calculated selected sample bounds now match that policy.
- A moved fixture's independent sound crossed the leading silent Hold's exact
  end and correctly acquired an automatic 96-sample fade. The test now declares
  that Hold edge Hard to isolate Slip clocks. Its first edit needed the complete
  typed edge object because default edge fields are omitted on the wire.
- The audition test initially tried to play a WAV with unspecified speakers.
  Qualified AAC now establishes PCM equivalence; the WAV checks exact plan
  equality and unchanged layout refusal.
- The first workspace run exposed two historical single-Original baseline
  failures. Measured baselines now include an exact window, while historical
  upgrades intentionally retain its absence. Baseline validation now permits
  that absence only when every other measured Source field matches exactly.
  Explicit incorrect windows still reject; stored baselines are not rewritten.
- The new baseline policy test initially called a nonexistent timestamp
  constructor; it now uses the type's explicit checked-span coordinates.

The corrected baseline policy, all 131 migration tests and eight current
single-Original tests pass. Independent window, Slip, PCM and baseline reviews
found no outstanding correctness issues.

## Workspace verification

All 3,114 workspace unit/integration tests and both compile-fail documentation
tests pass, with none failed or ignored. Formatting and strict workspace/all-target
Clippy with `deadpan-app/ui-harness` and `-D warnings` pass. Workspace tests took
880.819852 seconds and strict lint took 807.659273 seconds.
Debug linking retains the existing `__eh_frame` size warning; lint has no warnings.

All three final checks used the same source manifest:
`1fc905d263ff3f486524d866d696a22aab0fc63e401b7ed55f26426fa3bee87b`.
The host was Apple M5 Max with 128 GiB memory, macOS 26.5.2 (25F84), Rust 1.97.1,
locked dependencies and the pinned FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.
The [retained evidence](../../tools/media-qualification/evidence/2026-10-01-source-slip/README.md)
includes exact commands, failed and corrected runs, source manifests, reviews,
fixture hashes and the final no-app process check.

## Scope

The operation retains beat placement, duration, complete source spans, source
effects, audio bindings and root sounds while shifting both enabled maps.
Receipt admission and optimistic revision checks run again at commit. Dry runs
report exact/integer limits and clamping, including no-op results without history.

No native app has been opened for this increment. Native `:slip`, Trim mode,
In/Out/Roll, ripple/overwrite, physical source growth, nested/treated targets and
an index-derived held-picture policy for audio-only lead/tail remain open. These
checks do not qualify display, device playback, export or release packaging.
No requirement or gate is complete.
