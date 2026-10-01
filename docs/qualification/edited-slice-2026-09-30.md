# Edited slice core, 2026-09-30

This checkpoint implements immutable capture and atomic seam insertion of edited
contents through the core and headless command path. It does not establish the
native edited-slice register, replacement, move, role-only placement or temporal
occurrence interiors. The [contract](../EDITED_SLICES.md) records ownership and
the retained picture/audio clocks.

## Focused evidence

Runs use `rustup run 1.97.1 cargo ... --locked` and the existing qualified LGPL
FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix` where media is needed.

- Fifteen core tests pass: immutable capture, command serialization, fresh
  identities, exact mark bias/fragments, hidden/unresolved intent, absolute
  Sequence pins, one-time destination mark movement, nested partial recopy,
  lineage pairs, compact play-union overflow and rejection of forged capture
  provenance. The root-sound regression retains its one event and allowance,
  excludes catalog media from capture and adds exactly one destination route
  transform. The 16 sound-routing and 12 sound-allowance regressions also pass.
- Four synthetic picture-plan tests pass. Independent source PTS, VFR lookup,
  owner progress and Q32 framing oracles cover cropped Source/Hold contexts,
  captured geometry, nested clipping, full Repeat overrides/gaps, Preserve
  retiming, deleted originals, independent copies and partial copy-of-copy.
  Every unchanged destination picture is compared across insertion.
- Eight actual decoded-PCM tests pass at 30000/1001 fps with a 44.1 kHz source.
  They cover Source and RoomTone crops, old insert/delete clocks, complete
  Preserve preparation, exhausted support, independent copies, reordered Repeat
  histories, sparse overrides, retired birth Run support and unrelated Repeats
  sharing raw play IDs. Cold final reads and reversed irregular blocks retain
  the same samples. A separate authored-bus test checks group versus contents
  gain ownership.
- Thirty-three store tests pass across the edited-slice, source registration,
  generated admission and generation-bundle targets. They include capture
  without history, copies surviving source deletion and reopening, independent
  paste identities, exact Undo/Redo, stale revision rejection, and atomic
  rejection of invalid identity pools and clocks.
- The new CLI test passes: a serialized slice survives deletion of its source,
  dry-run changes no rows, commit records one command, malformed and stale
  requests fail, and Undo/Redo survive reopening.

The PCM run preceded the final provenance-only change. The other focused runs
share source manifest
`730b2de4752ceaffe93040c1c1a722187e5a1e075230e236ad6ac4974ba7b9e3`.

## Integrated gate

After independent review, all 2,717 locked workspace tests pass, with none
failed or ignored. Formatting and strict all-target workspace Clippy pass on
Rust 1.97.1. The gate includes the final root-sound test and all decoded-PCM
cases. No runtime source changed between these checks; their source manifest is
`70d2d02eca7b2daca2506b232a03c3f7464ecd6cb96ac261b675b2f335066815`.
Every retained source hash was checked again after the gate.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-edited-slice/README.md)
includes exact commands, exits, timings, toolchain/host identity, compressed full
logs and source manifests, scalar witnesses, early failures and review results.
Core 34/database 43 remain unchanged. This backend-only checkpoint did not
require another unchanged UI-feature run or native replay.

## Decisive timing witness

A Source starts at frame 1, exact mix coordinate 1601.6. Capturing global
`[2,4)` starts at old sample entry `B(2) - 1601.6 = 1601.4` and covers 3,203
samples. Pasting at frame 4 allocates `[B(4),B(6))`, which has 3,204 samples.
All 3,204 output samples agree with the independent retained-phase reference;
the extra final sample has real provider support. A separate crop ending at the
complete owner's terminal boundary verifies zero when retained support is
exhausted. No fixture changes the destination interval or pads an old vector to
force agreement.

The Repeat witness retains a four-play birth Run after one member is retired.
Three members remain live; the entire Run survives renaming. Reordered new
plays, an isolated gap, a play override and later default-gap changes retain
their own clocks through paste and copy-of-copy. A second witness uses unrelated
Repeats with identical raw allocation/ordinal values and different source entry
phases, so accidental identity-family merging changes observable PCM.

## Review and corrections

The first command round-trip exposed that `RawValue` cannot deserialize through
Serde's internally tagged command buffer. The captured wire now uses a bounded,
duplicate-rejecting JSON visitor before structural validation. It retains exact
integer/string scalars and rejects unsupported floating-point input.

The first PCM run passed seven tests; the eighth failed before capture because
its fixture illegally replaced captured Repeat survivors with an explicit Run.
The corrected fixture captures the unwrapped voice before introducing the
previously uncaptured Repeat. All eight tests then pass. Picture scalar oracles
were corrected during static review to apply the documented Q32 progress
rounding before interpolation; their exact clocks and PTS were unchanged.

Independent store review found that checking all media in the named revision
would also allow an exact historical catalog asset from outside the declared
selection. Paste now deterministically verifies the entire capture against that
immutable revision before granting historical media reuse. Tests reject the
unselected asset and forged receipt, artifact and revision provenance without
changing authored rows. Generic source and generated acceptance guards remain
in place.

The final independent core review reported no findings in capture, clock and
identity renaming, marks, bounded wire admission, command dispatch or the store's
complete historical recapture check. It checked that frozen audio equality
includes gap overrides and lineage. The reviewer did not run Cargo; the parent
owns the integrated checks.

A correctly captured accepted Hold can be pasted after its last occurrence was
deleted. Its artifact remains identical and the old generation request stays
Detached. A qualified Source can be pasted after legacy registration Undo;
the stored receipt is reused, and an unrelated catalog asset is not restored.
The generated-bundle fixture is synthetic admission evidence, not a decoded
movie or an AI quality result.

## Limits

Picture checks here inspect the canonical plan; they do not include a new GPU
or decoded-picture comparison. Audio checks establish the tested sample routes
and gain ownership, not a mastered export, acoustic quality or device delivery.
Native controls and keyboard routing did not change, and no new native UI,
accessibility or Kestrel replay was run for this backend checkpoint. The user's
separate Deadpan window and desktop were not inspected or changed. No DP
requirement or release gate is marked complete by these results.
