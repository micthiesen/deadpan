# Hold audio policy authoring qualification, 2026-09-27

This increment adds the backend command needed to choose room tone for an
existing pause and restore digital silence. It is not a completed native
room-tone workflow or a release gate. Base commit:
`464a7843f9bd8e91e0fc6d7b30a6e92ea424fc81`.

## Implementation and review

Core 32 adds `SetHoldAudio` and its occurrence form. Both update only the audio
recipe, preserve exact duration and retained sampling clocks, and use existing
raw-audio lineage reconciliation. Occurrence isolation happens before permission
cleanup. Only the resolved Hold loses obsolete per-sound silence allowances;
unrelated Holds, plays and default gaps retain theirs. The inverse patch restores
policy and permissions together. Existing Tail vocabulary is admitted without
claiming implemented tail rendering.

The store validates new source choices against the expected revision's asset,
immutable receipt, Original binding and measured source sample interval. The
generic command path, dry run and chronology validation share this check.
Unrelated legacy recipes are not reinterpreted. Stored admission does not prove
present byte availability; playback opens verified source snapshots. Generation
relevance still requires the existing complete observations.

Database 38 migrates database 37 through frozen core 31, whose command and
occurrence grammar exclude the setter. Sound allowances are preserved and
compared in snapshots and both patch directions. The genuine old-binary fixture
has 20 revisions, 14 history entries, a pending redo, a qualified real MP4,
retained sound routes, two permissions and abandoned branches. Its producer,
commands, SQL and provenance are in
[`crates/deadpan-store/tests/fixtures`](../../crates/deadpan-store/tests/fixtures/).
The preserved core-31 CLI SHA-256 is
`4fc6556c1ff44a54a82d118f41eb5120c0f0a9d7e89e1fb6fa93cc5c217d7c84`.

Independent core and storage review found no remaining implementation issues.
Review caught oversized reads in the new PCM witness; it now collects at most
256 frames per read, retaining the production bound. The focused storage run
passed all four migration tests and three admission tests, then failed because
the legacy-asset test used an invalid placeholder hash. Its fixture now uses a
valid SHA-256 while keeping qualification absent. Neither correction weakens
source admission or production limits. The failed run remains evidence.

## PCM and history witnesses

The retained nested-Preserve witness now applies the actual setter after
capturing old silence clocks. It compares canonical PCM with two complete
reference stretch passes, checks cold reads, rejects historical suppression and
asserts the exact inverse. It no longer changes authored policy through JSON.

The real-media CLI witness imports measured AAC from `cfr-bframes.mp4`, dry-runs
then commits room tone for an existing two-frame Hold, and compares the complete
3,200-sample loop with its explicit 256-sample/96-sample-overlap oracle. It checks
a cold interior read, a changed source range, separate digital silence, untouched
following Original samples, undo/redo and reopen. The tone fixture is an audio
correctness witness, not evidence of a speech-free user selection.

## Design target and remaining work

The built-in imagegen tool produced the
[room-tone interaction board](../design/boards/room-tone-board-v2.png) and
retained [prompts](../design/prompts/room-tone-board-v1.txt). Review corrected
an extra-video import button, a source-preview clock unrelated to the selection,
and silence wording that obscured explicit sound permissions.

The target keeps one pinned Original and a dominant picture, separates exact
original sample range from Hold duration, distinguishes source audition from
rendered pause audition, and requires explicit application. Its proposed native
commands, waveform and sample fields remain unimplemented. Native selection,
stale target capture, source audition, keyboard/focus/IME, GUI comparison and
listening across a representative ambience corpus remain required. The existing
native GUI is unchanged by this backend increment; its prior painted and timing
evidence is retained rather than rerun. Full effects, tails and export remain open.

## Verification

The complete locked workspace run passed **1,927 tests** across 153 groups,
with zero failures or ignored tests. Formatting and strict all-target workspace
Clippy passed. These final checks and both focused PCM witnesses use source
manifest `b45764a6b7749946b9f3ea94b7c38058518ddb27d6135f845d4f0257594cba25`.
The earlier focused core run passed 15 tests while storage work was finishing;
the final workspace run covers the integrated state and the corrected fixture.

The [retained evidence](../../tools/media-qualification/evidence/2026-09-27-hold-audio/README.md)
contains all seven terminal command records, their logs and source manifests,
including the failed fixture attempt. One owner ran Cargo commands serially;
no quiet compilation phase triggered a restart. Workspace Clippy took 275.9 s
and workspace tests including compilation took 740.4 s. These are run durations,
not application performance measurements.

Hardware: Apple M5 Max, macOS 26.5.2, Rust 1.97.1, pinned LGPL FFmpeg 8.0.3 prefix.
No app source changed, so the prior optional-harness and painted/performance
results were preserved. Physical input, display and listening acceptance were
not rerun for this backend increment.
