# Architecture

[Specification Sections 21, 23, and 24](spec/DEADPAN_SPEC.md#21-command-api-cli-and-extensibility) define the domain, dependency choices, and workspace contracts. This document maps them to the repository; it does not replace those contracts.

## Present implementation

| Crate | Present responsibility | Boundary |
| --- | --- | --- |
| `deadpan-core` | Exact time, validated flat beat tree, immutable asset metadata, structural commands, JSON, and reversible patches. | Pure Rust domain logic, independent of the application and external systems. |
| `deadpan-store` | SQLite packages, immutable revision snapshots, atomic edit/history writes, undo/redo, writer ownership, and backup API checkpoints. | SQLite is authoritative; exported JSON is inspection only. |
| `deadpan-plan` | Exact picture mappings, sequence duration indexes, compact repeat-run indexes, source-index selection, and deterministic inspection. | Immutable authored revision; no decoder, GPU handle, audio processing, or database connection. |
| `deadpan-jobs` | Typed length-framed worker protocol, pure attempt lifecycle, bounded subprocess supervision, and contained hash-verified artifact snapshots. | Candidates need independent media validation and explicit acceptance; no model backend, job persistence, or artifact promotion yet. |
| `deadpan-app` | Native development welcome shell with `egui`/`eframe` and `wgpu` on Metal. | Application entry point only; no authored document or media workflow yet. |
| `deadpan-cli` | Versioned headless project and command operations, dry runs, history, and diagnostics. | Shared with the native host's `--headless` path; no media rendering yet. |

The foundation has typed Source/Sequence/Hold/Repeat/Retime nodes, stable nested occurrence identities, persistent marks with atomic edit transforms, sparse play override subtrees, automatic isolation for node edits through complete occurrence paths, an indexed structural picture plan, and exact revision-aware boundary/named-mark range queries. Temporal attachments, effects, semantic editing through ranges, a media engine, audio pipeline, and inference worker remain open. Persistence migrates schema-1, schema-2, and schema-3 histories directly to schema 4; recovery UI, managed-media import, and host socket routing remain open. A native window is not a qualified media viewport.

## Full component map

Section 24 defines boundaries, not an obligation to create empty crates. Introduce each component when its implementation needs isolation; related modules may remain combined initially.

| Component | Required responsibility | Status |
| --- | --- | --- |
| `deadpan-core` | Document/time types, nodes, anchors, occurrences, selectors, commands, reduction, validation, and serialization contracts. | Documents, timing, node/occurrence-targeted commands, inverse patches, persistent marks/edit transforms, sparse play overrides, and exact boundary queries implemented; temporal attachments and remaining domains open. |
| `deadpan-store` | Authoritative SQLite document/history, one writer, migrations, recovery, and asset ownership. | SQLite schema 4, complete schema-1/2/3 history migration, writer lock, durable transactions, and checkpoints implemented; full lifecycle open. |
| `deadpan-plan` | Compile immutable revisions into indexed render plans and incremental fragments. | Picture mapping and indexed seeking implemented; fragment reuse, effects, audio, and actual preview/export integration open. |
| `deadpan-media` | Qualified FFmpeg/native probing, PTS indexing, bounded decoding, surfaces, encoding/mux interfaces. | Planned. |
| `deadpan-render` | Shared GPU composition, framing, color, visual effects, and output transformations. | Planned. |
| `deadpan-audio` | Audio master clock, sample-exact mixing/DSP, tails, and native device output. | Planned. |
| `deadpan-jobs` | Bounded scheduling, process supervision, cancellation, and versioned worker protocol. | Protocol/lifecycle and one-attempt subprocess supervision implemented; scheduler, persistence/recovery, and actual provider integration remain open. |
| `deadpan-analysis` | Local transcript/VAD/shot/target proposals, annotations, and corrections. | Planned. |
| `deadpan-models` | Pack verification/install, capability planning, AI requests, and candidate validation. | Planned. |
| `deadpan-ui` | Panes, keyboard routing, focus, inspectors, audition, and accessibility. | Planned; welcome UI currently belongs to the app. |
| `deadpan-app` | Lifecycle, platform integration, document host, and command dispatch. | Development shell only. |
| `deadpan-cli` | Headless validation/dump, revision-aware commands, render/plan inspection, benchmarks, diagnostics. | Project/command/history/migration, picture-plan inspection, and boundary resolution implemented; rendering and benchmarks open. |
| `native/` | Narrow platform and DSP bridges with isolated unsafe lifetime handling. | `deadpan-process` qualifies Darwin zombie-only worker groups with a bounded libproc adapter. Media/DSP application bridges remain planned. |
| `workers/` | Qualified private model runtime and provider adapters. | Planned. |
| `recipes/` | Versioned declarative starter gags built from ordinary primitives. | Planned. |
| `fixtures/` | Generated deterministic and rights-cleared real-media fixtures. | Planned. |
| `schemas/` | Versioned command, project dump, worker, and model-pack contracts. | Planned. |
| `packaging/` | Signed helper/model manifests, runtime bundles, notices, and distribution. | Planned. |
| `xtask/` | Build, bundle, verification, and test orchestration when needed. | Planned. |

## Dependency rules

`core` has no higher-layer dependency. `store` and `plan` depend on core. Media, rendering, and audio consume plans and media interfaces without mutating documents. Jobs supervise workers; models and analysis submit jobs and return proposals or candidates. The UI issues commands through the application host.

Every input follows the same intended route:

```text
gesture / menu / inspector / macro / CLI
  -> typed command and selector
  -> resolution against revision and context
  -> validation and optional preview
  -> atomic reversible transaction
  -> committed revision, invalidations, and job requests
```

Job requests are data in the core. A command cannot perform network work while holding a project lock. A render plan cannot contain a widget, database transaction, or Python object. An asset record cannot own a decoder. Narrow provider interfaces advertise actual capabilities and structured failure modes.

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
