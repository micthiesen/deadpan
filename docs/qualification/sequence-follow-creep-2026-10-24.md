# Whole-Sequence following creep, 2026-10-24

Whole ordinary Sequence groups now accept Keep-center following creep.
Implementation, independent review, focused and broad checks, native replay,
emitted-media, immutable history/raw-result and package verification pass.
DP-08 remains Partial.

## Behavior

`OrdinaryFollowOwner` now includes Sequence. A selected direct-child ordinary
Sequence with a plain Follow may add a Keep-center scale transition without
replacing its target. The whole group's `OwnerOutput` clock drives scale;
the currently displayed child's exact Original point drives the live target
center. A cut can jump the provider clock while the group scale progresses
continuously.

The selected owner must be outside enclosing Repeat and explicit occurrence
scopes, with no Visual selection, including a full-length selection. This gate
does not restrict the kinds of its children: Source, Hold and existing Retime
fragments remain composable. Root self-selection authority is unchanged.
Repeat and Retime owners, scoped occurrences and implicit replacement of an
already animated Follow or envelope retain explicit refusals. Named replay
and direct-edit dot resolve semantic Keep intent against the invocation's
current target and scale.

Only four production files change: the owner enum, ordinary semantic
classification, native Camera classification and command help. Generic raw
Follow validity, transition numerics, separate live/fallback bases, curves,
target identity, provider/audio timing, child framing and serialization are
unchanged. Core 58 / SQLite 87 remain current; no migration or new dependency.

## Focused checks and review

Focused lanes overlap the broad-suite totals; these counts are not a sum.

| Check | Observed result |
| --- | --- |
| Core builder and ordinary semantic focused tests | Two passed. |
| Sequence picture-plan focused tests | Two passed; 127 filtered out. |
| CLI actual-project Sequence integration | One passed after correcting the reverse Visual fixture cursor. |
| CLI framing integration | Thirteen passed; 46 filtered, none ignored. All fourteen scoped inputs unchanged; includes the focused Sequence case. |
| App all-target check and fourteen-path scoped formatting | Passed. |
| Independent general, owner-clock and test reviews | All fourteen scoped source paths reviewed; no remaining findings. Reviewers ran no product or Cargo checks. |
| Full Core and Plan suites | 2,049 passed; none ignored. All fourteen scoped inputs unchanged. |
| Full App unit suite | 1,304 passed; eleven existing opt-in cases ignored. All fourteen scoped inputs unchanged. |
| Strict Clippy, scoped formatting and release build | Passed with all fourteen scoped inputs unchanged. |
| Native Sequence / Hold / zoom replays | 107 / 67 / 213 workflow checks plus each live Kestrel audit pass. |
| Native image review | Sequence Camera, before/after-cut and final images inspected. |
| Actual final native project export and portable document | All eighty pictures, full 128,128-sample stereo window and exact copied document pass; all nine CLI commands succeed. |

Core exercises Source/Hold/Sequence authority, separate bases, complete owner
clocks, explicit `from=` rebasing, existing animation, full/interior/reverse
ranges and scoped refusals. The semantic test uses an unequal-duration group,
checks every owner frame and preserves child/audio state, compound replay
and inverse identity.

The Plan fixture places a fifteen-frame group between two neighbors. Three
independently framed children map discontinuous Original intervals and another
asset. Point and rectangle targets cover tracked interpolation, Lost holding,
manual correction, half-open span-end fallback and asset mismatch. Every output
frame checks provider, context and child/ancestor framing against the baseline.
Every group frame independently computes integer round-even Q32 scale for
Linear, Smoothstep and Step, target center fractions and source ticks. Complete
nonempty Source audio-plan spans match before and after. This is pure plan
coverage, not emitted PCM or a new captured-Hold qualification.

The CLI fixture registers the 120-picture Original as video-only and adds an
eight-frame group of disjoint four-frame Source intervals with independent
child framing. Every edit-frame provider and child scope is checked against
baseline; group scales and provider-time centers have independent oracles.
Dry-run, durable Undo/checkpoint/reopen/Redo, macro re-resolution, full and
reverse ranges and actual Repeat/Retime/scoped refusals preserve SQL and
register state as applicable. Its audio-plan preservation is silence; it
does not prove emitted audio.

The thirteen-case CLI framing lane also exercises existing Source/Hold, ranged,
one-play, pause and duration framing macros. It includes the focused Sequence
test above and is not the full CLI suite.

The first CLI run failed because a reverse extending selection fixture's
cursor did not match its head. The corrected fixture and diagnostic assertion
pass; failed streams, source pins and the exact failed test source remain
retained. No product defect or weakened scope gate was needed.

Focused worker receipts bind their owned paths and declared basis snapshots.
They do not claim every captured path still matches the final tree. The first
raw-audit attempt stopped because the early focused Plan basis also captured
`crates/deadpan-core/src/semantic/planner/tests/framing.rs` while its worker was still
writing tests: 19,434 bytes / SHA `fd1bc536…` became 19,640 bytes / SHA
`86282ac6…`. Both owned Plan test files and every Plan-dependency production
path remain unchanged. The first audit attempt is preserved; the corrected
audit distinguishes this unrelated test-basis delta from production inputs.
Both Plan tests executed again within the final full Core/Plan run below, which
binds the current fourteen-path scope. No additional product rerun was needed.

The full Core/Plan and App lanes, strict checks, scoped formatting and release
build bind all fourteen paths with unchanged before/after inputs. The App unit
executable was observed during execution and after exit with the same SHA-256,
`e64c4181da7f7039c06033dae4ce09e2a66bfbad8eb52881102db02840f75300`.
Existing ignored tests are not qualification evidence. No source changed after
the three independent reviews or these broad checks.

## Native and media witness

The authored scenario `sequence-follow-creep` uses production Camera keys,
commands and project writes, without a detector/tracker seam or authored fixture
mutation. It creates a point at source `(.58,.5)` and corrects it at indexed
Original 80 to `(.68,.5)`, deletes half-open Original `[40,80)` and groups the
two remaining forty-frame fragments. Those fragment wrappers remain valid
children of the ordinary selected Sequence.

The actual edit has eighty output frames: Edit 39/40 jumps from Original
39 to 80. Keep-center creep from 1.1× to 2.2× spans the complete group clock.
The scenario contains an independent all-frame integer Q32 oracle, exact
provider/target/child-scope comparisons, seam-adjacent stopped-picture checks,
unchanged audio spans and both cursors, separate target/framing history,
Camera no-change/cancel/release, recording, named replay, dot and wrapper/range
refusals. The production-key replay passes 107 workflow checks.

The candidate App is
`ed85fa1bac7a809b5ba4c5b77d758ff08c5a3fdd8ad9a8338034c48bc0623434`;
the CLI is
`923245545410e43a4d1248c03206a594e90ffff1ff0d83312ccdc69744d1c700`.
One candidate build and one Sequence workflow run qualify this change. The
first/final manifests bind identical bytes for all fourteen scoped source paths
and five sibling executables; `candidate-equivalence.json` records this.
`native-sequence-first` is the final passing Sequence workflow, not one of two
independent runs. Hold and zoom regressions pass on the same build. All source
and executed-binary guards remain unchanged.

Every replay's live Kestrel audit checks 62 reservations and 21,885,504 routes
without conflicts. Live and pinned source both hash to
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
The Sequence, Hold and zoom reports retain three, two and three warnings for
layout retries with distinct causes and screenshot capacity. Ignored-layout
retries and repeated-same-cause failures remain hard harness failures; neither
occurs. These are not latency measurements.

Root inspected Sequence 077 (Camera), 107 (before the cut), 114 (after the cut)
and 121 (final committed edit). Camera names the Sequence and target, with
separate eighty-frame Edit and 120-frame Original clocks. Seam images show
Original counters 39 and 80 while the inspector retains the complete group
scale range and two children. The final view keeps the whole Sequence selected
with readable keys. No overlapping labels or obscured controls were observed.
This is offscreen native Metal, not a physical display/input qualification.

The actual saved project exports eighty pictures at 30000/1001, 320×180. Every
picture passes the committed shared picture-path comparison. There are 152
observable and six unobservable neighboring-picture comparisons; unique index
observability is not claimed for every counter picture. The complete
`[0,128128)` stereo sample window at 48 kHz passes with zero uncovered samples,
zero offset and no physical gaps. The emitted audio edit retains media time
2,048 samples and no empty edit. No event-based alignment or encoder-block
tolerance conceals an offset. The reference is the canonical limited bus,
not an independent DSP oracle or listening check.

The emitted 117,671-byte MP4 hashes to
`f58bac4dd4e4dec609af135aa0958578ebe3bcb618cd0e089c1d2f086dc45611`.
All nine CLI commands pass and the portable authored document is exactly equal.
Independent SQLite inspection confirms equality of every selected immutable
revision, patch, history, receipt, redo, state and transaction-step table,
including twenty-six revisions, fifteen history rows and twenty-six
receipt-verified revisions. The frozen raw-result audit has no findings;
receipt SHA-256 is
`1287a23f69e7003a5a815d12f05d4311bd3677f69c88eca16a39249e2073e0a4`.
The initial Plan-basis audit stop is preserved byte-for-byte as five files,
including its preservation manifest.

## Evidence and remaining scope

Base is `fea6b081775165f9830ba14d938576d346fec6c8`; scratch is
`/tmp/deadpan-sequence-follow-creep-2026-10-24`. Fourteen source paths define
the final qualification scope. The raw-result audit is retained. Earlier
Source, Hold and point-target evidence stays unchanged.

The immutable [Primary evidence](../../tools/media-qualification/evidence/2026-10-24-sequence-follow-creep)
has 134 payloads, 8,457,096 bytes plus its inventory. Inventory SHA-256 is
`dac42373c6119a3baf5cd98869cf91beb01b66d676ab9a2763bdf7885bc8e7a9`.
Five archives retain fourteen first-source members, six worker-source members,
five first-audit members, thirty final project/export members and thirteen
actual native-project members. All 68 regular members total 4,626,097 bytes;
the omission maps are empty. Both authoritative SQLite databases and complete
authored history are retained. The collection charges 114,876,734 raw payload
bytes against the 256 MiB bound.

The first independent package verification passes with no findings. It checks
all 131 encoding rows, all 68 regular TAR members and the complete raw charge
with zero metadata or authored omissions. Primary bytes, the final raw audit,
the preserved first attempt and all fourteen current scoped source paths
remain unchanged. The frozen receipt
SHA-256 is
`34bb11df126ab428d71270ab876dd6ea255177ee9b1e2af5a89e5e66df0fb7df`.
The separate [verification supplement](../../tools/media-qualification/evidence/2026-10-24-sequence-follow-creep-verification)
retains eight payloads, 36,761 bytes plus its inventory
`c5de16121ad364cc734f499499ca60a29722b57c2b701c5a68708ea724128836`.
Root's complete inventory and every payload hash pass readback.
Authored diff checks pass. The complete staged check reports seven trailing
blank lines in retained logs and the raw verifier; those immutable bytes stay
exactly as qualified.

This slice establishes no physical input/display, OS IME delivery, VoiceOver
speech, listening, detector quality or latency target. Repeat/Retime/occurrence
following-creep authoring, general non-face proposals and the remaining full
normative scope stay open. Coverage remains 89%, eight Complete and sixteen
Partial. No requirement or gate closes.
