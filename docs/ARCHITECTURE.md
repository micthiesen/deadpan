# Architecture

[Specification Sections 21, 23, and 24](spec/DEADPAN_SPEC.md#21-command-api-cli-and-extensibility) define the domain, dependency choices, and workspace contracts. This document maps them to the repository; it does not replace those contracts.

## Present implementation

| Crate | Present responsibility | Boundary |
| --- | --- | --- |
| `deadpan-core` | Exact time, validated flat beat tree, immutable asset metadata, structural commands, JSON, and reversible patches. | Pure Rust domain logic, independent of the application and external systems. |
| `deadpan-store` | SQLite packages, immutable revision snapshots, atomic edit/history writes, generation requests/attempts, undo/redo, writer ownership, checkpoints, and generated-object byte storage. | SQLite is authoritative. Request relevance changes with authored revisions; attempts remain operational. Publishing bytes is separate from acceptance. |
| `deadpan-plan` | Exact picture mappings, sequence duration indexes, compact repeat-run indexes, source-index selection, and deterministic inspection. | Immutable authored revision; no decoder, GPU handle, audio processing, or database connection. |
| `deadpan-jobs` | Typed length-framed worker protocol, pure attempt lifecycle and validated checkpoints, bounded subprocess supervision, contained hash-verified artifact snapshots, and exact bridge-generation planning. | Real MLX qualification uses this boundary in a developer harness. The store persists attempts; app inference and artifact promotion remain open. |
| `deadpan-media` | Private snapshot conversion through a bounded native helper, exact generated-video contracts, and final content identity. | Safe host code does not link codecs or mutate projects. The helper independently verifies its encoded FFV1 output. |
| `deadpan-media-worker` | Descriptor-only FFmpeg decode, FFV1 v3 encoding, and independent decoded-pixel/timing comparison. | One isolated process per conversion, pinned LGPL libraries, no worker paths or publication authority. |
| `deadpan-app` | Native development welcome shell with `egui`/`eframe` and `wgpu` on Metal. | Application entry point only; no authored document or media workflow yet. |
| `deadpan-cli` | Versioned headless project and command operations, dry runs, history, and diagnostics. | Shared with the native host's `--headless` path; no media rendering yet. |

The foundation has typed Source/Sequence/Hold/Repeat/Retime nodes, stable nested occurrence identities, persistent marks with atomic edit transforms, sparse play override subtrees, automatic isolation for node edits through complete occurrence paths, an indexed structural picture plan, and exact revision-aware boundary/named-mark range queries. Temporal attachments, effects, semantic editing through ranges, a media engine, audio pipeline, and an app-managed inference worker remain open. Persistence migrates database schemas 1 through 6 directly to schema 7 and core document schema 5; recovery UI, managed-media import, and host socket routing remain open. A native window is not a qualified media viewport.

## Full component map

Section 24 defines boundaries, not an obligation to create empty crates. Introduce each component when its implementation needs isolation; related modules may remain combined initially.

| Component | Required responsibility | Status |
| --- | --- | --- |
| `deadpan-core` | Document/time types, nodes, anchors, occurrences, selectors, commands, reduction, validation, and serialization contracts. | Documents, timing, node/occurrence-targeted commands, inverse patches, persistent marks/edit transforms, sparse play overrides, and exact boundary queries implemented; temporal attachments and remaining domains open. |
| `deadpan-store` | Authoritative SQLite document/history, one writer, migrations, recovery, and asset ownership. | SQLite schema 7, complete schema-1/2/3/4/5/6 migration, writer lock, durable transactions, generation request relevance, attempts/receipts/selection, interrupted-attempt recovery, checkpoints, and BLAKE3 generated-object publication/readback implemented; generated Hold intent is authored in core, while qualified acceptance and full asset lifecycle remain open. |
| `deadpan-plan` | Compile immutable revisions into indexed render plans and incremental fragments. | Picture mapping and indexed seeking implemented; fragment reuse, effects, audio, and actual preview/export integration open. |
| `deadpan-media` | Qualified FFmpeg/native probing, PTS indexing, bounded decoding, surfaces, encoding/mux interfaces. | Generated RGB-to-FFV1 conversion boundary implemented. General import, indexing, surfaces, playback, and export remain open. |
| `deadpan-render` | Shared GPU composition, framing, color, visual effects, and output transformations. | Planned. |
| `deadpan-audio` | Audio master clock, sample-exact mixing/DSP, tails, and native device output. | Planned. |
| `deadpan-jobs` | Bounded scheduling, process supervision, cancellation, and versioned worker protocol. | Protocol/lifecycle, store checkpoints, and one-attempt subprocess supervision implemented; scheduler and app provider integration remain open. |
| `deadpan-analysis` | Local transcript/VAD/shot/target proposals, annotations, and corrections. | Planned. |
| `deadpan-models` | Pack verification/install, capability planning, AI requests, and candidate validation. | Planned. |
| `deadpan-ui` | Panes, keyboard routing, focus, inspectors, audition, and accessibility. | Planned; welcome UI currently belongs to the app. |
| `deadpan-app` | Lifecycle, platform integration, document host, and command dispatch. | Development shell only. |
| `deadpan-cli` | Headless validation/dump, revision-aware commands, render/plan inspection, benchmarks, diagnostics. | Project/command/history/migration, picture-plan inspection, and boundary resolution implemented; rendering and benchmarks open. |
| `native/` | Narrow platform and DSP bridges with isolated unsafe lifetime handling. | `deadpan-process` qualifies Darwin zombie-only groups; `deadpan-media-worker` isolates generated-video conversion. General media/DSP application bridges remain open. |
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
