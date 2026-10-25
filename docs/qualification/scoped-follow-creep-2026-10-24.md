# Scoped whole-owner following creep, 2026-10-24

Whole Source, Hold, Sequence, Repeat and Retime owners now retain a fixed live
Follow when Keep creep is authored through valid explicit Repeat-play scopes.
Focused Core/CLI/Plan checks and three independent reviews pass. Current
formatting, App all-target check and strict Clippy pass after a retained
test-only lint correction. Full Core/Plan passes 2,059 tests with zero ignored.
Full App passes 1,304 tests with eleven existing ignored. The release build
passes with all twenty-two source paths unchanged. Native replays and image
review, actual emitted-media checks, final independent raw/history audit and
immutable package verification pass. DP-08 remains Partial.

Base `fb38159bae82f2c08e7dfd96f112c24d6571b8e2`; working records are under
`/tmp/deadpan-scoped-follow-creep-2026-10-24`. The integration manifest lists
twenty-two source paths. Native fixture assertions below now have an actual
passing execution.

The slice and directory retain the 2026-10-24 date; current post-model-switch
execution occurred on 2026-10-25 UTC. The filename is not an execution date.

## Behavior and authority

The internal Rust marker is now `WholeFollowOwner`, and `Shot.whole_owner`
replaces `ordinary_owner`. The five actual owner kinds are unchanged. This
marker rename changes no serialized authority, payload, raw framing/schema,
curve, numeric, clock or isolation algorithm. Core 58 / SQLite 87 and
dependencies remain unchanged.

Ordinary authoring keeps its selection and direct-parent guards. Scoped
authoring reuses unchanged `validate_scoped_context`: Your edit, no Visual or
duration selection, captured root directly under its ordinary Sequence parent,
every enclosing Repeat explicitly addressed as Play, exact target/presentation
agreement, and target containment in that root. Nested all-Play scopes and
supported Retime traversal are admitted; descendant kinds remain unrestricted.
Default, missing/stale/mismatched authority, out-of-root and Original contexts
refuse. A full-owner Visual selection remains a range; an animated Follow/path
still requires explicit replacement.

After validation, the planner resolves current owner duration and fixed Follow,
classifies the actual target and uses existing `EditScoped` preparation,
allocation, target/instance mapping and staged re-resolution. Camera requires
the command's captured scoped authority and exact current scope match;
ordinary Camera admission remains direct-child-only. Scale uses the complete
selected owner output clock, while the center uses the emitted provider's
exact Original SourcePoint, including fractional coordinates. Live/fallback
bases, target identity, timing/pitch/audio, child framing, captured geometry,
other plays and gap recipes remain intact.

Earlier Hold/Sequence/Repeat/Retime explicit-Play negatives now prove positive
authoring, clone/address and render/audio preservation, and exact Undo.
Default, missing and forged-context refusals remain. A containing Repeat or
Retime grants no implicit authority.

## Observed checks and retained failures

Focused counts overlap and are not additional broad-suite totals.

| Check | Observed result |
| --- | --- |
| Core Follow builder filter | Three passed. |
| Core scoped semantic filter | Fifty-nine passed; one overlaps the builder filter, giving sixty-one unique Core cases. |
| CLI framing integrations | Sixteen passed in the final run; not the full CLI suite. |
| CLI live-Follow resolver unit case | One passed. |
| Plan scoped focused tests | Two passed after lint correction, zero ignored, 136 filtered. |
| Formatting, App all-target check and strict Clippy after correction | Exit 0, all twenty-two listed sources unchanged during each run. |
| Independent general, authority/clock and test-oracle reviews | No findings; reviewers ran no runtime checks. |
| Full Core/Plan | 2,059 passed, zero failed/ignored across 100 result summaries; exit 0 in 300.500 seconds, all twenty-two source guards unchanged. |
| Full App | 1,304 passed, zero failed, eleven existing opt-in cases ignored; exit 0 in 236.4356 seconds, all twenty-two source guards unchanged. |
| Ordinary command-reference validation | commands_reference_is_current passes in the full App suite after the intentional writer run. |
| Release | Exit 0 in 283.7249 seconds, all twenty-two source paths unchanged. |
| Actual native replay | Scoped 155 checks plus Kestrel pass; same-candidate Repeat134/Retime134/zoom213/Sequence101/Hold67 each plus Kestrel pass. All source/binary guards unchanged. |
| Visual review | Four actual native images inspected; controls visible, long Camera explanation reaches its pane clip. |
| Emitted media | Nine CLI commands passed; 240 pictures/full 384,384-sample stereo window, zero offset/uncovered/gaps. |
| Complete portable history/raw-result audit | Final fourth execution exits 0/no_findings; all seven selected tables exactly equal and pinned/current source inputs unchanged. |
| Immutable evidence verification | Final third two-package execution exits 0/no findings; both original inventories, current sources and raw/history evidence unchanged. Earlier reader stop and passing retry are retained. |

The first CLI integration attempt retained eleven passes/five failures caused
by a proof-helper assumption that a private leaf revision equals the outer
revision. The second retained fifteen passes/one failure at an ancestor
Follow's legitimate one-Q32-unit reprojection. Both fixes affect only the
shared test proof helper. Production admission and clock semantics did not
change; the final sixteen integrations pass. Exact pre-run source copies and
raw streams remain separate for every attempt.

The first strict Clippy run exited 101 for `clippy::collapsible_if` in the new
Plan test. Original streams, exact old module/registration and original
handoff/freeze remain in `plan-pre-clippy-fix-*` records. A conditional
let-chain fixes only that lint; assertions, fixture values and production
behavior are unchanged. Both focused Plan tests reran and passed in
`plan-focused-2.json`; final App check, format and strict Clippy also pass.
The earlier Plan pass remains historical, not a final-source pass.

`commands-write` intentionally exits 101 after rewriting `docs/COMMANDS.md`.
Its actual writer panic and streams remain retained. This expected writer exit
is distinct from the lint failure; ordinary reference validation subsequently
passes in the full App suite.

Worker checks retain argv/environment, timestamps, terminal results, binary
observations and source maps. Core/CLI's twelve owned paths, nine named
dependencies and all twenty-two listed paths stayed unchanged during its
runs. These bounded maps are not an all-product source snapshot. Plan's final
two owned paths, complete Core/Plan production source basis and observed named
twenty-two-path manifest stayed unchanged during its corrected focused run.
Unexecuted preparation snapshots, including the pre-Lost-continuation draft,
are not failed runtime attempts.

Earlier focused Core/CLI snapshots are historical inputs, not final-source
snapshots: the shared CLI test proof helper changed afterward. Their actual
production/named basis remained stable. Final broad runs bind the complete
current twenty-two-path scope. Within-run source stability does not mean
every earlier snapshot equals that final scope.

The App unit executable was observed live at 2026-10-25 06:44:08.738240 UTC and
after terminal completion at 2026-10-25 06:48:09.991876 UTC, with matching 245,292,400-byte
SHA256 e51e3136b2c818c690e223e5e3297e2b7d03b6a13e8f9930a31adcb099788197.
These two byte observations are not a pre-run capture or archived executable.
They do not establish immutable bytes continuously throughout execution.

Fresh host.json observations at 2026-10-25 06:39:40.176363 UTC retain
Mac17,7, 128 GiB memory, eighteen CPU cores, macOS 26.5.2 and hypervisor flag 0.
Power, thermal and cache state are uncontrolled.

The first native candidate manifest binds twenty-two source paths and five
executables. App is 78,435,184 bytes, SHA256
ee1b8280b4a1be5c176a3c0d9114bd68d9f404183a5a9dd4a63483f39d5b8de6;
CLI is 50,045,040 bytes, SHA256
46407b9bb0d6ae377ebd0980429b281408b9c22394821b6815dd32ecd62c9e02.
Actual native-scoped-first passes on this candidate. First/final manifests
bind identical twenty-two source paths and all five executable bytes in
candidate-equivalence.json. This is one candidate and one scoped workflow,
not duplicate first/final native executions.

## Pure and real-project proof

Builder tests cover all five markers, separate live/fallback bases, retained
entry clocks, OwnerOutput construction, explicit-from and full/interior range,
missing-marker and animated-path refusals. The semantic case authors all five
actual kinds below two explicit Plays. It proves allocation counts, unchanged
templates/copied marks, mapped target/presentation validity, independent
live/fallback values, selected-branch-only transition, Compound replay, JSON
round-trip and exact inverse. Reapplying animation and invalid scope variants
refuse before allocation.

CLI places each six-frame owner under nested Plays with a nonunit Tape Retime
ancestor. The inner twelve-frame body maps to eighteen output frames; outer
Repeat plus two-frame Background gap totals thirty-eight, followed by the
retained 120-frame suffix. Outer Play2/inner Play2 selects global `[29,38)`,
whose nine pictures sample owner coordinates `(2k+1)/3`. Retime as owner adds
its own 2/3 provider map. Tracked/lost/corrected targets, Sequence live clips
and Background fallback, Hold captured clips, and Repeat frozen gap/separate
escalation remain distinct.

Independent Q32 half-even and affine/clipping oracles check all nine views of
each owner. A shared helper reads the actual Compound from durable SQL history,
proves every clone/address and retained template before comparing all picture
fields and complete audio spans. Ancestor Follows legitimately reprojecting
through the edited owner use independent composition checks inside selected
pictures; outside views remain strict. No identity, timing, gain or envelope
field is silently dropped.

Named dry-run leaves complete SQL state unchanged and predicts commit.
Checkpoint/reopen/durable Undo/Redo restore exact documents/registers.
Staged Keep smash plus named replay maps the isolated target and re-resolves a
fresh concrete invocation; inline Apply exercises the shared dot boundary.
The resolver case retains Source's 120-frame owner clock beneath Retime and an
explicit Repeat play after linked Original deletion. Video-only registration
proves measured index and silence audio-plan semantics, not emitted pixels/PCM.

Plan uses one fifty-frame tree for whole Retime and whole inner Repeat edits,
each with point and region targets: four cases, 200 picture samples and
fifty-two independently recomputed selected-owner samples. Expected clone IDs
derive from the authored override tree independently of the isolation map:
ten fresh nodes for nested Retime, seven for inner Repeat. InstancePaths,
stable IterationIds and complete AudioSpan fields are explicitly mapped and
compared.

Retime's exact source ticks are `1001/3,1001,5005/3,7007/3,3003,11011/3`;
decoded ordinals separately are `0,1,1,2,3,3`. The target interpolates, holds
during Lost without interpolating toward a future detection, takes a manual
correction and excludes an out-of-span point. Source crop followed by zoom-out
cannot resurrect a clipped target. Repeat's retained clock and inner
escalation stay separate; default gaps use fallback while owner scale advances.
No production evaluation, target lookup, projection or quantization function
constructs expected values.

## Observed native witness and image review

The actual scoped replay passes 155 checks plus Kestrel in 14.068 seconds,
with all twenty-two source and executed-App guards unchanged. Production commands retain Original `[0,24)`, author
point .58 corrected to .68 at indexed Original 12, wrap at 2/3 Tape speed into
thirty-six frames and author fixed live Follow 1.1. Three plays/four-frame gaps
give 116; outer two-play Repeat/eight-frame gap gives 240. Plan assertions retain
384,384 canonical stereo 48k samples. Emitted audio verification also passes.

Explicit outer Play2/inner Play2 selects whole Retime global `[164,200)`.
Provider coordinate is `(2j+1)/3`, correction boundary local 18.
Independent integer half-even oracles assert all 240 pictures, 216 live frames,
144 fractional provider points, both stable Play IDs, gap capture/ownership,
scale/center and absent frame 240. Actual clone/override alias ownership is
proved before semantic picture/audio comparisons; original templates stay exact.

Passing assertions cover complete Undo/removal of isolation, exact Redo IDs,
Camera Enter/Escape preservation, explicit Static release plus Undo, named/dot
re-resolution on fresh concrete Plays, stale captured-command refusal, actual
Default combinations and native Visual-entry refusal. Final cursor 182 and
stopped boundary presentation checks pass.
Earlier Repeat/Retime replays now contain positive explicit-Play preservation
and Undo cases; both regressions pass. Repeat134, Retime134, zoom213,
Sequence101 and Hold67 each pass with Kestrel on the same candidate. Every
source/binary guard is unchanged. All retain three warnings except Hold's two.
Each report retains two explicit exclusions: only the OS picker's selected path
is scripted; physical display presentation, VoiceOver, OS IME and audio device
output are unverified. No failed scoped checks are hidden by those exclusions.

Root inspected Camera121, before-correction122, after-correction123 and
final124, pinned in visual-review.json at 2026-10-25 06:54:37.241203 UTC.
Camera shows both concrete Play selections, target and 1.1-to-2.2 animation.
Correction captures show Original11-to-12 at Edit181-to-182; final workspace
retains Original120/Edit240 clocks, scope breadcrumbs, Retime inspector and
focus cues. Interactive controls are visible without overlaps. Long Camera
explanation reaches the right pane clip; its entire text is not claimed to
fit at once. No product change followed this review.

The first media driver stopped before any CLI command because it incorrectly
required no declared exclusions in the native report. Its exact failure is
retained. Handling the two explicit owner exclusions changes the driver only;
there is no product or native rerun.

The second driver stopped before rendering, after one successful read-only
dump. Its oracle incorrectly expected fallback .58. Smash Follow intentionally
retains the unframed entry pose as fallback: actual fixed template and animated
clone both retain (.5,.5). Exact driver/source/streams remain preserved before
correcting the oracle. These two driver assertion failures are not failed
media renders. Product bytes and the native run remain unchanged. The final
emitted-media result is recorded below.

## Actual emitted media and portable history

media-third exits 0 in 8.4987 seconds, with all twenty-two source and binary
guards unchanged. All nine production CLI commands pass. The actual saved
native project exports all 240 pictures at 30000/1001 and 320 by 180, with the
complete [0,384384) stereo 48k window. No uncovered samples, nonzero offset or
audio gaps remain. The twenty compared signal blocks and seven offset
segments pass; the full window includes the retained silent gaps.

The 164,670-byte MP4 has SHA256
a3cce65faf3ab682ab5184fa20d0deb4f148ca5589f3b293810fe0f9eabe315d.
The verification report records 262 observable and 216 unobservable neighbor
comparisons, not unique index observability for every picture. AAC uses one
edit, no empty edit, media_time 2048; first PTS -2048, physical 387072 samples,
presented 384384. Explicit delay/padding metadata remains part of verification;
no event-based alignment, packet dropping or AAC-block tolerance is used.

The portable document is exactly equal. Native and portable history summaries
retain twenty-five revisions/fifteen history rows: one initial, fifteen edit,
seven Undo and two Redo revisions. History digest is
4dae063c100a608771634cfebcf9c9da1da1ae78a471ae3465c8ab0451087945.
Independent full raw/history audit passes as detailed below. Shared committed picture
and canonical audio references do not prove independent DSP or listening.

## Final raw audit and retained retry history

The independent raw auditor's first retained stop is
audit-history/first-historical-worker-scope-stop. Its initial equality rule
treated historical focused worker snapshots as final, although only the shared
CLI test proof helper differs and production/named basis is unchanged.
The second retained stop, second-q32-stage-oracle-stop, rounded once instead
of the documented separate Q32 endpoint, progress and interpolation stages.
Both corrections belong to offline audit assertions. Original scripts,
receipts and streams remain retained; no product/evidence input changed and
no product run was repeated. The third passing execution is retained under
audit-history/third-passing-before-final-integrity-guard before adding the
final evidence-integrity guard. Final success is the fourth audit execution,
not first-pass auditing.

The frozen final 322,370-byte receipt has SHA256
19a50d4712bb0d7c68e8be218cacd019c3678cf250e301723adaf391f3fc3ec8;
verifier SHA256
6359c61dd18cae220fe7034a4431d3fdf3e563d4876743dc5b24faf0c8f99772.
It exits 0/no_findings, verifying unchanged pinned evidence/current twenty-two
sources, candidate identities, media and native/portable table equality:
revisions25, history15, revision_patches9, transaction_steps10, receipt1,
state1, redo0. These are exact selected immutable/state/history tables, not
only matching current-document dumps.

## Immutable evidence and independent package verification

The immutable [Primary](../../tools/media-qualification/evidence/2026-10-24-scoped-follow-creep/files.json)
contains 218 payloads, 15,245,224 stored bytes plus its 32,406-byte inventory,
SHA256 b7858060082699ba66d5cf68e954414c4fb6a018ab6eb309b6dbf8f96bc10a6b.
Its 215 encoding rows include sixteen archives; raw payload charge is
202,592,698 bytes, below256MiB. The archives retain 774 regular members/
83,055,805 member bytes, including exact historical source and audit/media
attempts. Rust source evidence is retained as text, outside compiled modules.

There are four explicit metadata omissions: native and portable project
SQLite SHM32,768 bytes plus empty WAL0 bytes each, total65,536 bytes. Exact
hashes/reasons remain in archive-ledger.json; their paired WALs were observed
empty. No authored bytes are omitted. This is not a zero-omission package.

The separately published [regression collection](../../tools/media-qualification/evidence/2026-10-24-scoped-follow-creep-regressions/files.json) has
56 payloads, 6,111,277 bytes plus files.json inventory SHA256
0a980f41b3e92c3115c2699d4fcf33606cd89f30de69bd3c7fac8bd7ebcdf5d9.
It has 53 encoding rows, no archives and 109,235,808 raw bytes, below256MiB.
Root's complete hash/name readback passes for both collections in
root-package-readback.json. Full verbatim JSON/HTML native
reports are split across collections to retain all six pairs within each
256MiB collection budget. Four reviewed PNGs belong beside the scoped report
under native-scoped-first; other intermediate PNGs are not included.

The first independent package verifier stops at SQLite readback after passing
inventory, encoding, TAR, native/test and PNG checks. SQLite deserialize into
:memory: cannot open the archived WAL-format checkpoint's journal. Exact
script/receipt/streams are preserved under package-audit-history. A retry
reads only extracted byte-pinned checkpointed databases using
mode=ro&immutable=1, bound to actual empty-WAL omissions and checkpoint
receipts. Package, product and collector bytes are unchanged; no collector or
product run repeats. The second passing execution remains preserved under
package-audit-history/passing-retry-before-checkpoint-binding. Final success
is the third verifier execution, with checkpoint binding included. It must
not be called first-pass verification.

Final two-package verification exits 0/no findings, frozen 19,867-byte receipt
SHA256 eae0eb895d7552cd821cf71bada8488e07e5d98e1b80a0969f63e81c56f53aae;
script SHA256 bf3fc552768a10331588ab5216d28bd4472e182e01a1e944a64156b1d3192379.
It verifies 274 payloads/276 files including inventories, 21,356,501 stored
payload bytes, 268 encodings (218 identity/34 gzip/16 TAR+gzip), fifty gzip
streams and 312,449,885 decoded ledger bytes across two separately bounded
collections. Sixteen TARs retain 774 members/83,055,805 member bytes. The four
metadata omissions total 65,536 bytes; zero authored omissions. Both immutable
inventories, current twenty-two sources and raw audit/history inputs remain
unchanged.

The separate [verification supplement](../../tools/media-qualification/evidence/2026-10-24-scoped-follow-creep-verification/files.json)
has twenty-one payloads/142,949 bytes plus inventory SHA256
62e518ae1f8557a5e09c5ff21cf73c0163db0d73a6c134928c2208fe631cb609.
It retains final verifier/script/streams, prior package-audit attempts,
collector receipts and Root's package readback without modifying either
original collection.

Root's final complete file-name/hash readback passes for all three package
trees, retained in all-three-package-readback.json. Staged authored code/docs
checks pass. Full cached diff checking reports exactly ten immutable
blank-at-EOF warnings: commands-write.stdout, header.rs.txt, setup.rs.txt,
plan-focused-1.stdout, plan-focused-2.stdout, worker-cli-2.stdout,
worker-cli-3.stdout, worker-cli-unit-1.stdout, worker-core-1.stdout and
worker-core-scoped-1.stdout. These are retained raw evidence bytes, not
authored source/doc defects. Do not trim them.

## Remaining qualification and scope

Qualification is locally verified with immutable evidence. Both pre-render
media driver stops remain
preserved separately from the successful final media run. No product, model
or native rerun occurred to correct those driver assertions.

Coverage remains 89%, eight Complete and sixteen Partial; Section8 effects
coverage remains five of thirteen. No requirement or gate closes. Ranged live
following-creep authoring, general non-face proposals, remaining captured
acceptance and the full normative scope stay open. Physical input/display,
IME/VoiceOver, listening and uncontrolled performance are not established by
these focused checks. Earlier immutable qualifications/evidence remain intact.
