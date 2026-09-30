# Deadpan — implementation-agent handoff

Read version 1.1 of `DEADPAN_SPEC.md` as the current normative full-product specification. The imported 1.0 package is preserved in `archive/1.0/` and does not override the revised single-original V1 policy. Designs and examples are not implementation evidence; keep actual progress and measured capability in the requirement tracker.

## Development format policy, 2026-09-30

The user confirmed that Deadpan has no users and will remain unused throughout
this goal and session. New work may break project formats and schemas without
migrations when that simplifies implementation. Prefer this permission over
historical compatibility requirements; it does not require deleting working
adapters or reduce eventual runtime/recovery requirements.

## Owner preparation, 2026-09-30

CLI retain/relink/register/checkpoint now use the existing native import worker
and exact owning session. Do not synthesize native selection, rebase a late
registration, or release a worker slot before its cancelled reply drains.
Operational receipts survive bounded output and refresh failures independently
of authored revision receipts. Checkpoint publication retains the actual pinned
read revision and reports any post-rename durability failure with the saved path.

[Qualification](../qualification/owner-preparation-2026-09-30.md) records 28 real
CLI invocations and native catalog/picture/keyboard inspection, with 2,572 passing
locked workspace tests, 338 optional UI-feature tests, strict lint, formatting
and native startup/shutdown. Initial failures and scoped corrections remain in the
evidence. Current schema migration remains an independent read-only no-op.
Native relink/checkpoint controls, full preparation performance/failure acceptance
and native persisted Render recovery remain open.

For the next native recovery step, reuse service `ProjectRenderOperation::Retry`
and `Reconcile`, the CLI `retry_request`/`reconcile_request` builders, and bounded
store job/attempt/publication queries. Add session/ticket-bound browsing and
captured historical targets before destination pickers; the backend workflow
already provides checkpoint retry, re-encoding and destination reconciliation.
No schema change is needed for that connection.

Native inspection also found a pre-existing cursor-preservation gap: returning
from a catalog sound to the same video calls `preview::select_source`, which
unconditionally resets `source_cursor` to zero. Fix and verify that focused
round trip before the larger Render recovery UI work. Sound selection itself
and the Edit cursor remained intact; do not reset a retained Original position
merely to reselect its existing source.

## Open-project routing, 2026-09-30

The native writer now advertises a private authenticated local endpoint for
structural edits, undo/redo, primary geometry and the public Render workflow.
Discovery binds the actual package and writable open; close/reopen revokes the
old owner. A request cannot silently fall back, retarget or replay after delivery.
Replies retain durable edit receipts separately from native UI updates. Remote
Render refuses unresolved Camera, Gain and Room tone drafts, retains its exact
workflow through later edits, and shares native cancellation and teardown.
See [the contract](../LIVE_PROJECT.md) for bounds and supported operations.

[Qualification](../qualification/live-project-2026-09-30.md) retains 36 CLI
invocations against the real native owner, three actual preview refusals,
concurrent editing during Render, exact cancellation and historical recovery.
Initial and retry movies pass all 768 independently decoded planes and complete
authored audio. The original workspace run and scoped corrections cover 2,523
distinct passing tests; 330 optional UI-feature tests, strict Clippy, formatting,
native startup/shutdown, 79 visual Render checks and 2,426 release checks pass.
No native status window appeared for CLI-started work; the short recovery finished
before native progress could be inspected. Preserve this visibility limitation
alongside the retained CLI progress/results and endpoint crash-test gaps.

Original retention, relinking, registration and checkpoints now prepare through
the service's shared import worker and complete through their exact owner.
Registration retains the caller's complete revision and insertion intent.
Checkpoints use a private consistent SQLite backup and retain the actual captured
revision. Operational receipts survive refresh/reply failures; a renamed
checkpoint with failed directory sync retains its receipt and reports the error.
Native persisted-job recovery remains the next product boundary.
Section 20.5, DP-21 and all product gates remain open.

## Native and public Render, 2026-09-30

Native Render (`Cmd-E`, `:render`, or the visible control) and the public
closed-project headless commands now use the shared automatic SDR workflow.
Camera, Gain and Room tone drafts offer explicit commit/discard/keep choices.
Input is captured after native text processing; picker cancellation preserves the
draft. A typed `CommitAndStart` uses the exact durable preview commit receipt,
including when later refresh or render admission fails. Status and cancellation
remain separate from editor feedback. See [the contract](../RENDER_JOBS.md#native-and-public-render)
and [CLI commands](../HEADLESS.md#automatic-render).

The production `render` replay passed 71 checks, including actual publication,
later Undo during rendering, stale preview rejection and minimum/default layouts.
The public workflow passed start, stored status, checkpoint retry, reconciliation,
stale rejection, destination collision, SIGINT cancellation and JSON re-encoding.
[Qualification](../qualification/public-render-2026-09-30.md) retains 2,463
passing workspace tests, 318 optional UI-feature tests, strict Clippy, formatting,
native startup/shutdown, eight inspected final captures and 2,418 release replay
checks. Independent readers pass all 768 picture planes and complete authored
audio in both public exports. The accepted-generated-picture replay requires its
separate fixture and is explicitly skipped in this ordinary release run.

The open-project boundary above extends these commands to the native owner.
Prepared owner operations are described above; native persisted-job recovery
remains required by Section 20.5. Full mastering, HDR, expanded output qualification and the complete
product scope remain open.

## Durable automatic encoding, 2026-09-30

Database 42 adds a strict automatic job policy and immutable decisions owned by
the original encoding attempt. Qualification runs while Queued; the writer
commits the exact decision and Encoding transition together before the worker
can consume its live admission. Store closure revokes probe and encode work.
Fresh encoding retries qualify again. Checkpoint retry and reconciliation retain
the original decision, controls and runtime while freshly verifying the movie.
Automatic manifests and publication provenance use schema 2; engineering schema
1 keeps its frozen shape. See [the contract](../RENDER_JOBS.md#automatic-admission-and-recovery).

Migration preserves legacy cells, rejects automatic vocabulary in old job and
nested publication intents, and creates the new decision table without masking
name collisions. Full audits reuse validated job heads and compact revision
summaries; targeted reads still check their own allocation head.

[Qualification](../qualification/automatic-render-jobs-2026-09-30.md) passes two
fresh native-app encodes, four verifications, reopen/retry and reconciliation.
Independent readers pass 768 picture planes and complete authored audio in both
files; every old cell in 19 tables survives. Final coverage is 2,439 workspace
tests plus 310 optional UI-harness app tests. Strict Clippy, formatting and native
smoke pass. The original full-run migration-expectation failure and a native
example's f32 JSON-comparison failure remain retained, with scoped corrections.

The entrypoints above now expose automatic Render with captured preview decisions
and owning-session validation. Keep full mastering/effects, HDR, scheduling and
release qualification open.

## Encoder runtime binding, 2026-09-30

The [automatic consumer](../AUTOMATIC_ENCODER_ADMISSION.md#consuming-a-fresh-admission)
now consumes one live admission for a committed project encode. Probe protocol 2
and report schema 2 carry actual mapped helper and Avcodec/Avformat/Avutil/Swscale
observations. Project protocol 3 requires the exact nullable runtime/control
binding. Matching descriptors stay open through work and are fully hashed and
revalidated before and after. Missing evidence or a runtime change stops the
attempt. This proves backing-object provenance under trusted installed code,
not resident-memory or OS/framework/driver attestation.

`EncodeContract::new_v1` freezes the existing controls, with `new` as its alias.
Historical manifests and database schema 41 are unchanged. The real qualification
consumer retained a verified 128-frame, 205,005-sample project file and fresh
canonical references; all 19 database tables remained unchanged. See
[qualification](../qualification/encoder-runtime-2026-09-30.md) for final checks,
actual hashes and limits. Independent readers pass all 384 planes and the full
authored audio. Final coverage is 2,397 passing tests after one protocol fixture
correction; strict Clippy, formatting and native smoke pass. Tiny probes report
192x96 coded geometry above their 256/256/4096-pixel budgets. Their admission
bounds remain unchanged.

The durable automatic boundary above completes this increment's next storage
step. Native/public headless Render remains the next product entrypoint.

## Automatic encoder probe, 2026-09-30

The [admission boundary](../AUTOMATIC_ENCODER_ADMISSION.md) generates deterministic
moving I420 and independent stereo markers at the requested raster/rate. Fresh
supervised probes advance only after specific admitted native failures. The
selected file passes complete structural/decode verification plus per-frame
pixel and exact event checks. Ordered rejected attempts and owned probe bytes
remain available; serialized reports cannot create a project output capability.

[Final evidence](../qualification/encoder-admission-2026-09-30.md): 2,384 workspace
tests, strict Clippy, formatting and native startup/shutdown pass. Four native
cases cover 190 frames and 24 independently decoded exact audio events. The
14x16, 16x16 and 64x64 probes explicitly fail the geometry guard. Existing bounds
already include macroblock padding; capture rejected dimensions/limits or SPS
before deciding whether those failures justify a decoder change.

Keep legacy final-file verification policy 1 unchanged. An absent-B verification
failure still stops this selector; safe typed absence requires finishing every
other check. Legitimate all-I/P project output needs a separately versioned rule.
That milestone identified helper bytes, kernel and native version observations.
The runtime-binding increment above adds loaded-library evidence and a fresh
project consumer.

The durable automatic boundary above now retains the policy and each fresh
encoding decision. Native Render and public headless access remain open.

## Typed encoder failures, 2026-09-30

[Encode protocol 2](../ENCODED_RENDER.md) preserves the failed boundary and exact
native kind separately from diagnostic text. Missing video encoders and actual
PTS-before-DTS packets have specific kinds. Source, DSP, I/O, capacity, control
and generic driver failures cannot become capability evidence through prose.
Later supervision faults invalidate a typed report; a validated failure terminal
permits exit 1, while other failing exits and malformed tails remain faults.
No automatic fallback is implemented and no stored policy/schema is upgraded.

[Native qualification](../qualification/encoded-failures-2026-09-30.md) reproduces
the real typed hardware-B rejection, then separately verifies a no-B project
encode without authored changes. The synthetic matrix preserves all ten working
paths and three rejected hardware-B attempts, with six deliberate fault cases.
All 2,359 locked workspace tests, strict Clippy, formatting and native smoke pass.

Next, implement bounded automatic admission using deterministic moving pictures
and audio at the actual output raster/rate for at least two GOPs. Arbitrary short
or frequently cut project content cannot establish B-frame support. Bind the
result to the current runtime and record rejected probes. Preserve strict legacy
engineering intents; automatic policy belongs to the job, while the selected
encoder and qualification evidence belong to each new encoding attempt.
Checkpoint verification/reconciliation must retain the original encoding decision.
Then expose the same policy through native Render and the public headless API,
with preview commit/discard decisions and requests bound to the owning session.

## Visual slice placement requirement, 2026-09-30

The user requested an elegant keyboard workflow for putting selected parts of
the video elsewhere while seeing the edit. [Section 9.7](DEADPAN_SPEC.md#97-visual-slice-placement)
now requires visible source endpoints, a provisional destination placement,
frame-accurate adjustment, audition around both joins and one-step commit/undo.
Insert, replacement and move retain exact timing, occurrence scope and owned
attachments. Enter commits; Escape cancels the unsaved proposal. Existing moment
copy/paste does not fulfill this requirement. DP-05/DP-20 acceptance must include
the complete keyboard flow and minimum-size visual inspection. Temporal slices
are the current interpretation.

## Durable destination reconciliation, 2026-09-30

The [publication journal](../RENDER_PUBLICATION.md#durable-publication-journal)
records Intent, Prepared, report authorization/commit and movie authorization.
Opaque exact-stage permits require a direct checked DB/WAL full-sync and namespace
barrier after COMMIT. Barrier failure leaves the row visible, revokes permits and
requires reopen. Media work and full hashing remain off the writer.

Reopen interrupts active operations and never changes destination entries.
Explicit reconciliation requires a newer same-checkpoint verification attempt,
its live candidate, APFS volume/birth/inode/path identities and full byte hashes.
A matching final movie with a missing/damaged report remains committed but
unconfirmed; a replacement with identical bytes stays unresolved. Keep recovered
file locks through the final journal transaction. Schema 41 preserves all prior
schema-40 cells and core 33; earlier migration adapters remain intact.

[Native qualification](../qualification/publication-recovery-2026-09-30.md)
passes 20 SIGKILL cases, 40 fresh verifications and exact authored/history
preservation. Independent FFmpeg and AVFoundation readers pass 138 pictures,
221,021 authored sample frames and 414 image planes. The full workspace passes
2,318 tests, strict all-target Clippy and formatting on the native-qualified source.
Kills occur between completed host calls; physical power loss is not qualified.
Native Render, public headless
rendering, automatic policy, scheduling, full audio/effects and HDR remain open.

The shared `encoded_render::workflow` coordinator now connects start,
cancellation, checkpoint retry and publication reconciliation. Heavy stages use
one bounded worker; the native project service owns journal transactions and
retains its writer through close/switch/shutdown. Explicit process cleanup
evidence gates terminal failure, and render feedback remains separate from
editor feedback. Public Render still needs real automatic policy admission and
product controls; current encoder choices remain explicitly engineering-only.

[Workflow qualification](../qualification/render-workflow-2026-09-30.md) passes
two complete encodes, a real encode cancellation, six fresh verifications,
reopened checkpoint retries and reconciliation. Edits and undo/redo run during
encoding without retargeting its captured revision. Independent readers pass
138 pictures, 221,021 authored sample frames and 414 complete planes. The final
workspace passes 2,347 tests and strict all-target Clippy. A reproduced native
shutdown notification failure is corrected and its regression passes.

## Durable encoded checkpoints, 2026-09-29

The [render job boundary](../RENDER_JOBS.md) persists immutable render intent,
fresh attempts and exact transition sequences outside authored history. An I/O
worker retains complete movie and strict manifest objects in a separate bounded
`Media/RenderCandidates` namespace. The SQLite writer checkpoints opaque session
and attempt tokens using cheap freshness checks.

Writer reopen interrupts nonterminal attempts and preserves their checkpoints.
An explicit new attempt freshly hashes both retained objects, reconstructs the
captured historical contract and invokes the isolated finished-file verifier.
Stored reports cannot create a live verified candidate. Closing the owning store
revokes retained handles; final verifier admission checks session liveness.
Schema 40 adds operational tables while preserving schema-39 authored JSON and
patches byte-for-byte; older schemas retain strict replay.

[Qualification](../qualification/render-jobs-2026-09-29.md) passes two real
restart/reverification cases and independent readers over 138 pictures, 221,021
authored sample frames and 414 complete planes. The workspace has 2,284 passing
tests after correcting and rerunning three failed targets; all other full-run
results are retained. Strict workspace Clippy and formatting pass. Evidence
includes authentic schema-39 databases, final schema-40 backups, actual movies,
readers, source differences and independent reviews. That checkpoint qualified
orderly writer restart; subsequent process-death coverage is described above.
Physical power-loss injection remains open.

The subsequent publication journal is described above. Next implement the complete
native Render and public headless workflow, using the qualified staged APIs. Scheduling,
automatic platform policy, full mastering/effects, HDR and all release gates
remain required. See the contract for ownership, limits and API sequencing.

## Verified destination publication, 2026-09-29

The library [publication host](../RENDER_PUBLICATION.md) now takes a private
verified candidate and an explicit MP4 destination. It pins the destination
parent, stages exclusive sibling partials, checks exact destination bytes and
publishes the local report before atomically renaming the movie without replacing
an existing entry. Before that rename, failure preserves the verified candidate
and diagnostic recovery paths. After it, bounded integrity and durability checks
finish despite late cancellation; a failure returns `PublishedUnconfirmed`.
The report and movie are separate commits, so an orphan report can remain.

Provenance binds the captured historical revision and full document hash. Source
receipts supply original object identities and SHA-256; the catalog is explicitly
a committed superset. Effective Generated intervals follow the indexed picture
resolver through sparse plays, gaps and retiming, carrying complete artifact and
immutable provenance identities. Source labels, URLs and linked paths are omitted.
Capacity limits fail explicitly and do not truncate the report.

[Qualification](../qualification/render-publication-2026-09-29.md) passes seven
fresh project publications and independent final-file decode: 304 pictures,
390,695 authored sample frames, 912 complete planes and 18 exact audio markers.
A separate audit checks report/movie hashes, historical SQLite document hashes,
Original receipt/byte identities and the exact `[0, 30)` Generated interval.
All 2,248 locked workspace tests pass, with zero failed or ignored, as do strict
workspace Clippy and formatting. Independent review corrected cancellation-code
loss and a post-rename content-check gap; both corrections have passing tests.
The evidence retains actual bytes, SQLite backups and exact source/binary identities.

Native Render, public headless render commands,
automatic platform policy, complete mastering/effects, HDR and release
qualification remain open. No DP requirement or Gate A through G is complete.

## Isolated finished-file verification, 2026-09-29

The [finished-file verifier](../FINISHED_FILE_VERIFICATION.md) now inspects a
private encoded candidate in a separate supervised process. It binds the bytes,
document and captured contract, checks actual MP4 tables and edit lists, decodes
every picture, and compares every complete GOP with a fresh decoder starting at
its actual IDR. Manual and ordinary AAC modes must cover the exact authored
sample interval and agree at fixed sample coordinates without PCM realignment.
Clean teardown admits the report; failure retains the candidate for retry.

[Current qualification](../qualification/finished-file-verification-2026-09-29.md)
passes seven fresh project MP4s through normal and instrumented production
verifiers: 304 pictures, 390,695 authored sample frames and 21 GOPs per run.
Independent FFmpeg/AVFoundation readers pass all 912 picture planes and 18
audio marker observations at exact fixed coordinates. The workspace passes
2,222 tests, Clippy and formatting. Native C ASan/UBSan passes 241 selected
tests and reinspection of all seven fresh files. The evidence records exact
source/binary identities and the instrumentation limits.

Source admission remains bounded to 64 GiB, one million packets, a 16 MiB header
and 16 MiB packets, with bounded aggregate tables. This is narrower than the
encoder's capacity and fails explicitly. Structural/decode admission does not
replace content, event-sync or hardware/runtime qualification. See the linked
contract for the supported SDR interpretation and remaining limits.

The library publication and retained-checkpoint boundaries are described above.
Next add durable publication recovery, native Render and public headless render commands. Complete mastering,
remaining audio/picture effects, HDR, release-runtime coverage and the full
specification remain required. DP-17 stays open; every DP requirement and Gate A
through G remains open or partial. No native export workflow is complete.

## Isolated committed SDR encoding, 2026-09-29

The [encoded render child](../ENCODED_RENDER.md) now streams one committed
revision's real pictures and canonical PCM directly into the native encoder.
Both readers bind the complete document hash and exact range before native
allocation. Strict separate messages, the shared process supervisor and contained
snapshot admission return private MP4 candidates after clean teardown.

[Qualification](../qualification/encoded-render-2026-09-29.md) retains seven
actual candidates, 304 pictures and 390,695 authored sample frames. Normal and
sanitized independent readers pass complete picture/PCM comparisons, exact
clocks/edit lists, 21 fresh GOP boundaries and all marker events. Real live
edit/undo/redo, cancellation after progress, byte exhaustion and retry pass.
Sanitizers cover the independent readers here; the native encoder has its
separate preceding qualification. The production structural/decode verifier is
now implemented in the newer boundary above.

The locked workspace passes 2,182 tests, zero failed/ignored; strict all-target
workspace Clippy, formatting and 123 Python tests pass. Independent review fixed
qualification bounds/SAR checks and strengthened progress-fault fixtures, whose
seven integration tests pass separately. Evidence retains the initial failures,
actual bytes, SQLite backups, source/binary bindings and review dispositions.
No app UI changed.

The [isolated verifier](../FINISHED_FILE_VERIFICATION.md) now reuses the
descriptor-only source decoders and bounded MP4/packet observations. Its source
capacity and content/runtime qualification limits remain explicit. Library
publication now exists. Next add durable publication recovery, native Render and public
headless render commands.
Full audio/effects, HDR and every DP requirement and Gate A through G remain in
scope and incomplete.

## Native SDR encoder and offline audio, 2026-09-29

The [native encoding boundary](../NATIVE_ENCODING.md) now writes bounded
descriptor-only H.264/AAC MP4 from composed I420 and finite canonical stereo PCM.
It enforces exact chronological inputs, one deadline, explicit encoder attempts,
poisoned failures, complete codec EOF and restricted same-file fast-start.
`OfflineAudioSession` captures one historical revision and B(end)-B(start),
preserves limiter/source context, and shares the deadline through source index
comparison and provenance hashing. The existing inspection API stays unchanged.

[Qualification](../qualification/native-encoding-2026-09-29.md) retains normal and
ASan/UBSan files, all three audio readers, exact MP4 edit-list observations and
fresh-decoder GOP suffixes. Each matrix passes ten usable cases and six deliberate
failure checks. Hardware B-frame attempts still fail PTS<DTS on the measured
M5 Max. Hardware without B-frames and explicit OS software attempts are separate
measured paths; software requested for two B-frames emits one. No fallback occurs
inside an encoder session.

All 2,159 locked workspace tests pass, with zero failures or ignored tests.
Strict workspace/all-target Clippy, formatting and 114 Python oracle tests pass.
The evidence archive retains and verifies all 836 native result files, including
rejected attempts and partial outputs. No app UI changed in this milestone.

The integration above now feeds committed pictures and canonical PCM directly
into this encoder inside the supervised child, followed by the separate
[finished-file verifier](../FINISHED_FILE_VERIFICATION.md). Library publication now
exists. Add durable publication recovery, native Render and public headless render
commands.
Do not promote a synthetic adapter fixture into product export evidence. Full
audio/effects, HDR, release hardware/OS coverage and all DP-01 through DP-24 and
Gates A through G remain open or partial.

## Isolated committed picture worker, 2026-09-29

The [render worker](../RENDER_WORKER.md) now owns real decode, Metal composition
and bounded raw I420 output in a separate process. Host and child independently
bind the complete authored document hash and every exact output-contract field.
The shared process supervisor retains checked launch, group/reaping ownership,
bounded pipes and clean-exit admission through separate generation/render
protocol adapters. A returned range owns a verified private snapshot.

[Qualification](../qualification/render-worker-2026-09-29.md) passes 2,140 locked
workspace tests, zero failed/ignored, strict workspace/all-target Clippy and
formatting. Actual Metal passes 118 direct and 128 worker checks, including all
82 isolated frames, 31 independent complete-plane comparisons, live writer
edit/undo/redo, cancellation after real progress and a successful new request.
The macOS raw/fixed pipe-inheritance witness also passes. Source inventory
`1c9362b0…` binds all final checks and native artifacts.

Independent review fixed diagnostic overwriting, interrupted-copy error mapping
and interrupted control-drain handling. Retained evidence includes these findings,
the initial compile failure, terminal command journals, exact worker/example
binary hashes and every synthetic actual/direct/reference plane. No app UI or
native interaction changed. No compiler, test or native probe is left running.

The native encoder and bounded offline audio reader now exist as separate
boundaries described above. The separate encoded child now connects them.
Do not spool a full uncompressed product movie through the current 512 MiB raw
qualification range. The existing conversion worker still owns whole-file FFV1
conversion; it is not the H.264/AAC encoder.

The [encoded-file verifier](../FINISHED_FILE_VERIFICATION.md) now provides the
bounded structural/decode checks described above. Library publication is also
described above. Durable publication recovery, full audio/effects, native Render,
HDR and all remaining product requirements stay open. No DP requirement or
Gate A through G is complete.

## Committed encoder pictures, 2026-09-29

The [encoder picture host](../EXPORT_PICTURES.md) owns one captured revision,
range, exact output clock, legal even raster and completed I420 frame. Framing
still uses the unchanged authored canvas. Source PTS and absolute project frames
remain separate from relative output timestamps; both absolute audio boundaries
are retained. It reuses the actual Original/Generated reader and shared Metal
composition/readback, with cancellation/deadline checks and bounded ownership.

[Qualification](../qualification/export-pictures-2026-09-29.md) passes 118 actual
Metal checks with 52 retained frames. All 30 Generated frames and one odd-canvas
Original match independent complete-plane references within one code value;
93,396,906 codes were compared. Exact nonzero-range clocks, held-result rejection,
captured framing and live writer edit/undo/redo/close are covered. Three independent
reviews found no actionable defect. No app UI or native interaction changed.

The full locked workspace passes 2,106 tests, zero failed/ignored. Strict
workspace/all-target Clippy and final formatting pass. One initial harness
SHA-256 formatting compile failure and its correction remain in the evidence.
Corrected checks and Metal use the same source inventory `66cc8615…`; only docs
and archived evidence changed afterward. No compiler, test or native probe is
left running. Keep these results unless a later code change affects them.

The later [encoded worker](../ENCODED_RENDER.md) now isolates this real producer
and canonical PCM behind separate render messages and checked supervision. The
[finished-file verifier](../FINISHED_FILE_VERIFICATION.md) adds independent
structural/decode admission. Library publication is described above. Complete the
shared audio/effects graphs, durable publication recovery and native Render. This
picture boundary alone produces no encoded file or export control. All DP
requirements and Gates A through G remain in scope and incomplete.

## Accepted Generated Hold pictures, 2026-09-29

The [shared picture reader](../PROJECT_PICTURES.md) now admits schema-3 Generated
Holds from retained project media in native preview and captured-revision
preparation. It checks all six objects, strict provenance, both asset records,
canonical decoding, every sampled PTS and measured terminal duration. Its
revocable package handle and one private decoder preserve accepted historical
pictures through request staleness, relocation and prefix resizing. It needs no
model, current candidate selection or worker path. Legacy Accepted without this
evidence, Still and HDR remain explicit failures.

The [qualification](../qualification/generated-pictures-2026-09-29.md) retains
real FFV1 conversion/acceptance, all 30 exact RGBA frames, damaged-object cases,
undo/redo/revert and independent review. The full locked workspace passes 2,094
tests; after app-only layout changes, base app passes 268 and optional harness
304. Strict Clippy and formatting pass. Generated visual replay passes 108 checks
plus audit; the ordinary visual run's room-tone failure is corrected by a
220-check scoped continuation. Final release replays pass all 2,348 ordinary
checks and 367 Generated checks plus its audit. Full source inventories bind
each continuation. No compiler/test/replay process remains running.

The Hold inspector now exposes Picture, Sound and duration action first. A
stronger paint-order check and image review caught hidden mode/focus text:
inactive Gain/Sounds panels retained decoration that let later panes cover the
footer. Empty panels now retain their IDs without decoration. The read-only
clock row uses compact height where needed; the copied-Original minimum picture
is 141 points. Room-tone scroll checks reveal the complete fact/action group.
Failed captures and checks remain in the evidence. The generic Generated
fixture's minimum picture remains small; full editor visual acceptance is open.

Only app layout and harness sources changed after that milestone's backend
gate, with scoped tests and release replay covering them. Command journals bind
terminal status and source inventories. Inspect live PIDs and journals after
interruptions before starting another Cargo process; quiet compilation does not
justify a restart. App model management, real candidate audition/acceptance,
analysis, remaining editing/effects, recovery and signed distribution remain
required. No DP requirement or gate is complete.

## Earlier completed evidence

The [SDR encoder timing experiment](../qualification/encoder-timing-2026-09-28.md)
finds native AAC events and the stream endpoint 1,024 samples late when edit
lists are disabled, exceeding one frame at 60 fps. Default-edit-list references
are sample-aligned. Retain the failed
files and both decoder modes; do not hide priming by shifting/cropping PCM or
widening tolerance. The [independent AVFoundation comparison](../qualification/native-audio-2026-09-28.md)
finds missing opening events and later events 1,088 samples early in the same
disabled-edit-list file; the default reference aligns. The user approved the
§22.3 revision on 2026-09-28, applied on 2026-09-29: edit lists may represent
encoder delay, padding and frame reordering, with explicit stream-start/sync
and full emitted-file verification. Do not reopen this decision. The newer native
encoding and finished-file boundaries above add measured GOP/video evidence and
production structural/decode checks; full product export remains open.

The [shared SDR encoder pixel boundary](../SDR_ENCODER_PIXELS.md) snapshots
the composed linear working target into bounded owned memory and converts it
to explicit Rec.709 limited-range, left-sited I420. Preserve its signed working
values until the output transform and keep output timestamps separate from
source PTS. Cancelled GPU work retains its permit until callbacks drain.
The synthetic video-only encoder experiment does not qualify AAC timing under
the approved mux policy; neither boundary supplies a project export worker.
Its [qualification](../qualification/sdr-encoder-pixels-2026-09-28.md) retains
the full workspace pass, 22 actual Metal checks and normal/sanitized H.264
pixel comparisons. Later boundaries above add final-render isolation and file
verification. Legacy Accepted/Still readers and the full export workflow remain open.

## Product in one paragraph

Build a native macOS, Rust-first, Vim-style instrument for massaging one original video into a weird YTP. A project chooses one local or YouTube video and starts with its full unedited timeline automatically. Cuts, repeats, pauses, reframing and effects remain reversible structures on that original. Reuse moments from the same video, add external audio-only effects, and explicitly accept local AI Hold extensions. Do not offer additional video imports. New native projects live in Documents/Deadpan regardless of launch or source location. The full product still includes analysis, recovery, actual local generation and one-action source-derived YouTube output without external end-user runtimes.

## Read first

[Sound catalog audition](../PLAYBACK.md#sound-catalog-audition) uses the shared
canonical playback service with its own sample clock and leaves the retained
picture and edit selection untouched. Follow the
[sound design board](../design/boards/sound-audition-board-v1.png) for focused
selection, visible Space/Shift+Space keys and explicit state. It does not place
sound events. The [sound integration contract](../SOUND_EVENTS.md) distinguishes
node-owned clocks, continuous per-voice processing and scoped Hold allowances.
Its exact route kernel and CLI LRU PCM cache support the persisted root subset;
nested ownership, the remaining edit transforms, voice effects and full final
mixing remain required.
The [retained sample-route evaluator](../SOUND_EVENTS.md#retained-sample-routes)
now composes each edit's physical-grid cut and new anchor. Keep the old selected
audible mask separate from complete recipe/filter/DSP support. Current Hold
queries retain issuer identity; they grant no allowance and invent no historical
policy. Playback also uses bounded LRU source eviction. Complete per-voice
processing remains required. See
[sound-clock qualification](../qualification/sound-clocks-2026-09-27.md).
The [independent source operand](../SOUND_EVENTS.md#independent-catalog-source-operands)
now feeds qualified catalog audio through the existing tape and PCM engine without
adding a Source node. Its input retains complete DSP context; its output separately
applies current scoped silent-Hold rules. Preserve still requires the checked
owner/descendant relationship. Do not inherit Original bindings or use metadata
as permission to bypass host admission.
The [routed PCM readers](../SOUND_EVENTS.md#routed-pcm-preparation) connect retained
sample routes to complete source or projected providers on checked PointCeil and
RoundEven clocks. They read the old sample labels with full filter/DSP support,
including cold suffixes, and admit dependencies even for entirely masked output.
Captured provider policy stays separate from current consuming Hold gates. See
[routed-voice qualification](../qualification/routed-voices-2026-09-27.md).
Core 29/database 35 added qualified persisted root events and the shared
pre-master bus. Core 30/database 36 introduced chronological root sound routes
through InsertTime, SpliceSource and ordinary Sequence Delete. Non-root Split
is neutral; root Split and temporal occurrence edits remain guarded. Preserve
the complete recipe and old physical sample labels while current Hold gates
stay live. Parameter changes keep routing; explicit ReplaceSound discards it.
Frozen core 29 checks historical contextual admission before modern replay.
See [persisted root ripple edits](../SOUND_EVENTS.md#persisted-root-ripple-edits).
The [native root placement subset](../SOUND_EVENTS.md#native-root-placement) now
places the complete measured catalog sound with `,s` or `:sound-place` at the
retained Edit cursor. A separate Placed sounds pane owns event selection and
`j/k`, durable exact-frame `h/l` nudges, Enter for a whole 48 kHz sample onset,
3 dB `+` / `-` steps, soft/hard endpoints and `dd`. Edits use normal durable
history; picture duration never grows and overflow is rejected. Parameter entry
captures event/session/revision, including rejection when no event was captured.
Routed gain and edge edits retain their journal; native move/nudge rejects it.
Follow the [placement board](../design/boards/sound-placement-board-v2.png) and
the [native placement qualification record](../qualification/native-sound-placement-2026-09-27.md)
for the implemented subset and its verification limits. Nested ownership,
Repeat/Retime sound transforms, send/tail allowances, effects, the remaining
structural transforms and export remain required. No requirement or gate is
complete because these controls exist.

Core 31/database 37 add explicit per-sound, per-concrete-Hold allowances.
`:sound-allow` and `:sound-silence` capture the selected event, Edit frame,
issuer, session and revision. The writer re-resolves that scope and rechecks
source admission. Granting one sound leaves Original audio, other sounds and
other pauses suppressed. Raw preparation precedes current contribution gates;
an allowance cannot create sound in a retained route gap. Split and occurrence
isolation remap exact identities; database-36 histories replay through frozen
core 30 and gain no permission. See [sound allowances](../SOUND_EVENTS.md#persisted-root-sound-allowances).

Core 32/database 38 add [atomic Hold audio authoring](../ROOM_TONE_AUDIO.md).
`SetHoldAudio` and its occurrence form retain picture, duration and sample clocks,
reconcile changed audio lineage, and remove only that Hold's obsolete silence
permissions. Undo restores both policy and permissions. The store requires
revision-bound qualification and measured sample endpoints for new source
choices; unrelated legacy Hold recipes remain unchanged. Database-37 history
uses frozen core 31 with exact allowance comparison. The
[room-tone design board](../design/boards/room-tone-board-v2.png) now guides
`:room-tone` and `:hold-silence` for ordinary selected Holds. Copy Original time,
inspect inward-snapped source samples, audition on a separate audio-only clock,
then explicitly Apply. Native fields preserve IME and button/key ownership.
Reopen the saved range; replacing it from the captured copy is explicit.
Preparation successes and failures carry request/session/revision identities.
Waveforms, occurrence controls and acoustic qualification remain open.

[Gain contracts](../AUDIO_GAIN.md) add persisted node treatments, direct and
occurrence setters, exact owner clocks and canonical post-mapping PCM gain.
Core 33/database 39 replay database 38 through the closed core-32 adapter.
Context schema 4 retains a sparse treatment map separately from timing-only
`FrozenAudioLayout`. CLI `inspect-audio --authored-bus` exposes the pre-limiter
result. Native `+`/`-`, `:gain <dB>` and `:gain-mute` capture a selected ordinary
beat; Placed sounds retain their separate gain target. `:gain` opens a buffered
editor for trim, mute, exact owner-output envelopes and mute ranges, with a
single explicit Apply. Use the [gain board](../design/boards/clip-gain-board-v2.png)
for the panel hierarchy and retained picture.
Keep gain after complete time/pitch mapping and edges, with exact independent
owner clocks and unchanged continuous Preserve history. Root-owned sounds receive
only their own gain and root treatments, never an unrelated Source's gain.
Temporary drafts retain explicit content identity and the same delivered sample
window; a matching base revision alone cannot authorize cache/resume reuse.
Writer previews produce validated, unstored documents while media admission
remains anchored to the committed entry snapshot. Apply rechecks the captured
target and commits once; cancellation retains the accepted picture and restores
entry context only in the same session and revision.
Follow the [native gain integration design](../GAIN_EDITOR_DESIGN.md) for the
proposal/admission boundary and production-router verification.
See the [native-gain qualification](../qualification/native-gain-2026-09-28.md)
for actual verification, including native macOS command/focus/text/cancellation
and the separate 2,156-check release replay. The warm picture and 10,000-beat
CPU measurements retain their small-fixture/offscreen limits.
Waveform editing, physical keyboard/IME, VoiceOver, listening, complete audio processing
and DP-09 acceptance remain open.

The [measured beat overview](../WAVEFORMS.md) is implemented with scoped
[qualification](../qualification/gain-waveform-2026-09-28.md). Preserve the captured
committed owner independently of gain proposals, complete signed min/max bins,
unknown coverage and exact terminal owner clipping. Analysis uses the same
preparation owner as playback and waits for controller-confirmed output
quiescence. Retained peaks keep their memory reservations; a new request admits
source evidence afresh. Stale errors cannot disable a valid gain edit, and Retry
remains in the native modal keyboard circuit.
Wide layouts pair exact fields with the editable curve below the overview;
minimum-size layouts retain one scroller and fixed comparison/commit actions.
The image review caught and corrected fields scrolling away from their curve.
The final full release replay passes 2,348 checks across 18 scenarios; retain
the qualification's small-fixture and offscreen measurement boundaries.

The [compact workspace qualification](../qualification/compact-workspace-2026-09-28.md)
records empty Sounds consolidation into the Beats heading in short
single-Original edit views. Preserve its distinct focus target, measured label
width, scrolling breadcrumbs and empty panel ID. Capture placement once per
render pass and reject changed placement before picture submission. Room tone
is an overlay over the same layout; Gain/Camera and populated sound lists retain
their existing layouts. The final source passes app/painted checks within that
record's scope; release performance is recorded separately.

[Structural speed editing](../RETIME_EDITING.md) exposes `:retime` and
`:wrap-retime` through exact speed resolution and the native inspector. Preserve
the input range when adjusting an ordinary Retime; wrap split Partitions instead.
Only a changed stage's own retained output binding resets. Descendant source and
DSP input bindings remain intact. Core 28/database 34 close the old command
vocabulary through `legacy_v27`; old histories cannot gain new speed operations.

The current [Original/edit audition contract](../PLAYBACK.md) describes the native
Space Play/Pause and Shift+Space selection-loop increment, exact paused sample
retention and the limited edge-faded bus. Whole Original playback includes its
full A/V union; selected moments use measured picture endpoints before adding
audition context. It does not qualify the full mastered preview/export pipeline or
reduce the requirements below.

[Authored framing and Camera](../FRAMING.md) records the current implementation
contract and remaining work, including saved targets and tracking.
[Captured framing](../CAPTURED_FRAMING.md) preserves a pause's input composition
separately from its provider and live Camera operations. Use the dedicated [Camera design board](../design/boards/camera-framing-board-v1.png)
alongside the primary workspace target. A temporary preview must remain distinct
from a committed edit, and opening Camera must preserve existing curves.

[Exact Original moments](../SOURCE_MOMENTS.md) records measured range candidates
and selected audio placements, including physical-grid endpoint behavior and the
core-20/database-26 migration boundary. Core-27/database-33 add atomic
`SpliceSource` at an explicit ordinary Sequence slot. Native `v`/`y` selects and
copies an Original range; `p`/`P` pastes after/before the selected beat. Keep the
copied session/asset/receipt identity and captured revision/scope through
preparation. Admit the prepared existing receipt and Original freshness in the
same history/relevance transaction. Do not use separate Split/Insert commits.
The new Source begins unbound on the canonical project grid while each old
suffix owner retains its sample entry. Frozen core 26 preserves DB32 nested
pause history and rejects the new command. Persistent registers, arbitrary
occurrence/cursor splice and Visual replacement remain required. The
`original-moment` harness exercises the keyboard path when Metal is available.

[Compact audio reanchors](../AUDIO_REANCHORS.md) adds core-21/database-27 ordered
per-occurrence resume steps and retained allocation queries. Distinguish hidden
allocation from meaningful raw support and preserve each step's lexical scope.
Core-22/database-28 [gap bindings](../GAP_AUDIO_BINDINGS.md) extend this ownership
to configured Repeat gaps, including gaps with no current occurrence. General
atomic moment splice remains required.

Core-23/database-29 [editable gap branches](../REPEAT_GAP_BRANCHES.md) retain
independent subtrees after stable plays and materialize current default gaps
without changing their audio clocks. Final-play branches stay dormant until a
following play exists. Use these owned structures in the general splice author;
the primitive commands do not yet resolve an arbitrary cursor insertion.

Core-24/database-30 [pause insertion](../INSERT_TIME.md) admits existing root
Sequence seams before composite suffixes. Capture current placements without
replacing retained lattices; append windowed steps and stop at nonunity Preserve
outputs. Every older replay checks the frozen contextual command boundary before
modern apply. Core-25/database-31 add root Source/ordinary Hold fragment interiors
before composite suffixes. Capture sampling before Split and placements afterward
under separate timing identities; preserve each occurrence's own rounded entry.
Frozen core 24 admits its old seams but refuses these new interiors. Arbitrary
nested splice and general Visual replacement remain required. The native editing
replay includes both pause-before-Repeat and interior-pause/undo paths.

Core-26/database-32 extend that command into unretimed Sequence groups. Use
`insert_time_target` for the exact native capture parent and required Split IDs.
Capture only scopes below that parent; keep every ancestor live. Reanchor later
siblings at each Sequence level, stopping at physical Preserve outputs. Frozen
core 25 refuses this broader context when replaying database 31. Native edit
completion carries the captured cursor. Native [Sequence navigation](../GROUP_NAVIGATION.md)
adds Enter/Backspace, breadcrumbs and direct-child editing at each ordinary
Sequence depth. A deeper Hold selects its visible enclosing group until entered.
The `nested-pause` harness exercises insertion, navigation, duration, history and
Camera when Metal is available. This does
not implement fractional clocks or insertion under Repeat/Retime ancestors.

Use `AnchorIndex::locate_boundary` for exact project-to-content descent. The
[headless query](../HEADLESS.md) exposes all owner clocks, Sequence slots, stable
Repeat identities, distinct play/gap entries and implicit-gap terminals under
shared work bounds. This query performs no mutation.
The [splice design](../STRUCTURAL_SPLICE_DESIGN.md#exact-boundary-descent) records
the required ownership and resume work that remains. Do not use picture-center
sampling or flatten a Repeat out of its live group to choose a splice target.

The preferred [derived-clock representation](../STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks)
keeps authored integer durations separate from exact effective extents, including
nested Retime output. It is not yet a document capability. The shared framing
evaluator now accepts exact derived extents without rounding or overflowing
intermediate quotients; existing integer callers use the same implementation.
Do not implement subtree duration dilation as an audio-preserving shortcut:
Preserve processing depends on its physical input grid and retained history.

The borrowed [audio input tape](../AUDIO_INPUT_TAPES.md) projects current scoped
signals onto one intrinsic PointCeil grid and reads actual PCM through the shared
`StageAudio` path. Allocation seams never restart phase or crop filter support.
Checked `AudioStageProjection` views retain full intrinsic input/output history;
parent tapes consume child intrinsic output, while a separate PointCeil schedule
can place pauses around it. A request-local identity memo prevents duplicate
preparation and descriptor aliasing.
[Projected root placement](../AUDIO_PROJECTED_ROOT.md) separately allocates one
physical projection on the absolute RoundEven grid, preserving phase, exact
policy and support exhaustion through crops and repeated resumes. These are
borrowed evaluation views, not authored splice routes: exact effective owner
clocks, persistent lifecycle, aggregate scheduling and normal root-plan
integration remain required.

Use the [UI feedback loop](../UI_FEEDBACK.md) for every meaningful interaction
change: replay production keyboard, pointer, wheel and text paths, inspect actual
Metal captures, and run the separate release latency checks. The
[interaction review](../INTERACTION_REVIEW.md) records measured friction and
priorities. Reserve Kestrel's global shortcuts; whole-Original reuse is `,i`,
while Cmd+Return belongs to Kestrel. Update routing, visible hints, help and replay
together. A passing replay does not establish native IME, VoiceOver or physical
display behavior.

Read Sections 1–8 for product/primitive/keyboard semantics, 12–14 for AI contracts and qualification, 17–22 for rendering/runtime/storage/export, and 23–30 for dependencies, tests, requirements, and build gates. Section 31 resolves command targeting and source-browser behavior. Source references are in Section 33.

## Decisions already made

- Working name: Deadpan; `.deadpan` project directory packages.
- One pinned Original per new native project, with a full-source starting timeline and protected undo baseline. SQLite retains this workflow identity independently of reversible presentation state.
- A global system Documents/Deadpan project library; initial source-picker cancellation creates nothing and failed preparation remains explicitly recoverable.
- Original/moments plus a separate sound-effects collection. Existing generic backend and legacy multi-video projects remain valid in an explicit compatibility workspace; never discard their data to fit the new UI.
- Visible keycaps, pending-prefix guidance and a distinct focused-pane cue teach ordinary actions. Searchable contextual help supplements the interface.
- Native Rust UI with egui/eframe/wgpu on Metal; no browser shell.
- New domain core, not a whole-app fork of a general editor.
- Pinned FFmpeg/native media adapter; qualify Cutlass components only if extraction reduces complexity.
- Exact rational frame/source timing and 48 kHz sample coordinates.
- Source/Sequence/Hold/Repeat/Retime nodes with stable IDs, anchors, occurrences, and attachments.
- Same rendering and DSP semantics in preview and export.
- SQLite authoritative storage; no competing mutable JSON document.
- AI worker language/backend chosen for measured usable-output latency, not MLX loyalty or Rust purity.
- Immediate committed freeze fallback; actual generated candidates require explicit acceptance.
- Bundle yt-dlp, its JavaScript/EJS support, and all executable runtimes; model weights install in-app or from an approved offline pack.
- Full V1 scope includes every revised DP-01 through DP-24 requirement. The single-original policy is intentional; ordered gates do not excuse missing required creative operations.

## First concrete work

Create a dependency/architecture decision log and the workspace boundaries in Section 24. Run Gate A technical harnesses for actual macOS media decode/seek/encode, GPU preview, audio DSP/output, model inference, and private-runtime packaging. Record exact revisions, licenses, true output behavior, and measured hardware results. Do not spend the first implementation pass decorating a timeline while leaving timing, generation, and export unqualified.

In parallel, implement the pure core with generated fixture documents and property tests. Establish command resolution, exact duration math, reversible transactions, serialization, and render-plan inspection before connecting widgets to mutable state.

## Completion discipline

Playback tests reserve real-media work before fixture preparation and retain
the permit through the shared engine callback until both workers exit. Use the
existing test helper for new PCM scenarios; `Engine::drop` and `Stopped` alone
do not establish teardown. The
[scheduling record](../qualification/playback-waits-2026-09-27.md) retains the
reproduced preparation timeouts and the scope of this test-only correction.

Maintain a requirement tracker mapping DP IDs to implementation, tests, and evidence. Every operation must be editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A UI button, mock worker, successful model download, or ignored test does not count as implementation.

If a dependency fails qualification, preserve the product contract and replace the implementation behind its interface. In particular, slow or unreliable video generation is a measured engineering issue: do not silently redefine AI holds as freeze frames or require a manually installed external application.

## Non-negotiable correctness points

`3riw` means three total plays. Repeat gaps occur only between plays. A Hold inserts exactly N project frames and resumes untouched original speech. Do not accumulate fractional-rate duration rounding. Jobs cannot overwrite newer edits. Accepted generated media remains usable without the model. Export snapshots cannot mix revisions. Cache cleanup cannot remove referenced originals or accepted artifacts.

Undo cannot erase the original identity or cross its initialization baseline.
Deleting all current beats does not make another video eligible. Importing sound
does not imply placing it, lengthening the edit or replacing original speech;
sound-event overlay requires its actual authored and mixing path. Do not relabel
a generic blank-picture audio beat as a placed effect. Maintain core structural
capability and strict legacy migration while enforcing V1 through the optional
profile and native workflow.

## Delivery

Deliver the complete source, signed/notarized application distribution, approved model-pack manifests, clean-machine online/offline test evidence, benchmark report, keyboard guide, fixture/verification reports, migration policy, and third-party notices/SBOM. Any unfulfilled required behavior remains explicitly open rather than being described as finished.
