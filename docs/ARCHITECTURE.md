# Architecture

[Specification Sections 21, 23, and 24](spec/DEADPAN_SPEC.md#21-command-api-cli-and-extensibility) define the domain, dependency choices, and workspace contracts. This document maps them to the repository; it does not replace those contracts.

## Present implementation

| Crate | Present responsibility | Boundary |
| --- | --- | --- |
| `deadpan-core` | Exact time, validated flat beat tree, immutable asset metadata, structural commands, JSON, and reversible patches. | Pure Rust domain logic, independent of the application and external systems. |
| `deadpan-store` | SQLite packages, immutable revisions, atomic edit/history writes, generation requests/attempts, explicit bundle acceptance, writer ownership, checkpoints, and shared original/generated-object storage. | SQLite is authoritative. Request relevance changes with authored revisions; attempts and original locations remain operational. Acceptance rechecks selection, relevance and all retained objects. |
| `deadpan-plan` | Exact picture mappings, bounded audio spans and occurrence-bound processing stages, sequence duration indexes, compact repeat-run indexes, source-index selection, and deterministic inspection. | Immutable authored revision; no decoder, GPU handle, audio processing, or database connection. Audio separates final sample allocation from virtual point grids, retaining exact coordinates and Hold policies. |
| `deadpan-dsp` | Owned bounded planar PCM and the canonical pinned Signalsmith stretch engine behind a safe Rust/C++ boundary. | Worker-only preparation, fixed internal schedule, explicit recipe/engine identity, cooperative replay, no devices, decoding or cache publication. |
| `deadpan-audio` | Exact-phase source resampling, explicit speaker mixing, qualified PCM access and continuous Preserve preparation across nested structural stages. | Preparation workers only; exact affine grids, intrinsic DSP history, transitive source/layout cache provenance and shared work/residency limits. A revision-aware provider supplies media without database coupling. Full voice graph, background cache scheduling and devices remain open. |
| `deadpan-jobs` | Typed length-framed worker protocol, pure attempt lifecycle and validated checkpoints, bounded subprocess supervision, controlled hash-verified artifact snapshots, and exact bridge-generation planning. | Real MLX qualification uses this boundary in a developer harness. The store persists attempts; app inference remains open. |
| `deadpan-models` | Native bridge qualification, retained conditioning, measured source spans, request/provenance binding, and immutable host provenance. | The host derives both masters from one native snapshot and binds pre-launch inputs. Admission-bearing Ready receipts require all six objects; the store owns explicit acceptance. |
| `deadpan-media` | Shared verified source snapshots, measured video/audio indexes, exact video seeks, private PCM range caches and paired bridge conversion. | Source decoding and cache I/O use media threads. Original sample clocks, skip/discard and measured durations remain explicit. Conversion retains its isolated helper and shared hard deadline. No authored mutation. |
| `deadpan-source` | Separate persistent descriptor-only FFmpeg video/audio contexts, raw metadata, backward video seeks, owned RGBA and original-rate f32 extraction. | Bounded cooperative calls, no secondary input opens, explicit SDR interpretation, original clocks and strict runtime pins. Audio inventory remains a probe observation; selected audio decode supplies separate measured frame evidence. |
| `deadpan-fileclone` | Safe descriptor-based APFS clone boundary. | Narrow private unsafe system call; copying, verification, publication and durability belong to the store. |
| `deadpan-render` | Shared SDR picture composition, aspect/rotation and display transform. | Owned RGBA8 input, linear Rec.2020 working texture, one in-flight upload/render and no decoder or authored state. |
| `deadpan-media-worker` | Descriptor-only FFmpeg decode, exact interior RGB8 interpolation, FFV1 v3 encoding, and independent decoded-pixel/timing comparison. | One isolated process per conversion, pinned LGPL libraries, bounded native scratch, no worker paths or publication authority. |
| `deadpan-app` | Native project workspace with `egui`/`eframe`, Metal, a single-writer service, separate import preparation and preview workers. | Create/open, register-only import, explicit whole-source insertion, history, and immutable Source/Sequence frame inspection. Full editing, playback and export remain open. |
| `deadpan-cli` | Versioned headless project and command operations, dry runs, history, plan/source-PCM inspection and diagnostics. | Shared with the native host's `--headless` path. The audio host binds an immutable revision to historical receipts and verified originals; final mixes and export remain open. |

The foundation has typed Source/Sequence/Hold/Repeat/Retime nodes, stable nested occurrence identities, persistent marks with atomic edit transforms, sparse play override subtrees, automatic isolation for node edits through complete occurrence paths, an indexed structural picture plan, independent exact picture/audio mappings, and exact revision-aware boundary/named-mark range queries. Temporal attachments, effects, semantic editing through ranges, a full media engine, audio pipeline, and an app-managed inference worker remain open. Persistence migrates database schemas 1 through 14 directly to schema 15 and core document schema 10. [Source registration](SOURCE_REGISTRATION.md) binds measured indexes and metadata to historical assets and optionally inserts the full source atomically. [Automatic basis state](PRESENTATION_BASIS.md) and explicit canvas changes are authored transactions. [Connection-free import preparation](IMPORT_PREPARATION.md) separates copying, snapshot verification and qualification receipt construction from writer commits using session-bound opaque results. Native relinking, recovery UI and host socket routing remain open. Native source-preview evidence does not qualify a complete editing viewport.

## Full component map

Section 24 defines boundaries, not an obligation to create empty crates. Introduce each component when its implementation needs isolation; related modules may remain combined initially.

| Component | Required responsibility | Status |
| --- | --- | --- |
| `deadpan-core` | Document/time types, nodes, anchors, occurrences, selectors, commands, reduction, validation, and serialization contracts. | Documents, timing, node/occurrence-targeted commands, inverse patches, persistent marks/edit transforms, sparse play overrides, and exact boundary queries implemented; temporal attachments and remaining domains open. |
| `deadpan-store` | Authoritative SQLite document/history, one writer, migrations, recovery, and asset ownership. | SQLite schema 15, schema-1-through-14 migration, writer lock, durable transactions, request relevance, attempts/receipts/selection, interrupted recovery, checkpoints, verified object publication/readback, explicit bundle acceptance, managed/linked originals, relinking and qualified source registration implemented. Full asset lifecycle and application integration remain open. |
| `deadpan-plan` | Compile immutable revisions into indexed render plans and incremental fragments. | Picture mapping and bounded structural audio queries implemented; fragment reuse, attachments/effects and full preview/export integration open. |
| `deadpan-media` | Qualified FFmpeg/native probing, PTS indexing, bounded decoding, surfaces, encoding/mux interfaces. | Generated RGB-to-FFV1 conversion, persistent H.264/FFV1 source indexes/seeks and measured selected-stream qualification implemented. The store retains qualified indexes. The native workspace connects this bounded import path; full format matrix, playback and export remain open. |
| `deadpan-render` | Shared GPU composition, framing, color, visual effects, and output transformations. | SDR RGBA baseline implemented with actual Metal/CPU comparison. HDR, physical display integration, effects and encoder output remain open. |
| `deadpan-audio` | Audio master clock, sample-exact mixing/DSP, tails, and native device output. | Source preparation and bounded continuous Preserve stages implemented; complete voice graph, clock, effects and device output remain open. |
| `deadpan-jobs` | Bounded scheduling, process supervision, cancellation, and versioned worker protocol. | Protocol/lifecycle, store checkpoints, and one-attempt subprocess supervision implemented; scheduler and app provider integration remain open. |
| `deadpan-analysis` | Local transcript/VAD/shot/target proposals, annotations, and corrections. | Planned. |
| `deadpan-models` | Pack verification/install, capability planning, AI requests, and candidate validation. | Native bridge media/provenance qualification and retained-input evidence feed durable store acceptance. Source/color context, pack management, audition and app integration remain open. |
| `deadpan-ui` | Panes, keyboard routing, focus, inspectors, audition, and accessibility. | Planned; source preview UI currently belongs to the app. |
| `deadpan-app` | Lifecycle, platform integration, document host, and command dispatch. | Native project lifecycle, bounded service mailboxes, explicit source insertion/history and frame inspection. Complete editorial workflow remains open. |
| `deadpan-cli` | Headless validation/dump, revision-aware commands, render/plan inspection, benchmarks, diagnostics. | Project/command/history/migration, picture/audio-plan and source-PCM inspection, and boundary resolution implemented; final rendering and benchmarks open. |
| `native/` | Narrow platform and DSP bridges with isolated unsafe lifetime handling. | `deadpan-process` qualifies Darwin zombie-only groups; `deadpan-fileclone` wraps descriptor cloning; `deadpan-media-worker` isolates generated-video conversion; `deadpan-source` owns persistent source decoding; `deadpan-dsp` binds the canonical worker stretch schedule. Full audio pipeline and device integration remain open. |
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
