# Whole-Repeat following creep and live target clipping, 2026-10-24

Whole ordinary Repeats now accept Keep-center following creep. The shared
target projection also respects every live intermediate crop. Focused and
broad checks, independent review, release build, native replays, both emitted
media cases, independent raw-result audit and immutable package verification
pass. The CLI-derived case retains its original wrapper failure and passing
offline reconciliation. DP-08 remains Partial.

## Behavior

A directly selected whole Repeat outside an enclosing Repeat or explicit
occurrence scope can keep its plain Follow while creep adds a scale transition.
The complete Repeat `OwnerOutput` clock spans all plays and only intervening
gaps. Target centers use the current provider's exact Original point, resetting
with each play. Three plays means three total plays and two intervening gaps.

Existing `RepeatEscalation` stays a separate inner layer, including the owning
play's gap. Child structure, stable iterations, layout, sparse branches, gap
ownership, provider, audio, duration and authored child framing are unchanged.
Descendant kinds remain unrestricted. Retime owners, nested/enclosing Repeat
occurrences, explicit selected-play scopes, all Visual ranges including a
full-owner range, and implicit replacement of an animated Follow or envelope
retain explicit refusals. The earlier CLI Sequence test now proves whole-Repeat
success and exact Undo. The native Sequence scenario removes its obsolete
ordinary-Repeat refusal; dedicated Repeat assertions supply the positive
workflow. Retime and occurrence negatives remain.

During qualification, an independent renderer comparison exposed a real
preexisting defect: Follow without captured context projected through live
poses but ignored their intermediate clips. The renderer had already discarded
those pixels. The shared helper now clips after every live scope regardless of
whether captured geometry exists. A later zoom-out cannot restore a target
discarded by an earlier scope. A target whose center is clipped uses the
evaluated fallback while scale animation continues. Existing Lost-region holding remains;
its held center must also survive the clips to resolve live.

This intentionally corrects the earlier no-context exterior-center behavior.
Legacy targets without a measured aspect keep their existing coordinate
placement; clipping still applies after the live scopes. Captured projection
and live projection now share irreversible scope visibility. Core 58 / SQLite
87, wire representation, curves, Q32 numerics and dependencies are unchanged.

## Executed checks and review

Focused lanes overlap broader checks; counts are not a sum.

| Check | Observed result |
| --- | --- |
| Independent live-scope renderer comparison against the old helper | Failed, exit 101; exact old sources and raw failure retained. |
| Fixed full `follow_letterbox` renderer comparison | Three passed, including 600 live and 960 captured geometry cases. |
| Core Follow builder and semantic focused tests | Two passed. |
| CLI framing integrations | Fourteen passed; 46 filtered, none ignored. This is not the full CLI suite. |
| Repeat picture-plan focused tests | Three passed; 130 filtered, none ignored. |
| App all-target check and strict Clippy | Passed on the current twenty-one-source scope. |
| Independent general, clock and test reviews | All twenty-one scoped source paths reviewed; no findings. Reviewers ran no product or Cargo checks. |
| Full Core and Plan suites | 2,053 passed; none ignored. All twenty-one scoped inputs unchanged. |
| Full App unit suite | 1,304 passed; eleven existing opt-in cases ignored. All twenty-one scoped inputs unchanged. |
| Scoped formatting and release build | Passed with all twenty-one scoped inputs unchanged. |
| Native Repeat / Sequence / zoom / Hold replays | 129 / 96 / 213 / 67 workflow checks plus each Kestrel audit pass. |
| Native image review | Four Camera/gap/final images inspected. |
| Actual native-project emitted media and portable document | All eighty pictures, complete 128,128-sample stereo window and exact copied document pass; all nine CLI commands succeed. |
| CLI-derived clipping case | All 181 product commands return zero; original wrapper exits one on empty-WAL metadata, with passing read-only reconciliation. Eighty pose/picture checks, 28 clipped frames and full audio verify. |
| First independent raw-result audit | No findings; current sources/binaries, both media/history cases and preserved wrapper failure agree. |
| First independent package verification and supplement | Passed; immutable Primary/current source/audit bytes unchanged, complete readback passes. |

The Core builder exercises Source/Hold/Sequence/Repeat authority and both scale
bases. The semantic witness uses a three-play Repeat around unequal children,
default gaps and centered escalation, with independent scale, complete-document
preservation, replay/inverse and atomic scope refusals.

The real-project CLI lane tests no, centered and targeted escalation across
default frozen gaps and owned Background gaps in both ownership orders. Every
output frame compares provider/source identity, target time, context, owner and
child clocks, gaps/iterations and unchanged inner framing against baseline.
All owner scales, target centers and targeted escalation centers have independent
integer/rational oracles. Dry-run, durable Undo/checkpoint/reopen/Redo, named
replay and inline Keep re-resolution retain SQL/register authority. These
video-only fixtures preserve silence-plan spans and audio intent; they are not
emitted PCM evidence.

The pure Plan fixture has three four-frame plays, a two-frame default gap and
one three-frame captured owned gap. A retained final owned gap is dormant and
excluded from duration. Twelve shape/default-gap/curve combinations without
escalation and twenty-four with centered/targeted escalation check every owner
frame, including tracked interpolation, Lost holding, manual correction and
fallback. Two additional point/region witnesses prove a later escalation
zoom-out cannot restore a child-clipped target. Complete nonempty Source audio
spans and audio duration remain equal. This does not qualify actual gain/DSP,
arbitrary nested repetition or every gap/escalation combination.

One exact boundary is deliberate: the scale-three target clamp's `1/6` left
margin rounds up to `715827883 / 2^32`. A source-left edge point then maps to
exactly `-1 / 2^32`, outside the renderer's inclusive `[0,1]` visibility.
The outer Follow must use fallback. No epsilon or clamp-policy change hides
that result.

The 600 live and 960 captured comparisons use independently composed renderer
geometry, without GPU readback. Native Metal replay and emitted-file comparisons
are separate lanes below.

## Source provenance and retained failures

The old live-clip comparison actually ran and failed against the old helper.
Five exact old source copies and before/after receipts retain that witness;
the fixed full three-test comparison passes. Older Plan expectations of
exterior live centers are updated intentionally. Separately retained early
Repeat Plan fixture drafts were never executed and are not failed-run evidence.
The final focused Plan run passes on the corrected visibility semantics.

The first native wrapper invocation placed `--binary` after the positional
run label, inside argparse's command remainder. The runner tried to execute
`--binary` and raised `FileNotFoundError` before the App spawned. This is a
harness invocation error, not a product failure or replay result. The corrected
invocation places binary options before the label; only actual App replay
results count as native evidence.

All twenty-one captured paths stayed unchanged during the Core/CLI and renderer
runs. After those passes, only two explanatory Camera strings changed from
captured-view-specific clipping to earlier-crop visibility. The exact old App
source and replacement proof are retained in `camera-help-source-note.json`;
the other twenty paths remain byte-identical. That is a text-only App delta,
not a functional Core/Plan/CLI change. The current Plan focused run, App check
and strict check bind all twenty-one current paths. The full Core/Plan and
App suites, final formatting and release build then pass with that complete
current scope unchanged. Existing ignored tests are not verification evidence.

The App unit executable hash was observed only after the tests had terminated:
`ca1c96a16f08fee0a8c006d8f473936d0e8194febb4b797dba74856b8f5b9462`.
No pre-run or live hash was captured. This terminal identity does not prove
unchanged executable bytes throughout the run; the complete twenty-one-source
before/after receipt remains the source binding. It is a provenance limit,
not a failed test.

The retained host record, observed at 11:09:56 UTC, reports Mac17,7, 128 GiB
memory, eighteen CPUs, macOS 26.5.2 and virtualization flag zero. Power/thermal
and OS file-cache state remain uncontrolled; no hardware-tier or latency claim
is inferred.

## Native witness and media qualification

The authored `repeat-follow-creep` scenario uses production keys, commands and
project writes with the qualified 120-picture counter Original at 30000/1001.
It saves a point at Original 0, manually corrects it at indexed Original 12,
retains Original `[0,24)`, then creates three total plays with four-frame
default Freeze gaps and centered zoom-step escalation. No detector/tracker
seam or authored fixture mutation is used.

The actual native edit has eighty pictures, with a verified export audio window
of `[0,128128)` stereo samples at 48 kHz. Plays start at 0, 28 and 56;
gaps are `[24,28)` and `[52,56)`, both
holding Original 23. There is no final gap. Centered escalation remains the
separate inner scale 1, 9/8 and 5/4. The saved point changes from source
`(.58,.5)` to `(.68,.5)` at Original 12 in each play.

All eighty scale/target samples have independent Q32 oracles. Sixteen stopped
boundaries check exact presentation and source identity across play, correction
and gap seams. History, Camera preservation/release, semantic replay/dot,
full and partial Visual refusal and exact nested/selected-play branch authority
pass in the actual 129-check replay. The native fixture uses default frozen
gaps; explicit owned gaps are covered by the pure/CLI witnesses above.

The candidate App is
`16e7ee4a3de98eea74bdddd80554f799f8c4965aa7b7bc8c9b99f794cc3f88a3`;
the CLI is
`bf3b5e5ff9b1767b2c071928175e60cdc6ce7a1bb30ae1ad6ae74c6fa16edcae`.
One candidate build and one actual Repeat workflow qualify this slice.
First/final snapshots bind identical bytes for all twenty-one source paths
and five sibling executables; they are not two independent builds or runs.
The corrected `native-repeat-run` is the passing Repeat workflow. All source
and executed-binary guards remain unchanged.

An intended combined regression invocation repeated `--scenario`; only its
final value ran, producing 96 Sequence checks. Its retained scope note states
that limit. Separate zoom and Hold invocations then pass 213 and 67 checks.
No combined invocation is credited for those two scenarios. Repeat, Sequence,
zoom and Hold retain three, three, three and two layout/screenshot warnings
respectively; no ignored-layout retry or repeated-same-cause failure occurs.
Every replay's Kestrel audit checks 62 reservations and 21,885,504 routes,
with no conflict and matching live/pinned source SHA-256
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
These are not latency measurements.

Root inspected Camera 017 for draft controls/focus/units, Camera 070 for the
selected whole Repeat following its target before creep, 121 at the first
silent gap boundary and 124 at the third-play boundary/final edit. The gap
shows Original 23; the final inspector retains eighty Edit versus 120 Original
frames, three plays, two four-frame gaps, escalation and the Follow scale range.
Keycaps and inspector are readable, with no overlapping labels or hidden
controls observed. Camera 070 is after Repeat construction, before creep;
later Camera round trips are semantic checks, not unretained image claims.

The actual saved native project exports eighty pictures at 30000/1001,
320×180. Every decoded picture passes the committed shared picture-path
comparison. There are 114 observable and 44 unobservable neighboring-picture
comparisons; unique index observability is not claimed for every counter
picture. The complete `[0,128128)` stereo audio window passes with zero
uncovered samples, zero offset and no physical gaps. Encoder-delay metadata
retains audio edit media time 2,048 samples with no empty edit. No event-based
alignment or encoder-block tolerance conceals an offset. The reference is the
canonical limited bus, not an independent DSP oracle or listening check.

The emitted 105,129-byte MP4 hashes to
`9c8b9d241299e9c4e50ce48cc66849285ab637d06ceb27853f9d71a50cfc5915`.
All nine CLI commands pass and the portable authored document is exactly equal.

## CLI-derived clipped target and independent audit

A separate case copies the native project and uses a production CLI correction
to change only the point at indexed Original 12 from x=.68 to x=.98. It is
CLI-derived, not another native-authored workflow. All eighty independent pose
comparisons pass, including 28 clipped frames `[40,56)` and `[68,80)`. The
evaluated fallback retains authored x center `311385129/536870912`
(approximately .58) and y center `1/2`, not an assumed x=.5. Scale continues
on the Repeat clock.
Audio-plan spans remain equal. No separate canonical PCM/gain-bit before/after
comparison was run.

The derived emitted file verifies eighty pictures and the complete 128,128-sample
stereo window at zero offset, with no gaps or uncovered samples. Its neighboring
comparisons are 104 observable and 54 unobservable. Audio edit media time is
2,048 samples, with no empty edit. The 91,654-byte MP4 hashes to
`8e0c0ef23d90b74d05427e6f791defe7bc05bb1071706cc97b5bd622dd421f66`.
Portable copying and actual target Undo/Redo preserve the expected authored
document and target.

All 181 production CLI commands return zero. The wrapper then exits one on its
final byte guard: an absent `project.sqlite-wal` becomes an empty file, SHA-256
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
The native authored document/database and every preexisting guarded persistent
file remain exact; the original SHM exclusion is unchanged. This is an overly
strict evidence guard, not an authored mutation or failed product command.
The original exit one, failure streams and missing historical `result.json`
remain preserved.

A separate read-only reconciliation passes without a new product run. It
checks all 181 command outcomes and 362 stream pins, all twenty-one sources,
executables, the original script, native before/after documents, independent
poses, emitted media and portable Undo/Redo. It permits only that exact
absent-to-empty WAL metadata difference and does not create a historical result
or rewrite the failed receipt. Reconciliation receipt SHA-256 is
`fbb489090c84c6445dc06e35d5eb35be0a3105bb7649324f632da5776d729d8e`;
its script is
`0642f7e50073489c22a3e7b1dc3770f96d0b79efcfc6cc848044aee786742bc0`.

The first independent raw-result audit passes with no findings. Current
twenty-one source paths, five executable pins, both media cases, exact immutable
copies/history/redo/transaction-step records and the preserved wrapper failure
agree. Native and original portable packages each have twenty revisions and
twelve history rows. The derived package has twenty-one revisions/thirteen
history rows; its portable package has twenty-three/thirteen after actual
Undo/Redo, which creates two immutable traversal revisions. These are distinct
states, not a claim that all revision tables are identical after traversal.
The frozen raw audit receipt SHA-256 is
`7b01c81a5c2a3efcdc70b05931c8752d621dd2c7adc5739214b3e4057a7515b8`.

## Retained evidence

The immutable [Primary evidence](../../tools/media-qualification/evidence/2026-10-24-repeat-follow-creep)
has 165 payloads, 11,068,281 bytes plus its inventory. Inventory SHA-256 is
`dcee080990be595f9826917c3d55dceee8a402600432f55b9804085710db049d`.
Its 162 encoding rows comprise 128 identity records, 26 gzip records and eight
TAR archives. The archives retain 473 regular members totaling 8,852,173 bytes.
The collection charges 156,028,443 raw payload bytes against the 256 MiB bound.

There are four explicit metadata omissions: the native project's and derived
project's empty `project.sqlite-wal` (zero bytes each) and paired
`project.sqlite-shm` (32,768 bytes each). The archive ledger retains their exact
hashes and observed-empty-WAL reasons. No authored history, media or other
authored payload is omitted; authoritative databases and immutable histories
remain retained. The four metadata omissions total 65,536 bytes.

The first independent package verifier exits zero with no findings and empty
stderr. It verifies all 165 payloads, 162 encoding rows, 34 gzip streams and
473 regular TAR members. Ledger-stored bytes are 10,914,655 and decoded bytes
156,306,254, within the 256 MiB bound. Primary, all twenty-one current source
paths and audit receipts remain unchanged. Frozen verification receipt SHA-256:
`0319a245f5c0db913d22ce52f002ff144b36f3063c7f1e94041a3900d2f599b6`.
The separate [verification supplement](../../tools/media-qualification/evidence/2026-10-24-repeat-follow-creep-verification)
retains eight payloads, 53,834 bytes plus its inventory
`9af2236ab2ea4de777e2890a70ee87977431bcaa9fc154a97a41188fc1036417`.
Root's complete file/name/hash readback passes for both packages.

Authored code/documentation diff checks pass. The staged source/evidence check
exits two only for eight new blank-at-EOF warnings in immutable test logs and
source fragments: `commands-write.stdout`, `live-clip-before.stdout`,
`live-clip-fixed.stdout`, `native-header.rs.txt`, `native-history.rs.txt`,
`plan-focused-1.stdout`, `worker-cli-1.stdout` and `worker-core-1.stdout`.
Those evidence bytes remain exact; they are not trimmed to satisfy a source
whitespace check.

## Remaining scope

Base is `cb89b8b79a01a4ad59fc29dc572ed65015a3c3af`; scratch is
`/tmp/deadpan-repeat-follow-creep-2026-10-24`. Twenty-one source paths define
the final qualification scope. Earlier Source/Hold/Sequence evidence stays
unchanged; this record explicitly supersedes its no-context exterior-center
behavior, rather than rewriting those historical results.

Retime/scoped following-creep authoring, general non-face proposals and the
remaining full normative scope stay open. No physical input/display, OS IME,
VoiceOver speech, listening, tracking accuracy or latency target is established.
Coverage remains 89%, eight Complete and sixteen Partial. No requirement or
gate closes.
