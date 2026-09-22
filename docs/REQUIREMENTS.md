# Requirements and delivery gates

All DP-01 through DP-24 requirements in [specification Section 29](spec/DEADPAN_SPEC.md#29-requirements-traceability) remain in scope. Their detailed sections are normative. This tracker records the current implementation and measured evidence, not a reduced release scope.

**Open** means required behavior has no qualifying implementation. **Partial** identifies concrete groundwork while acceptance remains unmet. **Complete** requires linked code, passing relevant tests, and a demonstrable acceptance result. No requirement or delivery gate is complete. Baseline checks are described in [Development](DEVELOPMENT.md); they are not substitutes for full acceptance evidence.

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
Core schema 10/database schema 15 retain this policy and migrate older histories
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
Source/Sequence frame inspection. The [qualification report](qualification/native-workspace-2026-09-21.md)
records actual media, deterministic service interleavings, headless keyboard/focus
checks and native panel/picture observations. Full editing, playback, generated
provider preview, export and release acceptance remain open.

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

## Product requirements

| ID | Requirement | Status | Implementation / tests now | Required acceptance evidence still outstanding |
| --- | --- | --- | --- | --- |
| DP-01 | Project creation, reopen, autosave, undo/redo, migration, recovery. | Partial | [`deadpan-store`](../crates/deadpan-store/): durable packages/history, atomic mark transforms and generation relevance, writer ownership, WAL checkpoints, interrupted-attempt recovery, and [schema-1-through-14-to-15 migration tests](../crates/deadpan-store/tests/migration.rs) using old-binary-validated fixtures with requests, attempts, admission, source placements, branches and redo. | Native create/open/history now have [workspace evidence](qualification/native-workspace-2026-09-21.md); full media lifecycle, restore/recovery UI, history limits and full failure/chaos suite remain open. |
| DP-02 | Exact frame/sample/source-time model including VFR. | Partial | Typed rational clocks, VFR intervals, [independent picture mappings](SOURCE_VIDEO_MAPPING.md) and explicit selected-span endpoints in core and plan. [`SourceSession`](../crates/deadpan-media/src/source_session.rs) builds original-PTS indexes from private verified media and performs persistent exact seeks. [Registration](SOURCE_REGISTRATION.md) retains validated indexes and exact common origin by historical revision. [Native source evidence](qualification/source-preview-2026-09-21.md) retains measured VFR terminal-duration loss. | Complete source policies and actual shared playback/export, including 10,000 fractional-rate edits. |
| DP-03 | Structural Source/Sequence/Hold/Repeat/Retime primitives. | Partial | Validated tree, reversible commands, and [`deadpan-plan`](../crates/deadpan-plan/) picture mapping and [bounded structural audio queries](AUDIO_PLAN.md) through nested primitives, sparse overrides and compact repeat indexes. Audio keeps absolute sample allocation, original source coordinates, pitch stages and distinct Hold policies. | Semantic range selectors, incremental fragment reuse, actual golden picture/audio renders, and full preview/export integration. |
| DP-04 | Stable anchors, attachments, nested occurrences, single-play overrides. | Partial | Compact stable play IDs and exact revision-aware boundary/range queries. [`marks.rs`](../crates/deadpan-core/src/marks.rs) adds persistent marks, ownership/loss policies, biased structural transforms, and named-mark selection with [integration/property tests](../crates/deadpan-core/tests/marks.rs). [Sparse overrides](OVERRIDE_VERIFICATION.md) and [automatic nested occurrence edits](OCCURRENCE_VERIFICATION.md) preserve variable durations, owned marks, exact picture mappings, and atomic history. | Temporal attachments, partial-range and multi-target occurrence operations, explode/duplicate transforms, and complete structural edit property tests. |
| DP-05 | Complete normal/visual/operator/command/camera/trim keyboard flow. | Partial | [Native workspace](NATIVE_WORKSPACE.md) adds counted frame/beat navigation, persistent prefixes, pane focus, source search, command entry, explicit insertion/history shortcuts, and text/IME suppression. [Keyboard observations](qualification/source-preview-2026-09-21.md) cover this limited surface. | Full editing grammar, binding matrix, native IME/layout coverage and keyboard-only editorial session. |
| DP-06 | Registers, macros, semantic dot-repeat, configurable bindings. | Open | None. | Parser/transaction/replay tests. |
| DP-07 | All time/delivery operations in Section 8. | Partial | Core commands insert/delete/move/group/ungroup nodes, wrap/update structural repeats, and change Hold duration/provider. No rendered creative operation or interactive inspector yet. | Remaining operations, semantic targeting, recipe fixture renders, and editable inspector demos. |
| DP-08 | All framing/picture operations and keyboard target selection. | Partial | [Canvas geometry transactions](PRESENTATION_BASIS.md) preserve frame rate, nodes and marks; source-derived geometry uses qualified receipt metadata. | Framing/camera operations, keyboard target selection, effect reevaluation, and tracking/geometry/interaction tests. |
| DP-09 | All audio operations with preserved intentional dynamics. | Partial | No app audio operations. [Raw DSP qualification](qualification/audio-2026-09-20.md) retains failed targets. The [canonical worker prototype](qualification/audio-canonical-2026-09-20.md) supplies the single schedule now used by the bounded [production DSP adapter](AUDIO_DSP.md). [Source preparation](AUDIO_PREPARATION.md) implements exact-phase resampling and explicit matrices without loudness normalization. [Plan-driven source PCM](SOURCE_STAGE_AUDIO.md) binds exact spans to historical qualified media, including repeats, silent Holds and FollowSpeed retimes. [Continuous Preserve stages](AUDIO_STAGE_PREPARATION.md) retain exact fractional grids and nested history with bounded preparation and source/layout-aware caches. [Room-tone loops](ROOM_TONE_AUDIO.md) use explicit ranges and exact crossfades, with [qualification](qualification/room-tone-audio-2026-09-21.md). | Room-tone selection/editing/audition UI, full voice processing, authored layout choice, full signal/format/listening corpus, fades/gain/tails/limiting, all remaining audio operations, preview/export equivalence, devices, long-clip preparation and cache lifecycle. |
| DP-10 | Local transcript, timing refinement, shot/silence proposals. | Open | None. | Analysis accuracy and correction tests. |
| DP-11 | Selected target tracking with manual correction and loss handling. | Open | None. | Occlusion/shot-change fixtures. |
| DP-12 | Local AI hold generation, exact seams/duration, variants, acceptance. | Open | A [real supervised MLX development adapter](qualification/model-worker-2026-09-21.md) uses exact bridge planning and interior sampling, with decoded-file timing/color/hash checks. [Generated Hold semantics](GENERATED_HOLDS.md) retain sampling and resize fallback. [Dedicated store acceptance](GENERATION_ACCEPTANCE.md) binds the selected Ready receipt, retained inputs and derived assets to one reversible edit. Generic ingress remains guarded; no app backend or qualified model pack. | Source joins, speech preservation, source/color context, audition/variants, app integration, and the full qualified model corpus. |
| DP-13 | Model/runtime manager, safe downloads, offline pack installation. | Open | None. | Clean-machine and interrupted-install tests. |
| DP-14 | YouTube URL import with bundled JavaScript support. | Open | None. | Clean-machine permitted-source import. |
| DP-15 | Local media import, managed/linked assets, relinking. | Partial | [Original ownership](ORIGINAL_MEDIA.md) retains complete originals through APFS clone/verified copy, records linked locations, checks identity on relink, and returns private snapshots. [Source registration](SOURCE_REGISTRATION.md) qualifies explicitly selected streams, retains measured indexes and receipts, and registers/inserts with exact common-origin placements atomically. [Store tests](../crates/deadpan-store/tests/source_registration.rs) cover historical alias reuse, rollback, deduplication, undo/redo and relocation. [Automatic basis tests](../crates/deadpan-store/tests/presentation_basis.rs) cover primary intent, final-rate placement, audio clock locking and geometry adoption. [Background preparation](IMPORT_PREPARATION.md) keeps file verification and receipt preparation independent of the writer and rechecks source freshness at commit. | Native register/insert has [workspace evidence](qualification/native-workspace-2026-09-21.md); retry/relink and basis-preview UI, bookmark resolution, legacy asset requalification and full format/failure matrix remain open. |
| DP-16 | Shared realtime/offline renderer, bounded decode and proxy paths. | Partial | Structural picture plans plus persistent source decoding and [`deadpan-render`](../crates/deadpan-render/) shared SDR composition. [Metal qualification](qualification/source-preview-2026-09-21.md) compares 76 synthetic cases with a CPU reference; the app displays real decoded sources and exact plan-driven sequence frames through this pipeline. [Presentation state](qualification/preview-presentation-2026-09-21.md) retains actual sequence/revision identity through decode and GPU delays. | Full format/color matrix, plan-driven playback, audio/proxies, effects, preview/export comparison and stress benchmarks. |
| DP-17 | One-action automatic SDR/HDR YouTube-oriented output. | Open | None. | Encoded-file metadata/pixel/sync verification. |
| DP-18 | Nonblocking worker lifecycle, cancellation, stale result handling. | Partial | [`deadpan-jobs`](../crates/deadpan-jobs/) adds bounded typed framing, a revision-aware attempt lifecycle, native subprocess supervision, and [contained hash-verified snapshots](ARTIFACT_VERIFICATION.md). A [real MLX development worker](qualification/model-worker-2026-09-21.md) exercises this boundary. [Persistent requests](GENERATION_REQUESTS.md) atomically reconcile relevance; [attempts](GENERATION_ATTEMPTS.md) retain retries, validation receipts, candidate selection, and interrupted states across restart. | Bounded priority scheduling, app-connected inference/render workers and context resolution, production media validation/promotion, application lifecycle, and full concurrency/chaos coverage. |
| DP-19 | Cache integrity and accepted-media portability. | Partial | [Host FFV1 conversion](MEDIA_CONVERSION.md) verifies generated pixels/timing; shared [object storage](ORIGINAL_MEDIA.md) verifies generated objects and complete originals. [Admission](GENERATION_ACCEPTANCE.md) requires six retained objects before Ready/acceptance and derives assets from measured spans. Relocation, undo/redo/revert and independent real-media readback are exercised. Legacy receipts gain no inferred admission evidence. | Source-clock/color evidence, dependency/history reference tracking, cache eviction, portable copy, and offline-project rendering. |
| DP-20 | Accessible, native-behaving, simple UI. | Partial | [`deadpan-app`](../crates/deadpan-app/) [project workspace](NATIVE_WORKSPACE.md) has labeled source/beat/pane controls, native file panels, visible context and shortcut help. Visual review corrected contrast/glyphs; native keyboard and accent composition checks are recorded in [qualification](qualification/source-preview-2026-09-21.md). | Full workflow, VoiceOver, CJK IME and non-US layout acceptance; document editing and inspector ergonomics. |
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
| C: Build the interactive media workspace | Partial | Non-destructive source preview uses actual persistent decode/index and the shared SDR GPU baseline, with tested frame navigation. Managed/linked original storage is available through the headless boundary. The native workspace now creates/opens projects, registers and inserts sources, and navigates durable history. Still required: full import management, proxies, audio/playback, complete pane/selection/inspector workflows, full keyboard grammar, focus/IME/accessibility corpus and measured editorial latency without drift. |
| D: Complete the creative operation surface | Open | Every Section 8 operation and starter recipe, per-play overrides, tails, stretch/pitch, cutaways, framing, saved gags, registers, semantic macros, and shared command/help registry. Each must remain editable/portable and pass preview/export verification without no-op placeholders. |
| E: Add analysis and real AI holds | Open | Local analysis and correction, tracking, runtime/model manager, generation planning/validation, audition/acceptance, stale-job handling, and caching. Actual qualified local generations must meet duration/seam contracts; accepted projects must render offline without the model. Publish latency and quality measurements. |
| F: Complete import, export, and distribution | Open | Bundled yt-dlp/EJS/Deno, provenance, safe updates, automatic output, HDR/SDR and codec/mux verification, notices, signed runtimes, notarization, recovery/migration, and disk/permission failures. Complete the keyboard-only source-URL-to-MP4 workflow from the distribution without external setup. |
| G: Release qualification | Open | Run every requirement, crash/chaos/malicious-input suite, long-project stress, preview/export comparisons, clean-machine online/offline installation, accessibility, and performance measurements. Deliver app, approved packs, documentation, fixture/benchmark reports, SBOM/notices, and migration policy; explicitly report any deviation. |

## Updating evidence

For each completed slice, link the implementation and named tests plus an acceptance report containing the revision, fixture/input, command or interaction, expected/observed result, and environment. Include hardware/OS, dependency/runtime versions, power/cache state, latency distribution, and failed samples where relevant. Preserve unfulfilled behavior explicitly.

Before marking any creative operation complete, establish that it is editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A passing test double proves only its tested boundary. A claimed release requires actual media, actual local generations, verified emitted files, and the distributed clean-machine workflow.
