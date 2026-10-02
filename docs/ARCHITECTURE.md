# Architecture

[Specification Sections 21, 23, and 24](spec/DEADPAN_SPEC.md#21-command-api-cli-and-extensibility) define the domain, dependency choices, and workspace contracts. This document maps them to the repository; it does not replace those contracts.

## Present implementation

The [shared render coordinator](RENDER_JOBS.md#shared-workflow-and-native-ownership)
connects immutable capture, encoding, verification, publication, checkpoint retry
and reconciliation to the native project service. One bounded stage worker owns
media and destination locks; the existing service owns journal transactions and
retains the writer through cancellation and release. Explicit subprocess teardown
evidence gates terminal failures. Automatic SDR qualification now retains an
immutable decision per fresh encoding attempt, committed with its Encoding
transition. Checkpoint retry and reconciliation preserve the original decision.
Native Render and the public headless commands use this automatic SDR workflow.
Native admission requires an explicit decision for temporary Camera, Gain and
Room tone previews; the headless path never silently commits them. Full
mastering/effects, HDR, complete output qualification and native persisted-job
recovery remain open.

The native writer also owns an [authenticated local endpoint](LIVE_PROJECT.md).
CLI edits and Render requests route to that existing owner after a real writer
lock conflict. Retention, relinking, exact caller source registration and SQLite
checkpoints use its [background preparation worker](IMPORT_PREPARATION.md), with
short writer commits, exact cancellation targets and retained operational
receipts. Clients retain the admitted owner and never rediscover or replay an
unknown delivery. Read-only inspection, dry runs and current-schema migration
validation continue to coexist independently.

[Native Trim](COMBINED_TRIM.md#native-trim) captures an eligible direct Source or
neutral unity Source Partition under an ordinary Sequence, its literal right
sibling or absence, and the exact session/revision/scope. The app retains four
accepted In/Out/Slip/Roll values and sends ordered adjustments to the project
service. A refusal preserves the last accepted tuple; clamped nudges are never
coalesced across their intermediate limits. The service admits immutable
Before/Proposed snapshots and retains one private `ApplySourceTrim` command with
its exact resources. Apply consumes it once; a durable receipt survives workspace
refresh failure. Zero intent creates no proposed snapshot or history.

The preview worker prepares exact outgoing/incoming junction pictures. The app
presents each pair atomically with its complete proposal identity. Apply requires
the fully acknowledged proposal and its current Proposed pair submitted at the
final viewer raster. The playback preparation worker supplies the
[admitted Edit waveform](EDIT_WAVEFORMS.md) for that inspection's absolute 48 kHz
sample window before limiting and monitor gain. Optional audition moves only the
draft's heard position; both editor cursors and the boundary pair stay fixed.
Waveform or audition failure does not block an otherwise valid picture proposal.
Preview/playback workers and widgets cannot mutate authored state. Original,
Sounds, composite targets and Edit Visual ranges refuse Trim entry. Broader target
admission and full product qualification remain open. Native evidence is recorded
separately from backend qualification in the
[Trim qualification record](qualification/native-trim-2026-10-01.md).

| Crate | Present responsibility | Boundary |
| --- | --- | --- |
| `deadpan-core` | Exact time, validated flat beat tree, immutable asset metadata, structural commands, JSON, and reversible patches. | Pure Rust domain logic, independent of the application and external systems. |
| `deadpan-store` | SQLite packages, immutable revisions, atomic edit/history writes, generation requests/attempts, render intents/attempts/checkpoints, explicit bundle acceptance, writer ownership, and shared Original/Generated/RenderCandidates object storage. | SQLite is authoritative. Request relevance changes with authored revisions; attempts and original locations remain operational. Acceptance rechecks selection, relevance and all retained objects. |
| `deadpan-plan` | Exact picture mappings, bounded audio spans and occurrence-bound processing stages, sequence duration indexes, compact repeat-run indexes, source-index selection, and deterministic inspection. | Immutable authored revision; no decoder, GPU handle, audio processing, or database connection. Audio separates final allocation, grid origin/rule, retained sampling and post-mapping fade progress. Bound physical owners evaluate current raw recipes on retained clocks, with independent current/retained-placement policy. |
| `deadpan-dsp` | Owned bounded planar PCM and the canonical pinned Signalsmith stretch engine behind a safe Rust/C++ boundary. | Worker-only preparation, fixed internal schedule, explicit recipe/engine identity, cooperative replay, no devices, decoding or cache publication. |
| `deadpan-output` | Bounded prepared-PCM queue, generation revocation, delivery-clock intervals, sleep/wake observation and a narrow macOS CPAL device boundary. | Headless kernel plus explicit hardware harness; full lifecycle recovery, rate conversion and release callback qualification remain open. |
| `deadpan-playback` | Immutable revision/source admission, canonical limited PCM preparation, bounded admitted Edit-window waveform analysis and a separate device controller. | Limited Original/edit audition and selection loops with bounded queues, warm verified caches and explicit clock/route failure. Full mastering, acoustic and performance qualification remain open. |
| `deadpan-audio` | Exact-phase source resampling, explicit speaker mixing, qualified PCM access, exact room-tone loops, continuous Preserve preparation, sampled-root transfer onto point grids, informational meters and finite oversampled limiting. | Preparation/analysis workers only; exact affine grids, intrinsic DSP history, transitive source/layout cache provenance and shared work/residency limits. Transfer applies old audibility before filtering and retains it at exact destination points, separately from creative fades. LimitedAudio verifies actual final f32 in absolute cache tiles and shares its bus/DSP deadline. Meters preserve PCM. A revision-aware provider supplies media without database coupling. The playback crate owns devices; full voice graph, group mix, encoded mastering and background cache scheduling remain open. |
| `deadpan-jobs` | Typed length-framed worker protocol, pure attempt lifecycle and validated checkpoints, bounded subprocess supervision, controlled hash-verified artifact snapshots, and exact bridge-generation planning. | Separate generation and render adapters share the process transport. Real MLX qualification uses this boundary in a developer harness. The store persists generation and render attempts. Render intent and canonical document hashing are shared with the host; app inference remains open. |
| `deadpan-models` | Native bridge qualification, retained conditioning, measured source spans, request/provenance binding, and immutable host provenance. | The host derives both masters from one native snapshot and binds pre-launch inputs. Admission-bearing Ready receipts require all six objects; the store owns explicit acceptance. |
| `deadpan-media` | Shared verified source snapshots, measured video/audio indexes, exact video seeks, private PCM range caches and paired bridge conversion. | Source decoding and cache I/O use media threads. Original sample clocks, skip/discard and measured durations remain explicit. Conversion retains its isolated helper and shared hard deadline. No authored mutation. |
| `deadpan-source` | Separate persistent descriptor-only FFmpeg video/audio contexts, raw metadata, backward video seeks, owned RGBA/I420 and original-rate f32 extraction, and bounded MP4 packet/table inspection. | Bounded cooperative calls, no secondary input opens, explicit SDR interpretation, original clocks and strict runtime pins. Fresh GOP decoding starts with a new codec context at the exact IDR; manual and ordinary AAC modes expose separate physical/presented observations. These are observations for the host validator, not export admission. Existing source header/table/packet bounds remain unchanged. |
| `deadpan-fileclone` | Safe descriptor-based APFS clone boundary. | Narrow private unsafe system call; copying, verification, publication and durability belong to the store. |
| `deadpan-render` | Shared SDR picture composition, ordered framing/clipping, aspect/rotation, display transform and owned encoder pixels. | Owned RGBA8 input, linear Rec.2020 working texture, bounded single-flight working readback and explicit limited-range Rec.709 I420 conversion; no decoder, encoder, timestamps or authored state. |
| `deadpan-media-worker` | Descriptor-only FFmpeg decode, exact interior RGB8 interpolation, FFV1 v3 encoding, and independent decoded-pixel/timing comparison. | One isolated process per conversion, pinned LGPL libraries, bounded native scratch, no worker paths or publication authority. |
| `deadpan-encode` | [Bounded native SDR encoding](NATIVE_ENCODING.md) from composed I420 and canonical stereo PCM to H.264/AAC MP4. | Safe descriptor owner, exact chronological inputs, one deadline, explicit hardware/software attempt and same-descriptor fast-start. No decoding, project/GPU ownership, automatic fallback, verification or publication. |
| `deadpan-app` | Native project workspace with `egui`/`eframe`, Metal, a single-writer service, separate import preparation and preview workers. | System Documents library creation, atomic full-Original initialization, protected baseline history, separate sound registration, root sound placement and explicit Hold allowances, same-original reuse, legacy compatibility, current-depth Split/Repeat/delete/Hold-duration and Retime commands, a combined Source Trim draft with paired pictures, waveform and audition, atomic Source/Hold pause insertion, immutable Original/Your edit frame inspection, limited Original/edit audition with selection loops, automatic SDR Render with explicit preview decisions, and authenticated existing-owner CLI routing with background media/checkpoint preparation. Full editing, mastered playback, complete export qualification and native persisted-job recovery remain open. |
| `deadpan-cli` | Versioned headless project and command operations, dry runs, history, plan/source-PCM inspection, committed picture preparation, isolated encoding/verification, destination publication and diagnostics. | Shared with the native host's `--headless` path. Audio and [picture hosts](PROJECT_PICTURES.md) bind an immutable revision to historical receipts and verified Original/Generated media. The [encoder picture host](EXPORT_PICTURES.md) adds exact output clocks, legal raster mapping, shared composition and one retained I420 result. The [render worker](RENDER_WORKER.md) isolates that producer and independently admits bounded raw output after clean teardown. The [encoded worker](ENCODED_RENDER.md) streams those pictures and canonical PCM into a private native MP4 candidate. The separate [finished-file verifier](FINISHED_FILE_VERIFICATION.md) binds complete structural/decode checks to those bytes; failure retains the candidate for retry. The [render job adapter](RENDER_JOBS.md) separates immutable capture, durable retention and fresh verification from short store transitions. The [publication host](RENDER_PUBLICATION.md) owns bounded destination copies, byte readback, historical provenance, exclusive report/movie renames and truthful post-commit outcomes. Durable publication recovery, automatic SDR decisions and native/public headless Render use the shared workflow. An authenticated existing owner handles edits, Render and background retention/relink/registration/checkpoint operations without replay after unknown delivery. Legacy Accepted/Still readers, full mixes/effects, HDR, complete output qualification and native persisted-job recovery remain open. |

Finished-file verification checks actual container/packet clocks and normal-rate
edit lists, every decoded picture, complete GOPs from fresh IDR decoder contexts,
and manual/ordinary AAC coverage and PCM agreement at fixed sample coordinates.
The host admits the bound report only after clean process teardown. No verifier
owns the encoder, project writer or destination. [Current production evidence](qualification/finished-file-verification-2026-09-29.md)
covers seven fresh MP4s; content/event-sync and hardware/runtime qualification
remain separate. Source limits cap input at 64 GiB, aggregate packets at one
million, headers at 16 MiB and individual packets at 16 MiB, with separate bounded
tables. These limits can reject candidates within encoder capacity. The separate
[publication host](RENDER_PUBLICATION.md) carries verified byte identity through
destination staging and readback, publishes a bounded local report, and treats
the movie rename as the commit point. The report and movie are separate atomic
renames; a failure after movie rename is reported as `PublishedUnconfirmed`.
The [render job boundary](RENDER_JOBS.md) adds immutable intent, retained encoded
checkpoints, restart interruption and explicit fresh verification retries.
The publication journal adds explicit durable destination recovery. Automatic
SDR admission is connected to the shared workflow and its native/public headless
Render entrypoints. Full mastering/effects, HDR, complete output qualification,
native persisted-job recovery and release acceptance remain required.

The foundation has typed Source/Sequence/Hold/Repeat/Retime nodes, stable nested occurrence identities, persistent marks with atomic edit transforms, sparse play override subtrees, automatic isolation for node edits through complete occurrence paths, an indexed structural picture plan, independent exact picture/audio mappings, and exact revision-aware boundary/named-mark range queries. Temporal attachments, effects, semantic editing through ranges, a full media engine, audio pipeline, and an app-managed inference worker remain open.

Current persistence uses database schema 52 and core document schema 43. Schemas
1 through 38 retain their validated migration path to the current schema. Unused
development schemas 39 through 51 are refused before writable open or migration
backup, under the user's session-scoped permission to break unused development
formats without migrations. Existing supported migrations remain intact.

At the earlier database-42/core-33 checkpoint, persistence migrated schemas 1
through 41. Schemas 39 through 41 validated existing history without rewriting
authored JSON or patches. Schema 39 gained empty render tables; schemas 39 and 40
gained empty publication tables. All gained an empty automatic encoder decision
table while preserving operational cells and rejecting new vocabulary in legacy
intents. Those historical paths do not grant current admission to schemas 39
through 51.

Database 38 uses frozen core 32, preserving Hold audio setters while rejecting node treatments and gain setters. Database 37 uses frozen core 31, preserving allowances while rejecting direct and occurrence Hold audio setters. Database 36 uses frozen core 30, preserving its sound routes and contextual command admission while rejecting later allowance state and commands. [Root sound events](SOUND_EVENTS.md#persisted-root-sounds) have durable commands and shared pre-master mixing; chronological root ripple edits preserve sample phase. [Explicit allowances](SOUND_EVENTS.md#persisted-root-sound-allowances) permit one root contribution through one concrete silent Hold or default Repeat gap. Nested ownership, send/tail allowances, the full voice graph and remaining structural sound transforms stay open. [Original moments](SOURCE_MOMENTS.md) use native Visual selection and atomic explicit-Sequence paste, and retain measured picture ranges and exact audio selections independently of the full source phase mapping. [Audio copy lineage](AUDIO_LINEAGE.md) retains explicit relationships through Split, occurrence isolation and durable history. [Owned timing bindings](OWNED_AUDIO_BINDINGS.md) have persisted clocks, compact capture and a normal plan/PCM consumer; their complete authoring lifecycle remains open. [Audio edge intent](AUDIO_EDGES.md) persists exact boundary choices and applies shared short fades after time mapping. [Source registration](SOURCE_REGISTRATION.md) binds measured indexes and metadata to historical assets and optionally inserts the full source atomically. [Automatic basis state](PRESENTATION_BASIS.md) and explicit canvas changes are authored transactions. [Connection-free import preparation](IMPORT_PREPARATION.md) separates copying, snapshot verification and qualification receipt construction from writer commits using session-bound opaque results. The authenticated native owner now routes CLI retention, relinking, exact source registration and database checkpoints through that preparation boundary. Native relink/checkpoint controls and full recovery UI remain open. Native source-preview evidence does not qualify a complete editing viewport.

Each direct or routed sound prepares complete input before current Hold
suppression. Its output gate combines that contribution's exact issuer allowances
with current Hold policy and authored edges on the root sample grid. Original
silence, other sounds, source exhaustion and retained routing gaps keep their
independent meanings; granting an allowance never manufactures media. Native
`:sound-allow` and `:sound-silence` capture the sound, project session, revision,
Edit frame and concrete issuer on entry, then submit a revision-bound command
through the project service. Allowance-only transactions recheck source admission.

[Framing](FRAMING.md) stores bounded static poses or explicit whole-host envelopes
on authored nodes. The plan retains exact owner clocks and provider-to-root scope
identity; the renderer composes those operations with every intermediate canvas
clip. Native Camera temporarily changes the retained stopped picture, then submits
one revision-guarded command through the project service. Its scope is currently
the selected root beat. Split preserves complete effect clocks; framed Ungroup
remains required. [Captured Hold geometry](CAPTURED_FRAMING.md) retains the input
composition before the live Hold and ancestor framing. Native root insertion
captures lower scopes, preserving exact source PTS and intermediate clips.

The [single-Original workflow](SINGLE_ORIGINAL.md) is an optional store profile,
not a restriction baked into every core primitive. One atomic initialization
binds the measured Original and full timeline to a protected baseline. Subsequent
V1 admissions reject extra picture sources; audio-only catalog registration and
dedicated generated-Hold acceptance retain their own boundaries. Native creation
resolves Documents through safe Foundation calls in an autorelease pool and does
all package work on the service thread. Existing generic projects migrate without
invented profile data, and native Open upgrades only through validated backups.

[Sound events](SOUND_EVENTS.md) defines the required ownership and integration
contract for external audio over existing picture time. Sound recipes must retain
their source phase through structural edits, enter processing at their declared
owner, and preserve scoped silence. Catalog registration does not yet provide
placement. The default effect order keeps each voice's continuous time/pitch
processing and scoped output gates before the group bus. Explicit aggregate
processing can use a mixed intrinsic operand, but its scalar output policy
cannot replace independent voice gates.

[Retained audio contexts](AUDIO_CONTEXT.md) capture complete raw audio trees,
exact source inputs and immutable asset contracts independently of picture.
The plan compiles them directly through the shared audio graph and refuses
picture evaluation. Audio preparation requires explicit context-aware source
admission, including cached dependencies. The headless host authenticates the
complete context against `ProjectStore::snapshot_at` before reading historical
receipts and verified originals. Context schemas 1 and 2 are standalone and
explicitly reject nonempty binding state. Normal document plans support
binding-aware rendering; atomic Source/Hold insertion under ordinary Sequence groups is implemented;
arbitrary nested insertion remains open.

[Physical audio domains](AUDIO_PHYSICAL_DOMAINS.md) borrow the exact plan and
seed processing and flattened-policy queries at one retained subtree or gap.
Their signed root grids can extend outside visible allocation. `StageAudio`
reads them through the shared preparation/cache path; a domain transfer exposes
that raw signal on a point grid without resetting absolute phase. Headless
domain inspection uses the same historical media host. It does not yet author
the live bindings needed for inserted-time edits.

[Audio definition output](AUDIO_DEFINITIONS.md) reads a captured Node, actual
Repeat default or configured Repeat gap directly on a local-zero point grid.
An unplayed gap definition has no invented preceding-play identity. Actual gap
projection retains that stable identity and its own local support, bounded by
the nearest physical clock. Definition scope brands
relative paths, nested stage descriptors and caches. This supplies a distinct
new-play recipe operand even when all existing plays are overridden; it does not
replace their physical-domain continuity or install a live binding.

[Owned recipe clocks](OWNED_AUDIO_CLOCKS.md) evaluate a physical definition in an
explicit signed root placement while the selected plan supplies its current raw
inputs and policies. Root spans retain definition scope and preparation uses the
same bounded controller. This supports the proposed use of Split's owned children
with retained timing indexes instead of a second graph of historical raw bodies.
Core 17/database 23 retain bounded clock and symbolic-phase intent with explicit
Repeat birth/survivor scope. The normal StageAudio path consumes those bindings,
including current crop support, independent silence and post-mapping virtual
fades. Root-only bypass and evaluation scope participate in intrinsic cache
identity. Pure capture preserves existing bindings and rejects nonempty Repeat
gaps. Atomic Source/Hold insertion under ordinary Sequence groups composes those clocks; Repeat/Retime insertion
and the complete authoring lifecycle remain open.

## Full component map

Section 24 defines boundaries, not an obligation to create empty crates. Introduce each component when its implementation needs isolation; related modules may remain combined initially.

| Component | Required responsibility | Status |
| --- | --- | --- |
| `deadpan-core` | Document/time types, nodes, anchors, occurrences, selectors, commands, reduction, validation, and serialization contracts. | Documents, timing, node/occurrence-targeted commands, inverse patches, persistent marks/edit transforms, sparse play overrides, and exact boundary queries implemented; temporal attachments and remaining domains open. |
| `deadpan-store` | Authoritative SQLite document/history, one writer, migrations, recovery, and asset ownership. | SQLite schema 52/core document schema 43, retained schema-1-through-38 migration and explicit refusal of unused development schemas 39 through 51, optional validated single-Original profile and protected full-source baseline, writer lock, durable transactions, request relevance, attempts/receipts/selection, interrupted recovery, checkpoints, verified object publication/readback, explicit bundle acceptance, managed/linked originals, relinking and qualified source registration implemented. Full asset lifecycle and application integration remain open. |
| `deadpan-plan` | Compile immutable revisions into indexed render plans and incremental fragments. | Picture mapping and bounded structural audio queries implemented; fragment reuse, attachments/effects and full preview/export integration open. |
| `deadpan-media` | Qualified FFmpeg/native probing, PTS indexing, bounded decoding, surfaces, encoding/mux interfaces. | Generated RGB-to-FFV1 conversion, persistent H.264/FFV1 source indexes/seeks and measured selected-stream qualification implemented. The store retains qualified indexes. The native workspace connects this bounded import path and the shared automatic SDR Render workflow. Full format coverage and complete playback/export qualification remain open. |
| `deadpan-render` | Shared GPU composition, framing, color, visual effects, and output transformations. | SDR RGBA and ordered framing/clipping implemented with actual Metal/CPU comparison. The [encoder pixel boundary](SDR_ENCODER_PIXELS.md) adds owned Rec.709 I420 from that composed working target. Automatic native/public SDR Render uses this picture boundary. HDR, physical display integration, remaining effects and complete export qualification remain open. |
| `deadpan-audio` | Audio master clock, sample-exact mixing/DSP, tails, and native device output. | Source preparation, room-tone loops, bounded continuous Preserve stages, authored edges, exact owner-clock node gain/mute, root sound-event mixing/gain and per-contribution Hold allowances, informational meters and finite limiting implemented. The playback/output crates own device integration. Complete voice graph, send/tail allowances, final mastering and effects remain open. |
| `deadpan-jobs` | Bounded scheduling, process supervision, cancellation, and versioned worker protocol. | Protocol/lifecycle, store checkpoints, and one-attempt subprocess supervision implemented; scheduler and app provider integration remain open. |
| `deadpan-analysis` | Local transcript/VAD/shot/target proposals, annotations, and corrections. | Planned. |
| `deadpan-models` | Pack verification/install, capability planning, AI requests, and candidate validation. | Native bridge media/provenance qualification and retained-input evidence feed durable store acceptance. Source/color context, pack management, audition and app integration remain open. |
| `deadpan-ui` | Panes, keyboard routing, focus, inspectors, audition, and accessibility. | Planned; source preview UI currently belongs to the app. |
| `deadpan-app` | Lifecycle, platform integration, document host, and command dispatch. | Native one-Original lifecycle and Documents library, bounded service mailboxes, visible contextual keys, whole-original reuse/history, frame inspection and the captured combined Source Trim draft described above. Generic legacy projects keep their original profile and media. Automatic SDR Render, explicit preview decisions and existing-owner CLI routing with background preparation are implemented. Complete editorial workflow, native persisted-job recovery and release acceptance remain open. |
| `deadpan-cli` | Headless validation/dump, revision-aware commands, render/plan inspection, benchmarks, diagnostics. | Project/command/history/migration, picture/audio-plan and source-PCM inspection, boundary resolution, isolated committed SDR encoding, finished-file structural/decode verification and library destination publication with historical provenance and explicit durable restart reconciliation implemented. Native/public headless Render use the automatic SDR policy. Existing-owner routing covers revision-bound edits, Render and worker-prepared retention/relink/source registration/checkpoints, with exact targets and preserved receipts. Full audio/effects/HDR, native persisted-job recovery, complete output qualification and benchmarks remain open. |
| `native/` | Narrow platform and DSP bridges with isolated unsafe lifetime handling. | `deadpan-process` provides bounded Darwin worker-group teardown with unreaped ownership and membership checks; `deadpan-fileclone` wraps descriptor cloning; `deadpan-filesystem` supplies descriptor-based APFS volume identity for publication recovery; `deadpan-media-worker` isolates generated-video conversion; `deadpan-source` owns persistent source decoding; `deadpan-dsp` binds the canonical worker stretch schedule. Full audio pipeline and device integration remain open. |
| `workers/` | Qualified private model runtime and provider adapters. | Planned. |
| `recipes/` | Versioned declarative starter gags built from ordinary primitives. | Planned. |
| `fixtures/` | Generated deterministic and rights-cleared real-media fixtures. | Planned. |
| `schemas/` | Versioned command, project dump, worker, and model-pack contracts. | Planned. |
| `packaging/` | Signed helper/model manifests, runtime bundles, notices, and distribution. | Planned. |
| `xtask/` | Build, bundle, verification, and test orchestration when needed. | Planned. |

## Dependency rules

`core` has no higher-layer dependency. `store` and `plan` depend on core; `store`
also uses the job protocol's typed request values and relevance vocabulary.
Media, rendering, and audio consume plans and media interfaces without mutating
documents. Jobs supervise workers; models and analysis submit jobs and return
proposals or candidates. The UI issues commands through the application host.

Every input follows the same intended route:

```text
gesture / menu / inspector / macro / CLI
  -> typed command and selector
  -> resolution against revision and context
  -> validation and optional preview
  -> atomic reversible transaction
  -> committed revision, invalidations, and job requests
```

Generation recipes are authored data; request versions and operational relevance
live outside document history. The host resolves dependency hashes before
submitting a complete reconciliation plan to the store. No model loading or
media analysis occurs inside the SQLite transaction. A command cannot perform
network work while holding a project lock. A render plan cannot contain a widget,
database transaction, or Python object. An asset record cannot own a decoder.
Narrow provider interfaces advertise actual capabilities and structured failure
modes. [Generation request storage](GENERATION_REQUESTS.md) describes the current
boundary and remaining integration.

## Contracts that guide implementation

- Time uses distinct typed coordinates and half-open ranges. Convert both frame boundaries from the same origin using ties-to-even at 48 kHz. Repeat duration is `plays * child + (plays - 1) * gap`; authored repeats remain structural.
- A `.deadpan` directory package holds the authoritative SQLite database and durable media. Its discovery manifest is not a competing mutable document. JSON inspection dumps are derived from revisions.
- One immutable render plan defines picture and audio semantics for preview and export. Quality tiers may change resolution or sampling, never timing, effect order, or authored content. Export pins one committed revision.
- The audio callback uses prepared buffers and performs no blocking I/O, allocation, logging, or model work. Audio is the transport clock; seek generations reject stale video and audio work.
- Hold insertion and picture generation are separate. A deterministic fallback commits immediately. Model outputs are candidates until explicitly accepted through a transaction; request bindings prevent stale results from overriding edits.
- Originals and accepted generation artifacts are durable while referenced by retained history. Proxies and derived analysis are evictable. Accepted projects remain playable/renderable without their generation model.
- Model and final-render work are process-isolated. Worker control messages are versioned and bounded; large media travels by validated artifact reference. Providers cannot introduce arbitrary executable code through model packs.

## Decisions and qualification

The specification has selected Rust, egui/eframe/wgpu, a narrow FFmpeg/native media adapter, local worker boundaries, SQLite, and one automatic render policy. Preserve those product decisions unless evidence establishes a conflict. [Dependency decisions](DEPENDENCIES.md) records the foundation's pins and outstanding qualification work.

Gate A must qualify actual media decode/seek/encode, Metal preview, audio DSP/output, model candidates, and private-runtime packaging. It also compares isolated Cutlass extraction with a direct media adapter. Record exact versions/commits, build configurations, licenses, codec capability, hardware/OS, measurements, and failures in decision records. Library choice or compilation alone is not qualification.

Model selection remains empirical: usable holds per minute under editing load decides the default, not MLX loyalty, Rust purity, weight size, or upstream throughput claims. The specification's performance figures remain targets until Deadpan's own measured reports establish results.
