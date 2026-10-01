# Source-origin timing, 2026-10-01

An exact offset now maps each retained audio placement from current physical
coordinates to its historical clock. A checked binding rebase preserves sampling
when a Source's existing material moves behind an added physical prefix.
The implementation base is `f2388e5a379288ea2a9284f7cf40a2c2ac1ee642`.
See [the contract](../SOURCE_ORIGINS.md) and
[retained evidence](../../tools/media-qualification/evidence/2026-10-01-source-origins/README.md).

## Behavior

The offset changes the resolved origin and local support. Frozen layouts,
sample grids, rates and Repeat identities stay fixed. Resume boundaries and
symbolic phase endpoints move with current-local coordinates. Chronological
reanchor entries convert from historical-local coordinates before composing
their sample distances. Reanchor windows retain their enclosing captured clock.

The pure operation returns a complete binding or an error, leaving its input
untouched. Current documents and patches retain the new offset. Supported
historical readers reject the new vocabulary, including explicit zero/null and
escaped field names. Core schema 37/database 46 record the change; unused
development databases 39 through 45 refuse before writable acquisition or backup.

## Exact PCM cases

The synthetic stereo 48 kHz WAV fixture is the existing
`native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav`. Its admitted
extent is 8197 samples. No new media fixture was generated.

At 30000/1001 fps, one frame spans 8008/5 samples. A one-frame lead allocates
1602 samples, leaving a 2/5-sample phase at the Source's beginning. The first
PCM case compares plain, newly captured and previously moved/bound states
against an independently authored resampling oracle, retaining a separate
seven-sample audio offset. Physical prefixes of one and three frames behind
a unity Partition preserve the selected output and its edge fades.

The second case retains a symbolic resume and two chronological reanchors.
Independent boundaries yield 1602 + 1601 + 1601 + 64 = 4868 source samples.
The expected PCM differs from a deliberately one-sample-shifted oracle.
Both prefixes preserve this result and the retained sample-boundary queries.
Raw and edge-faded reads use different valid block partitions. Inverse patches
restore the exact prior document and PCM.

## Corrected fixture setup

The initial PCM tests requested 511-frame oracle blocks, exceeding the existing
256-frame limit, and a 16000-sample Source outside the fixture's admitted extent.
The first correction fixed the oracle and source span. The next run exposed
oversized stage reads with the same 256-frame limit. Baseline blocks now contain
251 frames; comparison partitions use 193 and 239 frames. Both PCM tests pass.
All failed commands and their source identities remain in the evidence.
No product code changed in response to those setup failures.

The first workspace build then found a test-only API mistake: the plan regression
called `unwrap()` on the integer returned by `ExactRatio::floor()`. Removing that
extra call lets the test compile. This failed build ran no workspace tests and
is retained separately from the corrected run.

The debug app link emitted an `__eh_frame` size warning: its unwind section was
too large for the compact unwind table. The build did not treat it as an error.
No exception-performance or release-binary claim is made from this debug build.

## Review and automated verification

Independent review found no actionable issues in the model/resolver changes,
historical closure, PCM arithmetic, storage/history test or format integration.
Workspace formatting passes. All 3,019 workspace unit/integration tests and
both compile-fail documentation tests pass, with none failed or ignored.
The corrected run completed in 694.74 seconds using Rust 1.97.1 and locked
dependencies. Strict workspace/all-target Clippy with the app's `ui-harness`
feature and `-D warnings` passes in 755.18 seconds.

The passing workspace run, final formatting check and strict lint share source
manifest `8aa01bb8b1783d7476bfdbe425a7f9fdc4c71c90f6bb7d992087b157fbe4c039`.
The host is an Apple M5 Max with 128 GiB RAM, running macOS 26.5.2 (25F84).
The process check at 12:26 UTC found no running Deadpan app.

The passing storage regression covers durability of an already rebased Source.
It checks nonzero offsets and unchanged frozen layouts through create,
reopen, serialized deletion history and fresh-revision Undo/Redo. It does not
introduce or qualify a public Source-extension command.

## Limits

No native app was opened. This does not qualify native Trim, physical input,
device playback, render/export or release packaging. Framing and gain semantics
must also retain their authored timing before an atomic Trim command can expose
physical Source growth in the product. No requirement or gate is completed.
