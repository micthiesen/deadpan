# Exact speed updates around an inner Repeat, 2026-10-20

An ordinary tape-speed stage with retained exact pause clocks can now change
its output duration around one inner Repeat level, including independent plays
and gaps. DP-07 remains partial. Base `92907cb0`; Apple M5 Max, 128 GB,
macOS 26.5.2 (25F84), Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Executed old-code witness

Public commands build a three-play Repeat with a default two-frame Source and
one-frame RoomTone Hold, a separate three-frame Source/RoomTone middle play,
a two-frame owned first gap, and a one-frame default gap. One-frame lead and
tail make its group 15 frames. `WrapRetime` produces 30 frames; public
`InsertTime` inserts three frames at root frame 3, leaving the stage at 33 frames
and its exact selected input at `33/2` frames. Two-frame outside Sources make a 37-frame root.
All graph, clock, occurrence and sample-phase controls pass on the old code;
public `SetRetime` to 18 then refuses `repeated exact pause contents`.

The retained old test binary has SHA-256
`d45789dfde718ac3876d6a70bc5d9c9f6f559672525dc76290126b64bb9ccdeb`.
Its exact test source, old document and request, failure and identities are retained.
The updated command produces a 22-frame root, keeps the exact selected input, all
interior bindings and descendant clocks, Repeat identities and override maps,
and inverts exactly.

## Admission and retained sound

Admission walks physical owners, distinguishing node bindings from default-gap
bindings. It obtains the current affine scale through indexed ancestry without
inventing an occurrence. Historical lattice templates remain authoritative.
Compact current runs are divided at sparse overrides, final-gap identities and
historical captured-repeat/run birth boundaries. The existing resolver supplies
the actual historical sampling unit for each proven-uniform rate class. Work
is bounded by authored structure and a shared budget, never by the play count.
Tests cover a million plays, historical alias changes, reordered final gaps,
1024 unrelated physical owners and dormant default/owned gaps.

A public `InsertPlays` case retains old root-clock samples at `5/16016` local
frames per sample beside a born PointCeil play and gap at `5/8008`. Output 1500
would leave the survivor's transfer `11/500` inside `1/64..64` but the born transfer
`11/1000` outside it, so both query and command refuse atomically. Output 18
passes. A representative play's origin or rounded support never proves an
entire class empty; repeated rate classes receive no empty-carrier exception.
Direct silent Holds keep their typed zero path, while default gaps, including
silent ones, still receive transfer checks.

Missing carriers use the same pre-edit capture factory as commit. Tests prove
one new timing table, exact factory lattices and retained AllocationEntry
steps, literal origins/sample boundaries for first/last defaults, owned middle
and default gap, and independently moved post-update geometry. No whole-Repeat
binding or collapsed resampling stage is introduced.

## Verification

Formatting and strict Clippy pass. The broad changed-package check ran
2,728 core/audio/CLI tests: 2,726 passed and two unrelated failures were retained.
Both pass in a serial targeted follow-up after the disk-full fixture repair
below, with strict lint passing again. The broad run also skips nine ignored
tests and the complete 45-test preview-export binary; this increment executes
its new export test separately in release mode. Existing passing results are
preserved rather than rerunning the entire suite.

The original code-signature verification hit its 60-second timeout. Its isolated
rerun passes in 10.17 seconds without a code change; the cause remains unproven.
The full-cache-volume test expected a worker error but APFS refused host
staging first. Its reviewed test-only repair creates the first production
staging file before filling, consumes it once and retains fresh retry outputs.
The strict worker-reported disk-full, classification, cleanup, publication and
recovery checks still apply and pass in 3.28 seconds. The fixture-only repair is
commit `1d89245a`. The disk-full error is
non-retryable, so this case does not itself exercise a retry.
The initial cancelling focused run reported two passed tests as leaky; the
complete broad run reports no leaks. Their initial cause was not established.

The decoded PCM regression independently transfers each of eight physical
Source, RoomTone and gap carriers at `11/6`. It checks complete seam-spanning reads,
alternate partitions, masks, JSON/inverse and a second public update back to 33.

The release H.264/AAC export passes all 22 literal preview expectations and
all 22 decoded pictures. Its complete 35,235-sample audio window has measured
offset zero, no uncovered samples and no verification flags. Maximum block
level difference is 0.084971 dB; minimum block SNR is 14.939826 dB. The report
retains 32 observable and ten unobservable neighboring-index comparisons.
The executed CLI SHA-256 is
`7c2b2ed48b5402b501c8101f5230c10c4ef7a9ee4f18607700f84c017aa182e7`;
the release test is
`b6ac390fd8113f6276b66ef058b8e5f7fe221c22cca89149c9bd82e8f9ab6184`.
The actual movie and complete report are retained.

Independent core and audio reviews found no remaining concrete issue. The
initial test compile failure (Box versus contained bound), obsolete blanket
Repeat refusal, invalid empty-reanchor expectation and processing fixture that
failed before its intended check are retained with their corrections. Strict
lint also caught a redundant iterator conversion; formatting was corrected.
The indexed ancestry implementation replaces a repeated whole-subtree
scan proposed during development. A scope-root scale error in an earlier
scratch proposal was corrected before production application.

Nested inner Repeats, Repeat/non-Sequence ancestors, processing families and
fractional own output extents remain outside this path. Existing retimed-path
restrictions on marks, placed/beat sounds, routes and allowances remain.
Historical templates with additional live axes fail closed. The shared native speed resolver and
inspector call the same core API; no app, keybinding or presentation code
changed, so unchanged UI-feature tests and painted replays are not repeated.
Physical input, listening and performance claims are unchanged.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-20-inner-repeat-speed)
contains commands, failures, final reports, source/binary hashes and review.
Root lockfile SHA-256 remains
`1e39d18ae4de6c7b898edad39eb7104235fb3ffe3af84521ca94df2b6c25b71b`.
