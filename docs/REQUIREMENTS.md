# Requirements and delivery gates

All DP-01 through DP-24 requirements in [specification Section 29](spec/DEADPAN_SPEC.md#29-requirements-traceability) remain in scope. Their detailed sections are normative. This tracker records the current implementation and measured evidence, not a reduced release scope.

Specification 1.1 applies them to [one Original](SINGLE_ORIGINAL.md): the full
video is the initial edit, native projects live in Documents/Deadpan, same-video
moments can be reused, and external media contributes sound only. Accepted AI
extensions remain in scope. Generic backend and legacy multi-video projects are
preserved. [Current design targets](design/README.md) replace the earlier
multi-video workspace; concept screens do not establish completed capabilities.

**Open** means required behavior has no qualifying implementation. **Partial** identifies concrete groundwork while acceptance remains unmet. **Complete** requires linked code, passing relevant tests, and a demonstrable acceptance result. No requirement or delivery gate is complete. Baseline checks are described in [Development](DEVELOPMENT.md); they are not substitutes for full acceptance evidence.

[Sequence audition](PLAYBACK.md) connects immutable qualified originals and
canonical pre-master PCM to the native device, with Space Play/Pause, a monitor
level, exact audio-clock picture scheduling and explicit interruption. Bounded
preparation runs separately from device control. Full mastering, Original-view
playback, acoustic synchronization, long-source/performance qualification and
preview/export equivalence remain required. This increment does not complete a
requirement or gate; [qualification](qualification/playback-2026-09-24.md) records
its native, headless and review evidence. Older evidence below describes its own
historical boundary.

[Master gain research](qualification/audio-limiter-gain-search-2026-09-24.md)
retains independently audited finite-fixture solutions, the corrected unwanted
muting failure, and a longer stress failure in a faster finite-context design.
Neither candidate is adopted. The [mastering boundary](AUDIO_MASTERING.md)
records the required final-output, dynamics and bounded-seek qualification;
production limiting, listening and encoded output remain open. No requirement
or gate changes status.

[Pause insertion](INSERT_TIME.md) adds atomic root Source/Hold splices with
sample-preserving resume for every shifted fragment, including earlier splits
and repeated pauses. Native `,h`, counts and exact `:hold` duration entry resolve
a measured freeze frame, retain the Original baseline and select the committed
pause. Core 17/database 23 freeze prior command history. Arbitrary nested scope,
compact Repeat-gap ownership, active generation context resolution, playback and
export remain required. [Qualification](qualification/insert-time-2026-09-24.md)
records exact timing, native interaction and actual baseline migration evidence.
No requirement or gate changes status.

[Logical mark bindings](MARK_FRAGMENTS.md) preserve one named mark across physical
bindings, with atomic loss/copy transforms, biased Partition seam visibility and
exact named resolution. Core 13/database 19 introduced frozen mark grammars.
[Qualification](qualification/mark-fragments-2026-09-23.md)
records review, migration and test evidence for this layer.

[Structural Split](STRUCTURAL_SPLIT.md) now cuts explicit beats and isolated
occurrences without changing duration or output, retains full processing contexts,
and refines repeated cuts without increasing wrapper depth. Native `s`/`:split`
captures an interior root-beat boundary and selects the committed right fragment.
Core 14/database 20 freeze prior history, including multi-binding marks.
[Qualification](qualification/structural-split-2026-09-23.md) records parity,
history and native review. Arbitrary Hold insertion, shifted-fragment sample
resume, minimal nested range planning and the full editing workflow remain open.

[Audio sampling clocks](AUDIO_SAMPLING.md) now separate allocation grids, exact
sample maps and retained envelope progress in all source/stage readers. Contract
tests exercise fractional-rate resume composition and envelope exhaustion without
changing filter support or rate. These are derived plan values, not persisted
Hold edits. Authored reference domains, genuine seam fades and arbitrary-boundary
insertion remain open.
[Qualification](qualification/audio-sampling-2026-09-23.md) records 985 passing
tests, the full gate and the reviewed wide-envelope compatibility fix.

[Frozen audio reference layouts](AUDIO_REFERENCE.md) retain old timing, compact
play identity, exact Source placement and Hold/gap policy without live sibling
dependencies or media ownership. Explicit root/Preserve clocks query old
audibility, and a bounded root-resume consumer combines it with current silence.
Actual DSP regressions cover both one-sample NTSC suppression mismatches.
This is a capture/query/PCM foundation, not an authored insertion or persistence
change. [Qualification](qualification/audio-reference-2026-09-23.md) records
1,011 passing tests, the full gate and independent review; schema versions
remain unchanged.

[Sampled-root transfer](AUDIO_SIGNAL_TRANSFER.md) converts admitted raw root PCM
onto an explicit point grid, retaining old silent samples before interpolation
and exact output suppression afterwards. StageAudio keeps full preparation
contexts and shares work, provenance checks and one deadline across all halo
reads. Creative fades remain after time mapping. Tests include real decoder and
canonical DSP paths. [Qualification](qualification/audio-transfer-2026-09-23.md)
records 1,021 passing tests, the full gate and independent review. Authored live-to-frozen bindings, policy replacement,
compact Repeat lifecycle, atomic Hold insertion and application playback remain
open. This conversion introduced no authored-state change and did not complete a gate.

[Physical reference-domain lookup](AUDIO_REFERENCE.md#physical-processing-domains-and-root-maps)
now retains opaque Preserve and meaningful context independently of visible
Partition allocation. Borrowed root maps compose the active cut and independently
anchor later domains. Tests cover NTSC phase, compact repeats, clock ownership,
real canonical DSP and fractional transfer. This does not author an insertion:
persisted sample bindings and their live edit lifecycle remain required. [Qualification](qualification/audio-domains-2026-09-23.md)
records 1,026 passing tests, the full gate and independent review; no requirement
or gate changes status.

[Authored audio copy lineage](AUDIO_LINEAGE.md) now survives Split, occurrence
isolation and durable history. Raw audio changes detach affected contexts and
ancestors while unrelated copies retain their relationships. Frozen reference
plans compare explicit lineage separately from physical identity and require
compatible clocks, phase and stable occurrence paths. Core 15/database 21 migrate
actual old copy history without inferring lineage from an initial snapshot.
[Qualification](qualification/audio-lineage-2026-09-23.md) records lifecycle,
strict migration and bounded reference tests. Live sample bindings, policy
replacement, atomic Hold insertion and application playback remain open.

[Retained audio contexts](AUDIO_CONTEXT.md) now capture complete raw audio trees,
exact media inputs and immutable asset contracts. Direct audio-only compilation
retains Source absence, RoomTone, nested Preserve, edges and compact Repeat
structure. The headless host authenticates each context against its exact
historical revision before verified media reads, including after undo and alias
reuse. This adds the old signal body; authored live bindings, phase composition,
policy replacement and atomic Hold insertion remain open. Core 15/database 21
are unchanged. [Qualification](qualification/audio-context-2026-09-23.md) records
the decoder/DSP parity, strict ingress and host-admission evidence.

[Physical audio domains](AUDIO_PHYSICAL_DOMAINS.md) now render retained Source,
RoomTone and Preserve context outside visible Partitions on the original signed
root grid. Both processing and policy queries stay inside the captured physical
subtree. Point-grid transfer preserves fractional phase and old silence through
one shared preparation allowance. Headless inspection reads actual qualified
historical media. [Qualification](qualification/audio-physical-domain-2026-09-23.md)
records hidden-sibling, negative-coordinate, decoder/DSP and admission tests.
Authored bindings, retained evaluation ownership, lifecycle/policy replacement
and atomic Hold insertion remain open; no requirement or gate changes status.

[Authored audio definitions](AUDIO_DEFINITIONS.md) now expose the actual Repeat
default independently of audible occurrences, with scoped point-grid queries,
real source/DSP rendering and historical headless admission. This resolves the
all-overridden-default counterexample for a future new-play operand.
[Qualification](qualification/audio-definition-2026-09-23.md) records exact
grid, silence, cache and media tests. Authored birth rules, binding lifecycle
and Hold insertion remain open; no requirement or gate changes status.

[Owned recipe clocks](OWNED_AUDIO_CLOCKS.md) evaluate current physical definitions
in explicit signed root placements through the shared reader. Source alignment
and silence-policy edits affect that revision's raw recipe; explicit historical
reads retain their old meaning. This supplies the evaluation boundary for the
proposed owned-tree binding model, without a second frozen raw-body graph.
Persisted anchors, compact Repeat birth/survivor rules, command lifecycle and
atomic Hold insertion remain open. No requirement or gate changes status.

[Owned-clock qualification](qualification/owned-audio-clock-2026-09-23.md)
records 11 new tests, the 1,108-test full gate and two independent reviews for
this evaluation boundary.

[Owned audio bindings](OWNED_AUDIO_BINDINGS.md) add the core 16/database 22
representation for timing lattices, stable Repeat scope, explicit births and
bounded symbolic phase. Split/isolation copy live arguments while retaining old
aliases; removal and inverse patches retain atomic ownership. PointCeil owned
evaluation preserves selected origins and nested preparation budgets. InsertTime
now captures bindings for supported root splices. Normal StageAudio rendering
now consumes root/point bindings, current and retained-placement policies, exact
resume phase and virtual post-mapping fades. A pure capture helper retains
compact birth scope and existing bindings, rejecting nonempty Repeat gaps.
Context-schema-1 capture and source-only SequenceAudio still reject nonempty
bindings. Full command lifecycle and arbitrary Hold insertion remain open.
No requirement or gate changes status.

[Binding qualification](qualification/owned-audio-bindings-2026-09-23.md)
records 1,140 passing tests, the complete repository gate and three independent
reviews for this representation and evaluation boundary.

[Consumer qualification](qualification/owned-audio-consumer-2026-09-23.md)
records 1,172 passing tests and the full gate for retained PCM,
post-mapping fades, compact capture, independent grid policies and shared
preparation admission. Review fixed tiny-clip fade changes, crop leakage,
dense policy inventory rejection and unintended muting of Preserve decay.
That qualification covers the engine milestone. The subsequent
[pause insertion](INSERT_TIME.md) command connects it to native authoring.

Current measured evidence: [editing foundation verification](FOUNDATION_VERIFICATION.md), [picture plan and migration verification](PLAN_MIGRATION_VERIFICATION.md), [exact boundary verification](ANCHOR_VERIFICATION.md), [persistent mark verification](MARK_VERIFICATION.md), [sparse override verification](OVERRIDE_VERIFICATION.md), [nested occurrence verification](OCCURRENCE_VERIFICATION.md), [native media qualification](qualification/media-2026-09-20.md), [compatible FFmpeg/Rust qualification](qualification/media-compatible-2026-09-20.md), [audio candidate qualification](qualification/audio-2026-09-20.md), and [canonical audio qualification](qualification/audio-canonical-2026-09-20.md).

The [native host conversion qualification](qualification/media-host-conversion-2026-09-21.md)
adds actual sampled/native model-output conversion, exact RGB comparisons,
bounded failure tests, and adapter sanitizer evidence. Complete media/provenance bundles are now implemented through
[host qualification and Ready storage](GENERATION_BUNDLES.md).
Schema 9 adds [explicit durable acceptance](GENERATION_ACCEPTANCE.md), measured
source spans and six-object admission. Application acceptance remains open.
The [bridge sampling qualification](qualification/media-bridge-2026-09-21.md)
adds paired native/sampled masters from one immutable input, exact integer
interpolation, and comparison with the earlier captured-model pixels. This
validates media derivation; it does not qualify candidate acceptance or seams.
The [native bundle qualification](qualification/model-bundle-2026-09-21.md) adds
a real protocol-2 local generation, required provenance binding, exact host-derived
masters, three-object publication/readback, and schema-8 Ready persistence.
That earlier run did not exercise authored acceptance or application integration.
Host qualification also retains the exact context manifest and both prepared
input byte streams before generation, binds them to worker provenance, and
returns all six objects for durable publication. [Retained conditioning evidence](qualification/conditioning-2026-09-21.md)
records a fresh local model run and relocated six-object readback. This is byte provenance, not
source-clock, image-decoding, color or seam qualification. Admission-bearing Ready
and acceptance now verify all six objects. A captured real generation has been
accepted, relocated, undone/redone/reverted and independently decoded in a
[developer probe](qualification/acceptance-2026-09-21.md). History reference
inventory and application rendering remain open.

[Source preview qualification](qualification/source-preview-2026-09-21.md) adds
persistent source decoding, identity-verified measured indexes, a shared SDR GPU
baseline and a native preview with frame navigation. Actual Metal comparisons,
source/session sanitizer tests and native visual/keyboard observations are
recorded separately. Project import, editorial playback and export remain open.

[Video admission qualification](qualification/video-admission-2026-09-21.md)
adds closed MP4/Matroska checks before demux allocation, FFV1 configuration
expansion limits, controlled first-frame probing and per-packet bounds. It
preserves the measured source fixtures and an accepted generated master. The
full required format matrix and authored import remain open.

[Original ownership qualification](qualification/original-media-2026-09-21.md)
adds managed complete-file retention, APFS clone/copy, linked locations,
identity-checked relinking, private snapshots and schema-10 migration. A bounded
native audio inventory reports observed metadata; that earlier ownership change
did not establish decoded audio bounds or complete authored import.

[Source audio](SOURCE_AUDIO.md) now provides a separate persistent AAC/PCM
decoder, identity-bound measured sample indexes and bounded private PCM range
reads. Video/audio share one verified original snapshot. Explicit skip/discard
and measured terminal-duration evidence remain distinct from unknown priming and
container claims. [Qualification](qualification/source-audio-2026-09-21.md)
records exact PCM and AAC fixtures. The [independent audio mapping](SOURCE_AUDIO_MAPPING.md) represents exact natural-rate
durations and offsets, with reversible commands and schema-11 history migration.
[Mapping qualification](qualification/audio-mapping-2026-09-21.md) records exact
boundaries, headless history behavior and old-binary fixture reproduction.
[Exact source picture mapping](SOURCE_VIDEO_MAPPING.md) now separates original
video speed from rounded beat duration, retaining selected-span endpoint policy
in picture plans and exact inverse anchors. Reversible mapping commands and
schema-12 migration preserve prior audio and picture decisions.
[Picture-mapping qualification](qualification/video-mapping-2026-09-21.md) records
VFR boundary cases, actual decoded pixels and old-binary migration evidence.
Authored import, resampling, playback, device output and full format qualification remain open.

[Measured import timing](SOURCE_IMPORT_TIMING.md) adds exact independent stream
starts, common-origin candidates, complete available-audio preservation, outward
source enclosure and measured cadence/geometry candidates. Core schema 8 and
database schema 13 retain earlier mappings through frozen history replay.
[Import-timing qualification](qualification/import-timing-2026-09-21.md) records
actual media and migration evidence. These candidates do not establish import
readiness or adopt a project's presentation basis.

[Source registration](SOURCE_REGISTRATION.md) adds live selected-stream
qualification, durable measured indexes and revision-bound receipts, plus atomic
asset registration and optional full-source insertion through the headless host.
Core schema 9/database schema 14 preserve older histories without inventing source
evidence. [Qualification](qualification/source-registration-2026-09-21.md) records
actual-media, rollback, relocation, historical alias reuse and migration checks.
The native workspace connects registration and explicit whole-source insertion;
complete import/relink/format acceptance remains open.

[Automatic presentation basis](PRESENTATION_BASIS.md) selects measured cadence
and geometry on the first primary picture insertion in an untimed project.
Audio or other timed editing locks the existing clock; later primary geometry
adoption is a separate previewable, undoable transaction that preserves timing.
Core schema 15/database schema 21 retain this policy and migrate older histories
as explicit choices. [Qualification](qualification/presentation-basis-2026-09-21.md)
records real-media insertion, unchanged sample/mark coordinates, rollback,
headless previews and authentic schema-14 history migration. Native canvas
previews, framing-effect reevaluation and rendered-output checks remain open.

[Background import preparation](IMPORT_PREPARATION.md) separates complete-file
verification and source receipt preparation from the project writer. Opaque
results are tied to a live writer session; final namespace/version checks reject
changed originals. Current-revision insertion and atomic persistence retain the
existing command path. The native workspace now connects bounded import preparation; measured large-media
latency remains open. The [qualification report](qualification/import-preparation-2026-09-21.md)
records the headless concurrency, freshness and rollback evidence.

[Native project workspace](NATIVE_WORKSPACE.md) connects creation/reopen,
managed/linked import, explicit whole-source insertion, saved history and exact
Source/Sequence frame inspection. Root-beat Repeat wrapping/setters, deletion and
existing Hold-duration edits now share the service/core/store command path.
The [root editing qualification](qualification/native-editing-2026-09-23.md)
records durable edit/history, selection, concurrent import and native keyboard
checks, including pointer/text-focus regressions.
The [initial qualification report](qualification/native-workspace-2026-09-21.md)
records actual media, deterministic service interleavings, headless keyboard/focus
checks and native panel/picture observations. Full editing, playback, generated
provider preview, export and release acceptance remain open.

[Single-original initialization](SINGLE_ORIGINAL.md) adds the optional schema-17
profile, full measured baseline and protected undo floor, with strict historical
validation. The native host creates these projects in the system Documents
library and separates Original identity from audio-only catalog registration.
[Profile tests](../crates/deadpan-store/tests/single_source.rs) exercise baseline,
history, rollback, immutable Original and forged-profile rejection. Native
[service tests](../crates/deadpan-app/src/project/tests.rs) cover creation, sound
selection, recovery and backed-up opening of authentic schema-16 projects.
The [single-Original review](qualification/single-original-2026-09-23.md) records
the actual native workflow, imagegen comparison and verification limits.
Generic projects migrate without invented profiles. Full SFX placement/mixing,
range reuse, local YouTube acquisition and the remaining editorial session are
still required; audio-only sequential insertion is not a sound overlay.

[Transparent partitions](AUDIO_PARTITIONS.md) add a retained audio-context
building block for splices. Core 12/database 18 separate allocation, exact source
filter support and full envelope width; ordinary authored trims retain their
old meaning. Real PCM tests compare retained partitions against the original,
including fractional rates, short fades, nested pitch stages and RoomTone.
The [qualification report](qualification/audio-partitions-2026-09-23.md) records
tests and review for that layer. The later Split implementation uses these
retained contexts. InsertTime now applies exact resume anchors to supported root
Source/Hold splices; arbitrary nested insertion remains open.

[Preview presentation](qualification/preview-presentation-2026-09-21.md) separates
requested, decoded and displayed identities, retains pending GPU work, rejects
late replies and labels the submitted Source/Sequence position. Headless
interleavings, real decoded freeze frames and native keyboard/appearance checks
cover this boundary. Continuous playback scheduling, full accessibility, physical
display timing and forced GPU-loss qualification remain open.

[Structural audio plans](AUDIO_PLAN.md) now expose bounded sample intervals with
original coordinates and distinct authored policies. The [native DSP boundary](AUDIO_DSP.md)
binds the canonical stretch schedule to owned Rust PCM. [Qualification](qualification/audio-foundation-2026-09-21.md)
records independent interval references, 50 prior PCM hash matches, native ABI
sanitizers and the 648-test repository gate. [Source preparation](AUDIO_PREPARATION.md)
now adds exact-phase resampling, explicit stereo speaker mixing and full-index
verification of original PCM sessions. Its [qualification](qualification/audio-preparation-2026-09-21.md)
records real PCM/AAC tests, signal checks and measured worker cost.
[Plan-driven source PCM](SOURCE_STAGE_AUDIO.md) now connects immutable spans to
historical source receipts and original bytes, including silent Holds, repeats,
fractional placements and FollowSpeed retimes. Its
[qualification](qualification/sequence-audio-2026-09-21.md) records real CLI/app
headless parity and cold-reader identity after undo and asset-alias reuse. The
[exact-rate DSP extension](qualification/audio-exact-rate-2026-09-21.md) now keeps
consumption speed independent of rounded storage/output counts.
[Continuous stage preparation](AUDIO_STAGE_PREPARATION.md) connects that adapter
through seams, nested retimes, exact fractional grids and immutable source
provenance. Its [qualification](qualification/audio-stages-2026-09-21.md) covers
PCM parity, Hold policies, cache changes and shared work admission.
[Room-tone preparation](ROOM_TONE_AUDIO.md) now loops explicit retained source
ranges through exact short crossfades, including retimes and repeat gaps. Full
voice processing, native audio playback, effects and export remain open.
[Prepared output](AUDIO_OUTPUT.md) adds a bounded generation-aware queue and a
narrow macOS device boundary, with explicit prepare/activate, starvation and
fault states. Its [hardware qualification](qualification/audio-output-2026-09-21.md)
is separate from application playback, full device/lifecycle coverage and
acoustic or stress acceptance.

[Informational audio measurement](AUDIO_METERING.md) adds shared integrated
loudness and true-peak meters, generated standard cases and an independent
[reference qualification](qualification/audio-metering-2026-09-21.md). Meters
preserve PCM; the failed limiter prototypes remain evidence rather than a
production mastering implementation. Limiting, reduction reporting and final
mix integration remain open.

[Worker cleanup verification](qualification/worker-cleanup-2026-09-21.md) records
a failed macOS CI run and the shared membership-confirmed teardown for media
and model workers. Deterministic signal/ownership/pipe regressions and actual
forking descendants cover these fixes; process-group cleanup is not a sandbox.
[Conditioned limiter research](qualification/audio-limiter-masks-2026-09-21.md)
retains a 42-fixture pass and two subsequent suppression-mask counterexamples.
The prototype remains unadopted; this does not advance mastering acceptance.
[Post-mask gain research](qualification/audio-limiter-postmask-2026-09-22.md)
retains 16 further outputs and a complete-sinc audit. Two distinct rapid-mask
outputs still fail the broader reconstruction check despite passing both finite
meters. Fixed fades that excessively quiet tiny fragments are rejected as a
default; production limiting remains open. The shared edge stage described
below uses shortened fades with an explicit tiny-fragment contract.

[Audio boundary provenance](AUDIO_PLAN.md) now retains exact structural,
placement and Repeat-gap edge owners, including coincident constraints and each
owner's own occurrence path. Root query crops preserve this metadata, and copy
work is bounded. [Authored edge policies and shared fades](AUDIO_EDGES.md) add
reversible node/occurrence commands, strict core-11/database-16 migration, and
sample-centered 2 ms fades shortened within each root allocation after continuous
time mapping. The explicit headless stage preserves silence and read partition
invariance. Native controls, listening qualification, simultaneous voices,
downstream effects/mastering, playback and export remain open.

## Product requirements

| ID | Requirement | Status | Implementation / tests now | Required acceptance evidence still outstanding |
| --- | --- | --- | --- | --- |
| DP-01 | Documents library, one-Original initialization/baseline, reopen, autosave, undo/redo, migration, recovery. | Partial | [`deadpan-store`](../crates/deadpan-store/): durable packages/history, atomic mark transforms and generation relevance, writer ownership, WAL checkpoints, interrupted-attempt recovery, and [schema-1-through-22-to-23 migration tests](../crates/deadpan-store/tests/migration.rs) using old-binary-validated fixtures with requests, attempts, admission, source placements, branches and redo. | Native create/open/history now have [workspace evidence](qualification/native-workspace-2026-09-21.md); full media lifecycle, restore/recovery UI, history limits and full failure/chaos suite remain open. |
| DP-02 | Exact frame/sample/source-time model including VFR. | Partial | Typed rational clocks, VFR intervals, [independent picture mappings](SOURCE_VIDEO_MAPPING.md) and explicit selected-span endpoints in core and plan. [`SourceSession`](../crates/deadpan-media/src/source_session.rs) builds original-PTS indexes from private verified media and performs persistent exact seeks. [Registration](SOURCE_REGISTRATION.md) retains validated indexes and exact common origin by historical revision. [Native source evidence](qualification/source-preview-2026-09-21.md) retains measured VFR terminal-duration loss. | Complete source policies and actual shared playback/export, including 10,000 fractional-rate edits. |
| DP-03 | Structural Source/Sequence/Hold/Repeat/Retime primitives. | Partial | Validated tree, reversible commands, and [`deadpan-plan`](../crates/deadpan-plan/) picture mapping and [bounded structural audio queries](AUDIO_PLAN.md) through nested primitives, sparse overrides and compact repeat indexes. Audio keeps absolute sample allocation, original source coordinates, pitch stages and distinct Hold policies. | Semantic range selectors, incremental fragment reuse, actual golden picture/audio renders, and full preview/export integration. |
| DP-04 | Stable anchors, attachments, nested occurrences, single-play overrides. | Partial | Compact stable play IDs and exact revision-aware boundary/range queries. [`marks.rs`](../crates/deadpan-core/src/marks.rs) adds persistent marks, ownership/loss policies, biased structural transforms, and named-mark selection with [integration/property tests](../crates/deadpan-core/tests/marks.rs). [Sparse overrides](OVERRIDE_VERIFICATION.md) and [automatic nested occurrence edits](OCCURRENCE_VERIFICATION.md) preserve variable durations, owned marks, exact picture mappings, and atomic history. | Temporal attachments, partial-range and multi-target occurrence operations, explode/duplicate transforms, and complete structural edit property tests. |
| DP-05 | Complete normal/visual/operator/command/camera/trim keyboard flow. | Partial | [Native workspace](NATIVE_WORKSPACE.md) adds counted frame/beat navigation, persistent prefixes, pane focus, source search, command entry, explicit insertion/history shortcuts, text/IME suppression, and root-beat `s`/`rr`/`dd` plus typed Split/Repeat/Hold-duration commands, counted `,h` pauses and exact `:hold` units. | Full editing grammar, nested occurrence navigation, binding matrix, native IME/layout coverage and keyboard-only editorial session. |
| DP-06 | Registers, macros, semantic dot-repeat, configurable bindings. | Open | None. | Parser/transaction/replay tests. |
| DP-07 | All time/delivery operations in Section 8. | Partial | Core commands insert/delete/move/group/ungroup nodes, wrap/update structural repeats, and change Hold duration/provider. Pure Split retains complete contexts and logical marks. Native root-beat Split/Repeat/delete/Hold-duration edits use that path and refresh the stopped-frame picture plan. Atomic InsertTime retains sample phase through Source/Hold fragments and freezes a measured original picture. | Arbitrary nested/gapped Hold insertion, compact occurrence resume dispatch, nested range planning, remaining operations, semantic targeting, recipe fixture renders, and editable inspector demos. |
| DP-08 | All framing/picture operations and keyboard target selection. | Partial | [Canvas geometry transactions](PRESENTATION_BASIS.md) preserve frame rate, nodes and marks; source-derived geometry uses qualified receipt metadata. | Framing/camera operations, keyboard target selection, effect reevaluation, and tracking/geometry/interaction tests. |
| DP-09 | All audio operations with preserved intentional dynamics. | Partial | Pre-master [sequence audition](PLAYBACK.md) connects canonical PCM to the native device; full audio authoring remains open. [Raw DSP qualification](qualification/audio-2026-09-20.md) retains failed targets. The [canonical worker prototype](qualification/audio-canonical-2026-09-20.md) supplies the single schedule now used by the bounded [production DSP adapter](AUDIO_DSP.md). [Source preparation](AUDIO_PREPARATION.md) implements exact-phase resampling and explicit matrices without loudness normalization. [Plan-driven source PCM](SOURCE_STAGE_AUDIO.md) binds exact spans to historical qualified media, including repeats, silent Holds and FollowSpeed retimes. [Continuous Preserve stages](AUDIO_STAGE_PREPARATION.md) retain exact fractional grids and nested history with bounded preparation and source/layout-aware caches. [Room-tone loops](ROOM_TONE_AUDIO.md) use explicit ranges and exact crossfades, with [qualification](qualification/room-tone-audio-2026-09-21.md). [Authored edges](AUDIO_EDGES.md) add reversible hard exceptions and shared post-mapping fades. | Room-tone selection/editing/audition UI, full voice processing, authored layout choice, full signal/format/listening corpus, remaining fade integration, gain/tails/limiting, all remaining audio operations, preview/export equivalence, devices, long-clip preparation and cache lifecycle. |
| DP-10 | Local transcript, timing refinement, shot/silence proposals. | Open | None. | Analysis accuracy and correction tests. |
| DP-11 | Selected target tracking with manual correction and loss handling. | Open | None. | Occlusion/shot-change fixtures. |
| DP-12 | Local AI hold generation, exact seams/duration, variants, acceptance. | Open | A [real supervised MLX development adapter](qualification/model-worker-2026-09-21.md) uses exact bridge planning and interior sampling, with decoded-file timing/color/hash checks. [Generated Hold semantics](GENERATED_HOLDS.md) retain sampling and resize fallback. [Dedicated store acceptance](GENERATION_ACCEPTANCE.md) binds the selected Ready receipt, retained inputs and derived assets to one reversible edit. Generic ingress remains guarded; no app backend or qualified model pack. | Source joins, speech preservation, source/color context, audition/variants, app integration, and the full qualified model corpus. |
| DP-13 | Model/runtime manager, safe downloads, offline pack installation. | Open | None. | Clean-machine and interrupted-install tests. |
| DP-14 | One Original from a YouTube URL with bundled JavaScript support. | Open | None. | Clean-machine single-video import and automatic full-original baseline creation. |
| DP-15 | One local Original plus external audio-only effects, managed/linked assets and relinking. | Partial | [Original ownership](ORIGINAL_MEDIA.md) retains complete originals through APFS clone/verified copy, records linked locations, checks identity on relink, and returns private snapshots. [Source registration](SOURCE_REGISTRATION.md) qualifies explicitly selected streams, retains measured indexes and receipts, and registers/inserts with exact common-origin placements atomically. [Store tests](../crates/deadpan-store/tests/source_registration.rs) cover historical alias reuse, rollback, deduplication, undo/redo and relocation. [Automatic basis tests](../crates/deadpan-store/tests/presentation_basis.rs) cover primary intent, final-rate placement, audio clock locking and geometry adoption. [Background preparation](IMPORT_PREPARATION.md) keeps file verification and receipt preparation independent of the writer and rechecks source freshness at commit. | Native register/insert has [workspace evidence](qualification/native-workspace-2026-09-21.md); single-original initialization retry is implemented; relink and basis-preview UI, bookmark resolution, legacy asset requalification, sound-event placement and full format/failure matrix remain open. |
| DP-16 | Shared realtime/offline renderer, bounded decode and proxy paths. | Partial | Structural picture plans plus persistent source decoding and [`deadpan-render`](../crates/deadpan-render/) shared SDR composition. [Metal qualification](qualification/source-preview-2026-09-21.md) compares 76 synthetic cases with a CPU reference; the app displays real decoded sources and exact plan-driven sequence frames through this pipeline. [Presentation state](qualification/preview-presentation-2026-09-21.md) retains actual sequence/revision identity through decode and GPU delays. Native sequence audition adds device-clock picture coalescing and exact paused-sample resume. | Full format/color matrix, mastered playback, proxies, acoustic synchronization, effects, preview/export comparison and stress benchmarks. |
| DP-17 | One-action automatic SDR/HDR YouTube-oriented output. | Open | None. | Encoded-file metadata/pixel/sync verification. |
| DP-18 | Nonblocking worker lifecycle, cancellation, stale result handling. | Partial | [`deadpan-jobs`](../crates/deadpan-jobs/) adds bounded typed framing, a revision-aware attempt lifecycle, native subprocess supervision, and [contained hash-verified snapshots](ARTIFACT_VERIFICATION.md). A [real MLX development worker](qualification/model-worker-2026-09-21.md) exercises this boundary. [Persistent requests](GENERATION_REQUESTS.md) atomically reconcile relevance; [attempts](GENERATION_ATTEMPTS.md) retain retries, validation receipts, candidate selection, and interrupted states across restart. | Bounded priority scheduling, app-connected inference/render workers and context resolution, production media validation/promotion, application lifecycle, and full concurrency/chaos coverage. |
| DP-19 | Cache integrity and accepted-media portability. | Partial | [Host FFV1 conversion](MEDIA_CONVERSION.md) verifies generated pixels/timing; shared [object storage](ORIGINAL_MEDIA.md) verifies generated objects and complete originals. [Admission](GENERATION_ACCEPTANCE.md) requires six retained objects before Ready/acceptance and derives assets from measured spans. Relocation, undo/redo/revert and independent real-media readback are exercised. Legacy receipts gain no inferred admission evidence. | Source-clock/color evidence, dependency/history reference tracking, cache eviction, portable copy, and offline-project rendering. |
| DP-20 | Focused one-Original UI with visible keybindings and native accessibility. | Partial | [`deadpan-app`](../crates/deadpan-app/) [project workspace](NATIVE_WORKSPACE.md) has labeled source/beat/pane controls, native file panels, visible context and shortcut help. Visual review corrected contrast/glyphs; native keyboard and accent composition checks are recorded in [qualification](qualification/source-preview-2026-09-21.md). | Full workflow, VoiceOver, CJK IME and non-US layout acceptance; document editing and inspector ergonomics. |
| DP-21 | CLI/JSON API with revision checks and dry-run. | Partial | [Shared headless API](HEADLESS.md): project/command/history operations, explicit migration, picture/audio-plan and source-PCM inspection, exact boundary and named-mark range resolution, original retention/inventory/verification/relinking, automatic project creation, source registration/insertion, geometry preview/adoption and structured errors. [Registration subprocess tests](../crates/deadpan-cli/tests/source_registration.rs) and [audio inspection tests](../crates/deadpan-cli/tests/audio_inspection.rs) cover actual media, historical identity, read-only coexistence and stable failures. | Complete command/selector surface, host socket routing, final render operations, and headless/GUI parity. |
| DP-22 | Signed/notarized zero-manual-setup distribution. | Open | None; source development builds are not an application distribution. | Clean-machine online and offline acceptance. |
| DP-23 | License/SBOM/privacy/security requirements. | Partial | [Dependency inventory](DEPENDENCIES.md), native harness build/license hashes, strict bounded domain JSON, schema checks, and initial package-path protections. Qualification explicitly excludes the developer GPL FFmpeg build from distribution. | Release audit, complete hostile-project/worker/pack tests, SBOM/notices, privacy checks, and exact shipped component licenses. |
| DP-24 | Measured performance budgets and diagnostics. | Partial | `doctor` reports actual core/SQLite probes. [Native harness](qualification/media-2026-09-20.md) measures tiny fixture decode/seek/encode on recorded hardware; these are qualification observations, not product budgets. | Full-size playback/edit/export/inference benchmarks, latency distributions, diagnostics, memory pressure, and published reproducible product measurements. |

## Delivery gates

[Specification Section 30](spec/DEADPAN_SPEC.md#30-implementation-workstreams-and-delivery-gates) defines the complete ordered build plan. Gates may have parallel work behind their interfaces; none permits calling an earlier subset the completed product.

The [first actual LTX MLX probe](qualification/model-smoke-2026-09-20.md) adds
single-file generation evidence to Gate A. It does not select a model or satisfy
the corpus, warm-performance, exact seam, color, or packaging gates.
The [supervised adapter](qualification/model-worker-2026-09-21.md) adds real
worker-boundary and exact interior-file evidence while those gates remain open.
The [lossless master probe](qualification/ffv1-2026-09-21.md) adds actual generated
frame conversion through LGPL FFmpeg, with normal/sanitizer runs, failed initial
duration metadata, rejected corrupt payloads, and retained trailer-truncation
limitations. Durable promotion and application rendering remain open.

| Gate | Status | Required work and exit evidence |
| --- | --- | --- |
| A: Qualify risky dependencies | Partial | [Native](qualification/media-2026-09-20.md) and [compatible media](qualification/media-compatible-2026-09-20.md) harnesses include actual decode/seek/audio, sanitizers, and retained frame ownership. Signed LGPL FFmpeg 8.0.3 works with pinned rsmpeg; B-frame mux and VFR terminal-duration failures remain. The [raw DSP candidate](qualification/audio-2026-09-20.md) retains 82/605 failures. A [canonical audio prototype](qualification/audio-canonical-2026-09-20.md) passes 3,447 checks per normal/sanitized run, with 558 identical PCM hashes and failed analysis-window alternatives retained. Still required: integrated shipping adapters, format/color matrix, GPU viewport, application audio scheduling/cache lifecycle/device output, listening, model candidates, private-runtime packaging, full-size benchmarks, and model-pack qualification. |
| B: Establish the pure editing foundation | Partial | Typed time/nodes, commands/inverses, stable nested instance paths, persistent marks and edit transforms, sparse play subtrees with variable-duration indexing, atomic edits through complete occurrence paths, exact boundary/range queries, schema/history migration, headless API, indexed exact picture plans and bounded structural audio plans exist. Still required: temporal attachments, remaining semantic range/text operators, incremental fragments and full audio rendering. Representative nested edits must render with exact picture/sample selection; serialization and inverse transactions must preserve all authored meaning. |
| C: Build the interactive media workspace | Partial | Non-destructive source preview uses actual persistent decode/index and the shared SDR GPU baseline, with tested frame navigation. Managed/linked original storage is available through the headless boundary. The native workspace creates one-Original projects in Documents/Deadpan, initializes the full video, protects that undo baseline, registers separate sound media, reuses the original and preserves legacy projects. Imagegen targets guide the visible Original/Your edit layout and contextual keyboard teaching. Still required: full import management, proxies, mastered audio/playback, complete pane/selection/inspector workflows, full keyboard grammar, focus/IME/accessibility corpus and measured editorial latency without drift. |
| D: Complete the creative operation surface | Open | Every Section 8 operation and starter recipe, per-play overrides, tails, stretch/pitch, cutaways, framing, saved gags, registers, semantic macros, and shared command/help registry. Each must remain editable/portable and pass preview/export verification without no-op placeholders. |
| E: Add analysis and real AI holds | Open | Local analysis and correction, tracking, runtime/model manager, generation planning/validation, audition/acceptance, stale-job handling, and caching. Actual qualified local generations must meet duration/seam contracts; accepted projects must render offline without the model. Publish latency and quality measurements. |
| F: Complete import, export, and distribution | Open | Bundled single-video yt-dlp/EJS/Deno, Original provenance and full-source initialization, audio-only effect import, safe updates, automatic output, HDR/SDR and codec/mux verification, notices, signed runtimes, notarization, recovery/migration, and disk/permission failures. Complete the keyboard-only source-URL-to-MP4 workflow from the distribution without external setup. |
| G: Release qualification | Open | Run every requirement, crash/chaos/malicious-input suite, long-project stress, preview/export comparisons, clean-machine online/offline installation, accessibility, and performance measurements. Deliver app, approved packs, documentation, fixture/benchmark reports, SBOM/notices, and migration policy; explicitly report any deviation. |

## Updating evidence

For each completed slice, link the implementation and named tests plus an acceptance report containing the revision, fixture/input, command or interaction, expected/observed result, and environment. Include hardware/OS, dependency/runtime versions, power/cache state, latency distribution, and failed samples where relevant. Preserve unfulfilled behavior explicitly.

Before marking any creative operation complete, establish that it is editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A passing test double proves only its tested boundary. A claimed release requires actual media, actual local generations, verified emitted files, and the distributed clean-machine workflow.
