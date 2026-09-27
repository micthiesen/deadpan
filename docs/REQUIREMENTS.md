# Requirements and delivery gates

All DP-01 through DP-24 requirements in [specification Section 29](spec/DEADPAN_SPEC.md#29-requirements-traceability) remain in scope. Their detailed sections are normative. This tracker records the current implementation and measured evidence, not a reduced release scope.

Specification 1.1 applies them to [one Original](SINGLE_ORIGINAL.md): the full
video is the initial edit, native projects live in Documents/Deadpan, same-video
moments can be reused, and external media contributes sound only. Accepted AI
extensions remain in scope. Generic backend and legacy multi-video projects are
preserved. [Current design targets](design/README.md) replace the earlier
multi-video workspace; concept screens do not establish completed capabilities.

**Open** means required behavior has no qualifying implementation. **Partial**
identifies concrete groundwork while acceptance remains unmet. **Complete**
requires linked code, passing relevant tests, and a demonstrable acceptance
result. No requirement or delivery gate is complete. [Development](DEVELOPMENT.md)
defines the repository gate and the [UI feedback loop](UI_FEEDBACK.md) defines
application replay, image review and separate responsiveness measurements.
Neither a passing harness nor a screenshot establishes full acceptance. Current
interaction suggestions and personal shortcut conflicts are tracked in
[Interaction review](INTERACTION_REVIEW.md); each implemented fix needs its own
evidence. The initial optional UI harness passed nine visual replay scenarios
and its shortcut audit; its release run missed Repeat/Hold feedback targets.
The [expanded qualification](qualification/workspace-layout-2026-09-27.md)
records fourteen-scenario composite coverage and fresh release p95 values of
4.67 ms for navigation, 6.85 ms for Repeat and 9.11 ms for Hold, all within their
existing targets. The earlier misses remain retained. Native accessibility,
physical display behavior and full-size workloads remain open.

The [footer qualification](qualification/footer-layout-2026-09-27.md) resolves
command-exit layout gaps, clipped Original shortcuts and immediate-Escape text
loss. It retains failing development cases, controlled async test delivery,
final workspace checks, same-frame replay assertions and reviewed Metal captures.
The minimum-size picture still needs more room; no requirement or gate closes.

The [repaint-wait correction](qualification/repaint-wake-2026-09-26.md) replaces
harness polling with bounded egui wakeups and separates worker phases from UI
delivery. That increment's feature lint and 193 unit plus 2 integration tests
passed, while both GPU replays stopped before scenarios because Metal was
unavailable. The later
[layout qualification](qualification/workspace-layout-2026-09-27.md) restores
actual Metal evidence for all fourteen current scenarios through a full run and
affected-path follow-ups. It fixes common Camera visibility, first-frame error
and speed-hint clipping, and pinned sound controls. Default app tests (208) and
feature tests (236), both Clippy configurations and the final focused replays
pass. These checks do not complete the full editing grammar or native
accessibility acceptance.

[Authored framing and Camera](FRAMING.md) implement
per-node static/enveloped framing, retained effect clocks and intermediate clips,
core-18/database-24 history, shared native/offline spatial rendering, and a
temporary keyboard Camera mode. The [new Camera target](design/README.md) retains
the intended saved-target and region workflows, which are not implemented yet.
[Qualification](qualification/framing-2026-09-24.md) records headless, actual Metal,
migration and native aesthetics/keyboard evidence. This work does not complete
DP-08 or any gate.

[Captured framing](CAPTURED_FRAMING.md) adds retained canvas fits and intermediate
clips to Hold recipes, independent of provider changes and the new Hold's Camera
settings. Root pause insertion samples descendant framing and inherits the root's
live framing once. Core 19/database 25 preserve earlier history through a frozen
core-18 adapter. Its [qualification record](qualification/captured-framing-2026-09-26.md)
separates headless checks from blocked Metal and native visual review; this work
does not complete DP-08 or any gate.

[Exact Original moments](SOURCE_MOMENTS.md) derive selected VFR picture intervals
and preserve fractional audio boundaries with a separate audible selection over
the full measured sample span. Core 20/database 26 and audio-context schema 2
retain that intent. Native selection and Sequence-slot paste now build on this layer; persistent
registers, named moments and the complete general splice remain required. [Verification](qualification/source-moments-2026-09-26.md)
records the tests, review and remaining acceptance. No requirement or gate changes status.

[Repeat-gap definition clocks](AUDIO_DEFINITIONS.md) provide current gap recipes
on intrinsic PointCeil or explicit root/point placements, including one-play
Repeats without a rendered gap. The bounded frozen-layout support query resolves
actual stable gap occurrences without crossing an opaque Preserve clock.
Authored gap bindings now consume these operands; general atomic splice
remains open. Verification of the original operand increment is recorded in
[the gap-clock qualification](qualification/gap-clocks-2026-09-26.md).

[Compact audio reanchors](AUDIO_REANCHORS.md) retain per-occurrence visible
allocation entries and chronological sample-phase changes without expanding
Repeats. Core 21/database 27 preserve these steps through copied scopes and
durable history; older histories gain no invented steps. General cursor splice and the full register workflow remain open.
[Qualification](qualification/audio-reanchors-2026-09-26.md) records verification
and limitations. No DP requirement or gate changes status.

[Authored gap bindings](GAP_AUDIO_BINDINGS.md) add core-22/database-28 ownership
for a Repeat's gap recipe, separate from its full timeline. Capture includes
unplayed gaps; stable existing gaps retain their sampling phase while newly born
gaps use their canonical definition clock. Root, point and physical-domain
evaluation share the PCM path, current policy and source admission. Complete
range splice and persistent register support remain required.
[Qualification](qualification/gap-bindings-2026-09-26.md) records passing focused
checks, strict history replay, independent review and the full-run failures with
their diagnostic follow-up. No requirement or gate changes status.

[Editable Repeat gaps](REPEAT_GAP_BRANCHES.md) add core-23/database-29 sparse
owned branches, including zero-gap suppression, dormant final branches,
occurrence-safe copies and exact default-gap materialization. The current Node
recipe retains historical gap clocks without freezing its raw audio policy.
Core, plan and decoded-PCM tests cover these paths; complete project-boundary
splice, persistent registers and full media acceptance remain open.
[Qualification](qualification/gap-branches-2026-09-26.md) records current CLI
history checks, independent reviews, strict migrations and workspace/harness
results, including the remaining sandbox-denied socket check.

[Composite suffix insertion](INSERT_TIME.md) adds core-24/database-30 pause edits
at existing root Sequence seams, with current-clock entry dispatch through
Repeat/gap branches and Preserve outputs. Old-admitted edits keep their prior
reducer, and older history cannot acquire the broader admission. Core, picture,
decoded-PCM, native service and migration checks are recorded in
[qualification](qualification/composite-insertion-2026-09-26.md). The new UI
replay steps are present but their visual run is blocked by the sandbox's missing
Metal adapter. Arbitrary interior splice and Visual replacement remain open;
no requirement or gate changes status.

[Interior pause insertion](INSERT_TIME.md) adds core-25/database-31 atomic cuts
inside root Source/ordinary Hold fragments before composite suffixes. Separate
pre-Split sampling and post-Split placement records retain exact rounded audio
entries, captured framing and one-step undo. Frozen core 24 retains its narrower
contextual admission. [Qualification](qualification/interior-insertion-2026-09-26.md)
records this increment's checks and limitations. Arbitrary nested insertion and
Visual replacement remain required; no requirement or gate changes status.

[Nested Sequence pauses](INSERT_TIME.md) add core-26/database-32 admission for
cuts inside ordinary groups. The Hold belongs to its actual Sequence, live
ancestor framing applies once, and every later sibling resumes from its own
audio entry. The native completion records the exact cursor and selects the
visible enclosing child. Repeat/Retime interiors, rational owner clocks and
Visual replacement remain required. See
[qualification](qualification/nested-sequence-2026-09-26.md) for the checks and
limits. No requirement or gate changes status.

[Sequence group navigation](GROUP_NAVIGATION.md) adds native Enter/Backspace,
breadcrumbs, current-depth inspector edits, scoped Original reuse and Camera.
Service requests retain their exact scope across preparation and completion;
history reconciles paths without retargeting the cursor. The contributed UI
harness now covers nested navigation and editing. Repeat/Retime occurrence
navigation and the full keyboard editing grammar remain open.
See [group navigation qualification](qualification/group-navigation-2026-09-26.md)
for actual checks, review and the unavailable GPU replay.

[Original moment selection and paste](SOURCE_MOMENTS.md) add native v/y and p/P,
an identity-bound session copy, measured temporal range bar, and exact explicit
Sequence-slot insertion. Core 27/database 33 retain shifted audio entries and
admit prepared receipts atomically with history/relevance. Persistent/named
registers, Visual replacement, arbitrary occurrence/cursor splice and native
visual/performance acceptance remain open. See
[paste qualification](qualification/moment-paste-2026-09-27.md) for actual evidence.
No requirement or gate changes status.

[Exact boundary descent](STRUCTURAL_SPLICE_DESIGN.md#exact-boundary-descent)
now exposes the complete revision-bound owner path through Sequences, Retimes,
stable Repeat plays and implicit/owned gaps. Core and headless queries retain
fractional coordinates and shared work bounds without editing the project.
This is a prerequisite for arbitrary nested insertion, not its implementation.
[Qualification](qualification/boundary-location-2026-09-26.md) records the scope
and verification. No requirement or gate changes status.

[Derived-clock framing](FRAMING.md) evaluates exact rational owner extents with
bounded numeric arithmetic while preserving existing integer callers. It is a
prerequisite for the [nested splice representation](STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks),
not support for fractional authored frames or a new editing command. Structural
clock maps, retained DSP contexts and arbitrary nested splice remain open.
[Qualification](qualification/framing-clocks-2026-09-26.md) records the independent
reference cases, reviews, regression checks and unchanged schema boundary.

[Live audio input tapes](AUDIO_INPUT_TAPES.md) project exact current-tree windows
onto one PointCeil grid and read their PCM through the shared renderer. They
retain sampling support and scoped identities. Checked `AudioStageProjection`
views now supply nested intrinsic operands, independent output-policy clocks
and request-local PCM memoization; a PointCeil tape can schedule that output
around an inserted pause. [Projected root placement](AUDIO_PROJECTED_ROOT.md)
separately supplies absolute RoundEven allocation and repeated resume that
preserves phase for one physical projection. Persisted splice routes, effective owner
clocks, aggregate output scheduling and arbitrary nested editing remain open.
These evaluation APIs do not complete a requirement or gate.
[Qualification](qualification/audio-input-tapes-2026-09-26.md) records its exact
mapping tests, real PCM comparisons, independent review and repository checks.
[Intrinsic projection qualification](qualification/preserve-projections-2026-09-26.md)
records nested-history PCM, output-policy clocks, admission and memory limits,
review corrections and the unchanged authored-schema boundary.
[Root projection qualification](qualification/projected-root-2026-09-26.md)
records independent sample-phase and policy tests, admission review and the
required repository checks.

[Scoped sound mixing](SOUND_EVENTS.md) adds borrowed ordered voices, exact gates
and per-voice silence on a common intrinsic grid. Explicit aggregate Preserve
input uses one canonical preparation and retains its independent output-policy clock. This is
raw preparation before creative voice effects and mastering. Persisted sound
recipes, edit transforms, native placement, voice treatments,
final bus integration and preview/export evidence remain required. Catalog import
alone does not place a sound, and this prerequisite does not promote a DP or gate.
[Qualification](qualification/scoped-mix-2026-09-27.md) records decoded PCM,
scope/policy checks, review corrections and remaining acceptance work.

The sound increment adds separate catalog audition through canonical playback,
a bounded exact recipe-route kernel and a CLI LRU source cache. Sound playback
uses measured sample endpoints and preserves the stopped picture, edit clocks
and selection. The route kernel is not yet installed in authored sound events;
no placement, migration, DSP lattice or final mix is implied. The
[audition board](design/boards/sound-audition-board-v1.png) and exact prompt are
retained targets. [Qualification](qualification/sound-audition-2026-09-27.md)
records reviews, actual PCM, harness checks and acceptance limits. All DP and gate
statuses remain unchanged.

The [sound-clock preparation increment](qualification/sound-clocks-2026-09-27.md)
adds physical sample-grid history to exact routes, separate current Hold issuer
queries, and bounded LRU eviction to playback's source cache. Window/Keep
projections preserve both the complete recipe and the old selection's audible
mask, so displaced rounding cannot expose audio past a cut. These remain
preparation APIs; authored sound persistence, placement, voice processing and
final mixing are still required. Core 28/database 34 are unchanged. No DP or
gate status is promoted.

The [catalog source-voice increment](qualification/source-voices-2026-09-27.md)
adds a checked independent audio operand with exact natural-rate source placement
and separate input/output Hold policy. It reuses the tape, source admission and
canonical Preserve readers without adding authored Source nodes. This does not
install sampled routes or persisted events, grant allowances, implement voice
effects or populate the final bus. All DP and gate statuses remain unchanged.

[Retained sample routes](qualification/routed-voices-2026-09-27.md) now have checked
PCM preparation handles for independent source input, intrinsic Preserve output
and captured root output. Reads preserve
the old physical samples and complete filter/DSP history instead of reconstructing
phase from the edited frame clock. Captured output policy and old selected masks
stay separate from current consuming Hold gates. These remain preparation APIs;
persisted sound-event commands, allowances, voice effects and final bus integration
are still required. Core 28/database 34 and all DP/gate statuses are unchanged.

The [playback test scheduling record](qualification/playback-waits-2026-09-27.md)
tracks recurring cold-preparation timeouts separately from product failures.
The test-only PCM reservation includes detached worker teardown; production
timeouts, PCM assertions and all requirement/gate statuses remain unchanged.

[Original and edit audition](PLAYBACK.md) connects immutable qualified originals and
canonical limited PCM to the native device, with Space Play/Pause, a monitor
level, exact audio-clock picture scheduling and explicit interruption. Bounded
preparation runs separately from device control. Original Space playback uses
the complete measured A/V union; Shift+Space loops the selected Original moment
or edited beat with adjustable context. Delivery remains monotonic across laps,
and read-only audition leaves history unchanged. Full mastering,
acoustic synchronization, long-source/performance qualification and
preview/export equivalence remain required. This increment does not complete a
requirement or gate; [qualification](qualification/playback-2026-09-24.md) records
its native, headless and review evidence. Older evidence below describes its own
historical boundary.
The [Original/selection increment](qualification/original-audition-2026-09-27.md)
records the separate full-source and selected-picture endpoint policies, real
PCM and native routing regressions, and contributed harness coverage. All product
requirements and gates retain their existing open/partial status.

[Master gain research](qualification/audio-limiter-gain-search-2026-09-24.md)
retains independently audited finite-fixture solutions, the corrected unwanted
muting failure, and a longer stress failure in a faster finite-context design.
Neither of those earlier candidates is adopted. The [shared limited reader](AUDIO_MASTERING.md)
now implements a pinned finite oversampled gain path over the current edge-faded
bus, consumed by playback and `inspect-audio --limited`. It uses canonical tiles,
complete source provenance, one cumulative preparation budget and final-f32
verification, with [qualification evidence](qualification/audio-limited-2026-09-24.md).
Full voice/group processing, gain-motion/listening acceptance and
encoded output remain open. No requirement or gate changes status.

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
captures an interior boundary in the selected child and selects the committed right fragment.
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
compact birth scope and existing bindings, including configured Repeat gaps
through the separate [gap ownership map](GAP_AUDIO_BINDINGS.md).
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
preserve PCM; the failed limiter prototypes remain historical evidence. The
current [limited reader](AUDIO_MASTERING.md) separately applies gain and reports
reduction through the headless API. Reduction UI and final mix integration remain
open.

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

[Structural speed editing](RETIME_EDITING.md) adds exact native Retime creation
and parameter adjustment with explicit preserve/tape pitch, retained input range,
atomic history and a closed core-28/database-34 migration boundary. Full range
operators, variable speed, independent pitch, rendered GUI acceptance and encoded
preview/export equivalence remain required; this does not complete DP-07 or DP-09.

| ID | Requirement | Status | Implementation / tests now | Required acceptance evidence still outstanding |
| --- | --- | --- | --- | --- |
| DP-01 | Documents library, one-Original initialization/baseline, reopen, autosave, undo/redo, migration, recovery. | Partial | [`deadpan-store`](../crates/deadpan-store/): durable packages/history, atomic mark transforms and generation relevance, writer ownership, WAL checkpoints, interrupted-attempt recovery, and [schema-1-through-25-to-26 migration tests](../crates/deadpan-store/tests/migration.rs) using old-binary-validated fixtures with requests, attempts, admission, source placements, branches and redo. | Native create/open/history now have [workspace evidence](qualification/native-workspace-2026-09-21.md); full media lifecycle, restore/recovery UI, history limits and full failure/chaos suite remain open. |
| DP-02 | Exact frame/sample/source-time model including VFR. | Partial | Typed rational clocks, VFR intervals, [independent picture mappings](SOURCE_VIDEO_MAPPING.md) and explicit selected-span endpoints in core and plan. [Original moment candidates](SOURCE_MOMENTS.md) retain exact VFR and fractional-sample selection boundaries. [`SourceSession`](../crates/deadpan-media/src/source_session.rs) builds original-PTS indexes from private verified media and performs persistent exact seeks. [Registration](SOURCE_REGISTRATION.md) retains validated indexes and exact common origin by historical revision. [Native source evidence](qualification/source-preview-2026-09-21.md) retains measured VFR terminal-duration loss. | Complete source policies and actual shared playback/export, including 10,000 fractional-rate edits. |
| DP-03 | Structural Source/Sequence/Hold/Repeat/Retime primitives. | Partial | Validated tree, reversible commands, and [`deadpan-plan`](../crates/deadpan-plan/) picture mapping and [bounded structural audio queries](AUDIO_PLAN.md) through nested primitives, sparse overrides and compact repeat indexes. Audio keeps absolute sample allocation, original source coordinates, pitch stages and distinct Hold policies. | Semantic range selectors, incremental fragment reuse, actual golden picture/audio renders, and full preview/export integration. |
| DP-04 | Stable anchors, attachments, nested occurrences, single-play overrides. | Partial | Compact stable play IDs and exact revision-aware boundary/range queries. [`marks.rs`](../crates/deadpan-core/src/marks.rs) adds persistent marks, ownership/loss policies, biased structural transforms, and named-mark selection with [integration/property tests](../crates/deadpan-core/tests/marks.rs). [Sparse overrides](OVERRIDE_VERIFICATION.md) and [automatic nested occurrence edits](OCCURRENCE_VERIFICATION.md) preserve variable durations, owned marks, exact picture mappings, and atomic history. | Temporal attachments, partial-range and multi-target occurrence operations, explode/duplicate transforms, and complete structural edit property tests. |
| DP-05 | Complete normal/visual/operator/command/camera/trim keyboard flow. | Partial | [Native workspace](NATIVE_WORKSPACE.md) adds counted navigation, persistent prefixes, pane focus, search, command entry, text/IME suppression, Sequence Enter/Backspace and breadcrumbs, current-depth `s`/`rr`/`dd`, exact Hold commands and history. [Bounded explicit Repeat input](qualification/repeat-input-2026-09-27.md) retains eight batched wraps as separate commits and undo steps, preserving partial keys and reporting overflow/cancellation. [Same-frame footer and text focus](qualification/footer-layout-2026-09-27.md) preserve final text through immediate Escape and combined resize/input, keep pending keys single, and anchor the painted mode before presentation. `,i` reuses the Original without taking Kestrel's Cmd+Return. [Expanded UI qualification](qualification/workspace-layout-2026-09-27.md) checks 3,472 production routing cases against 62 global reservations and replays real editing/focus, Original moment, audition and Retime paths with visible controls and hints. | Full editing grammar, nested occurrence navigation, native IME/layout and physical key delivery coverage, and keyboard-only editorial session. |
| DP-06 | Registers, macros, semantic dot-repeat, configurable bindings. | Open | None. | Parser/transaction/replay tests. |
| DP-07 | All time/delivery operations in Section 8. | Partial | Core commands insert/delete/move/group/ungroup nodes, wrap/update structural repeats and [Retimes](RETIME_EDITING.md), and change Hold duration/provider. Pure Split retains complete contexts and logical marks. Native current-depth Split/Repeat/Retime/delete/Hold-duration edits use that path and refresh the stopped-frame picture plan. Atomic InsertTime retains sample phase through Source/Hold fragments under ordinary Sequence groups and freezes a measured original picture below live ancestors. | Arbitrary nested/gapped Hold insertion, compact occurrence resume dispatch, nested range planning, remaining operations, semantic targeting, recipe fixture renders, and editable inspector demos. |
| DP-08 | All framing/picture operations and keyboard target selection. | Partial | [Authored framing and Camera](FRAMING.md): static/enveloped operations, exact owner clocks, intermediate clipping, counted pan/zoom, numeric fields and center/corner selection; [earlier qualification](qualification/framing-2026-09-24.md) includes actual Metal/CPU comparisons, migration and native review. [Captured pauses](CAPTURED_FRAMING.md) retain input composition separately from live framing; [their evidence](qualification/captured-framing-2026-09-26.md) records remaining GPU/native checks. [Canvas transactions](PRESENTATION_BASIS.md) preserve time and reevaluate normalized framing. | Saved manual points/regions, keyboard region creation, detection/tracking, per-play escalation, native occurrence selection, equivalent framed Ungroup, Metal/native acceptance for captured pauses, complete Section 8 effects and preview/export acceptance. |
| DP-09 | All audio operations with preserved intentional dynamics. | Partial | Limited [Original/edit/sound audition](PLAYBACK.md) connects canonical PCM to the native device. The shared [finite oversampled limiter](AUDIO_MASTERING.md) verifies emitted tiles and preserves source-aware context across reads; full audio authoring remains open. [Raw DSP qualification](qualification/audio-2026-09-20.md) retains failed targets. The [canonical worker prototype](qualification/audio-canonical-2026-09-20.md) supplies the single schedule now used by the bounded [production DSP adapter](AUDIO_DSP.md). [Source preparation](AUDIO_PREPARATION.md) implements exact-phase resampling and explicit matrices without loudness normalization. [Plan-driven source PCM](SOURCE_STAGE_AUDIO.md) binds exact spans to historical qualified media, including repeats, silent Holds and FollowSpeed retimes. [Continuous Preserve stages](AUDIO_STAGE_PREPARATION.md) retain exact fractional grids and nested history with bounded preparation and source/layout-aware caches. [Room-tone loops](ROOM_TONE_AUDIO.md) use explicit ranges and exact crossfades, with [qualification](qualification/room-tone-audio-2026-09-21.md). [Authored edges](AUDIO_EDGES.md) add reversible hard exceptions and shared post-mapping fades. | Room-tone selection/editing/audition UI, full voice processing, authored layout choice, full signal/format/listening corpus, remaining fade integration, gain/tails and final-bus integration, all remaining audio operations, preview/export equivalence, devices, long-clip preparation and cache lifecycle. |
| DP-10 | Local transcript, timing refinement, shot/silence proposals. | Open | None. | Analysis accuracy and correction tests. |
| DP-11 | Selected target tracking with manual correction and loss handling. | Open | None. | Occlusion/shot-change fixtures. |
| DP-12 | Local AI hold generation, exact seams/duration, variants, acceptance. | Open | A [real supervised MLX development adapter](qualification/model-worker-2026-09-21.md) uses exact bridge planning and interior sampling, with decoded-file timing/color/hash checks. [Generated Hold semantics](GENERATED_HOLDS.md) retain sampling and resize fallback. [Dedicated store acceptance](GENERATION_ACCEPTANCE.md) binds the selected Ready receipt, retained inputs and derived assets to one reversible edit. Generic ingress remains guarded; no app backend or qualified model pack. | Source joins, speech preservation, source/color context, audition/variants, app integration, and the full qualified model corpus. |
| DP-13 | Model/runtime manager, safe downloads, offline pack installation. | Open | None. | Clean-machine and interrupted-install tests. |
| DP-14 | One Original from a YouTube URL with bundled JavaScript support. | Open | None. | Clean-machine single-video import and automatic full-original baseline creation. |
| DP-15 | One local Original plus external audio-only effects, managed/linked assets and relinking. | Partial | [Original ownership](ORIGINAL_MEDIA.md) retains complete originals through APFS clone/verified copy, records linked locations, checks identity on relink, and returns private snapshots. [Source registration](SOURCE_REGISTRATION.md) qualifies explicitly selected streams, retains measured indexes and receipts, and registers/inserts with exact common-origin placements atomically. [Store tests](../crates/deadpan-store/tests/source_registration.rs) cover historical alias reuse, rollback, deduplication, undo/redo and relocation. [Automatic basis tests](../crates/deadpan-store/tests/presentation_basis.rs) cover primary intent, final-rate placement, audio clock locking and geometry adoption. [Background preparation](IMPORT_PREPARATION.md) keeps file verification and receipt preparation independent of the writer and rechecks source freshness at commit. | Native register/insert has [workspace evidence](qualification/native-workspace-2026-09-21.md); single-original initialization retry is implemented; relink and basis-preview UI, bookmark resolution, legacy asset requalification, sound-event placement and full format/failure matrix remain open. |
| DP-16 | Shared realtime/offline renderer, bounded decode and proxy paths. | Partial | Structural picture plans plus persistent source decoding and [`deadpan-render`](../crates/deadpan-render/) shared SDR composition. [Metal qualification](qualification/source-preview-2026-09-21.md) compares 76 synthetic cases with a CPU reference; the app displays real decoded sources and exact plan-driven sequence frames through this pipeline. [Presentation state](qualification/preview-presentation-2026-09-21.md) retains actual sequence/revision identity through decode and GPU delays. Native Original/edit audition adds device-clock picture coalescing, context-bounded selection loops and exact paused-sample resume. | Full format/color matrix, mastered playback, proxies, acoustic synchronization, effects, preview/export comparison and stress benchmarks. |
| DP-17 | One-action automatic SDR/HDR YouTube-oriented output. | Open | None. | Encoded-file metadata/pixel/sync verification. |
| DP-18 | Nonblocking worker lifecycle, cancellation, stale result handling. | Partial | [`deadpan-jobs`](../crates/deadpan-jobs/) adds bounded typed framing, a revision-aware attempt lifecycle, native subprocess supervision, and [contained hash-verified snapshots](ARTIFACT_VERIFICATION.md). A [real MLX development worker](qualification/model-worker-2026-09-21.md) exercises this boundary. [Persistent requests](GENERATION_REQUESTS.md) atomically reconcile relevance; [attempts](GENERATION_ATTEMPTS.md) retain retries, validation receipts, candidate selection, and interrupted states across restart. | Bounded priority scheduling, app-connected inference/render workers and context resolution, production media validation/promotion, application lifecycle, and full concurrency/chaos coverage. |
| DP-19 | Cache integrity and accepted-media portability. | Partial | [Host FFV1 conversion](MEDIA_CONVERSION.md) verifies generated pixels/timing; shared [object storage](ORIGINAL_MEDIA.md) verifies generated objects and complete originals. [Admission](GENERATION_ACCEPTANCE.md) requires six retained objects before Ready/acceptance and derives assets from measured spans. Relocation, undo/redo/revert and independent real-media readback are exercised. Legacy receipts gain no inferred admission evidence. | Source-clock/color evidence, dependency/history reference tracking, cache eviction, portable copy, and offline-project rendering. |
| DP-20 | Focused one-Original UI with visible keybindings and native accessibility. | Partial | [`deadpan-app`](../crates/deadpan-app/) has labeled controls, visible context, shortcut help and native file panels. [Expanded UI replay and visual review](qualification/workspace-layout-2026-09-27.md) cover fourteen scenarios through a full run and focused follow-ups. Actual Metal captures show a larger viewer, visible Camera/pause actions, first-frame notices and pinned sound controls; pointer/wheel/text ownership, resize and 10,000-beat selection checks pass. [Repeat queue feedback](qualification/repeat-input-2026-09-27.md) exposes pending work and keeps the header Working through completion. [Footer qualification](qualification/footer-layout-2026-09-27.md) resolves first-frame command-exit gaps and clipped Original shortcuts, with same-input mode/geometry checks and actual Metal captures. Native keyboard/accent evidence remains in the [earlier qualification](qualification/source-preview-2026-09-21.md). | Full workflow, VoiceOver, CJK IME and non-US layout acceptance, thumbnails, other busy-command behavior and minimum-size picture priority in the [interaction review](INTERACTION_REVIEW.md). |
| DP-21 | CLI/JSON API with revision checks and dry-run. | Partial | [Shared headless API](HEADLESS.md): project/command/history operations, explicit migration, picture/audio-plan and source-PCM inspection, exact boundary and named-mark range resolution, original retention/inventory/verification/relinking, automatic project creation, source registration/insertion, geometry preview/adoption and structured errors. [Registration subprocess tests](../crates/deadpan-cli/tests/source_registration.rs) and [audio inspection tests](../crates/deadpan-cli/tests/audio_inspection.rs) cover actual media, historical identity, read-only coexistence and stable failures. | Complete command/selector surface, host socket routing, final render operations, and headless/GUI parity. |
| DP-22 | Signed/notarized zero-manual-setup distribution. | Open | None; source development builds are not an application distribution. | Clean-machine online and offline acceptance. |
| DP-23 | License/SBOM/privacy/security requirements. | Partial | [Dependency inventory](DEPENDENCIES.md), native harness build/license hashes, strict bounded domain JSON, schema checks, and initial package-path protections. Qualification explicitly excludes the developer GPL FFmpeg build from distribution. | Release audit, complete hostile-project/worker/pack tests, SBOM/notices, privacy checks, and exact shipped component licenses. |
| DP-24 | Measured performance budgets and diagnostics. | Partial | `doctor` reports actual core/SQLite probes. [Final footer release qualification](qualification/footer-layout-2026-09-27.md) passes warm navigation p95 5.20 ms, input CPU 0.70 ms, cached Repeat 7.76 ms and Hold fallback 8.59 ms against unchanged targets, after the bounded Repeat queue checks restore the Original. It uses the corrected repaint wait and actual offscreen Metal completion. [Earlier qualification](qualification/ui-feedback-2026-09-26.md) retains the 75.26/138.65 ms edit misses and 1.23 ms 10,000-beat navigation CPU result; the latter performance workload was not rerun here. | Qualify full-size playback/edit/export/inference, physical display latency, memory pressure and complete diagnostics. Tiny-fixture offscreen results do not establish those budgets. |

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
