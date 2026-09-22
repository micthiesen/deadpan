# Deadpan

**A keyboard-native editor for making a moment last considerably too long.**

Deadpan is a Rust-native macOS video editor designed around editable beats: pauses, repeats, reactions, selective audio emphasis, reframing, and local AI-generated holds. Timing and attention are the organizing ideas. The full design targets Apple Silicon and an eventual self-contained application.

This repository currently contains an **editing foundation**, not a working video editor:

- `deadpan-core`: exact time, validated beat documents, structural commands, nested occurrence edits, reversible transactions, and [generated Hold intent and retained sampling](docs/GENERATED_HOLDS.md).
- `deadpan-store`: SQLite project packages, persistent marks and undo/redo, monotonic generation requests, durable attempts and interrupted-job recovery, schema-1-through-14 migration to schema 15, checkpoints, verified media storage, and [explicit durable bundle acceptance](docs/GENERATION_ACCEPTANCE.md). Generic new generated-provider ingress remains guarded.
- `deadpan-plan`: immutable indexed picture mappings and [exact structural audio spans](docs/AUDIO_PLAN.md), with exact retiming, stable repeated-play identities, and sparse play overrides.
- `deadpan-jobs`: bounded worker messages, job lifecycle, subprocess supervision, contained artifact snapshots, and exact bridge-generation planning. A real MLX development adapter exercises this boundary; it is not connected to the app yet.
- `deadpan-media`: shared verified originals, measured video/audio indexes, exact video seeks and bounded original-rate PCM caches through `native/deadpan-source`, plus isolated FFV1 conversion and exact interior bridge sampling.
- `deadpan-dsp`: a bounded [worker-side canonical stretch adapter](docs/AUDIO_DSP.md) with owned PCM and a shared preview/export preparation schedule; device output remains open.
- `deadpan-audio`: [exact source resampling and explicit stereo mixing](docs/AUDIO_PREPARATION.md), [plan-driven source PCM](docs/SOURCE_STAGE_AUDIO.md), [continuous Preserve retimes](docs/AUDIO_STAGE_PREPARATION.md) and [room-tone loops](docs/ROOM_TONE_AUDIO.md) from verified originals; the full voice graph remains open.
- [Independent source audio timing](docs/SOURCE_AUDIO_MAPPING.md): exact audio durations and offsets, reversible commands, and per-play edits that preserve picture timing.
- [Exact source picture timing](docs/SOURCE_VIDEO_MAPPING.md): natural-rate frame selection independent of beat rounding, with persisted endpoint holding restricted to the selected trim.
- [Measured import timing](docs/SOURCE_IMPORT_TIMING.md): exact independent stream starts, preserved available audio, full-source enclosure and presentation-basis candidates.
- [Source registration](docs/SOURCE_REGISTRATION.md): live stream qualification, durable indexes and revision-bound receipts, atomic asset registration and optional insertion through the headless host. [Automatic presentation basis](docs/PRESENTATION_BASIS.md) selects the first primary picture before timed edits; later geometry changes preserve the fixed clock. Native relinking and recovery UI remain open.
- [Original media ownership](docs/ORIGINAL_MEDIA.md): durable managed originals, APFS clone/copy, linked locations, identity-checked relinking and private snapshots. [Background import preparation](docs/IMPORT_PREPARATION.md) separates file verification and measured receipts from writer commits, with session and freshness checks at admission.
- `deadpan-render`: shared SDR GPU picture baseline, linear Rec.2020 composition, source aspect/rotation and explicit sRGB display conversion.
- `deadpan-models`: [native bridge bundles](docs/GENERATION_BUNDLES.md), retained conditioning inputs, provenance binding, and measured media identities/spans. Admission-bearing schema-9 receipts require all six objects before Ready and explicit acceptance.
- `deadpan-app`: an `egui`/`eframe` [native project workspace](docs/NATIVE_WORKSPACE.md) using Metal, with background media import, source browsing, explicit whole-source insertion, durable undo/redo, and exact Source/Sequence frame inspection.
- `deadpan-cli`: headless project/command/history operations, migration, picture/audio-plan and source-PCM inspection, and exact boundary selection, also available through `deadpan-app --headless`.

The app implements a first create/import/insert/reopen workflow. Full structural keyboard editing, playback, AI generation, and export remain unimplemented in the app. All 24 full-product requirements remain open or partial in the [requirement tracker](docs/REQUIREMENTS.md). [Compatible native media qualification](docs/qualification/media-compatible-2026-09-20.md), [canonical audio qualification](docs/qualification/audio-canonical-2026-09-20.md), and a [real local model smoke](docs/qualification/model-smoke-2026-09-20.md) record actual tests, failed configurations, and measured limits separately from the application.

## Run the foundation

Development uses Rust 1.97.1, Cargo, and the macOS native build tools. The pinned toolchain is selected by `rust-toolchain.toml`.

```sh
cargo run -p deadpan-cli -- doctor
cargo run -p deadpan-app
```

The application opens a non-destructive source preview on macOS. Use `⌘O` to focus the video path, Enter to open it, and Left/Right or Home/End to inspect original frames. `--preview-source PATH` opens a source at launch. The current decoder admits explicitly tagged progressive 8-bit SDR H.264/FFV1 in MP4/Matroska; unsupported interpretations fail visibly. See [source preview qualification](docs/qualification/source-preview-2026-09-21.md) for measured scope and [Headless commands](docs/HEADLESS.md) for project operations. `doctor` reports the foundation, not release qualification. No credentials or model downloads are required.

Before running the app or complete workspace gate, build the pinned FFmpeg developer dependency
as described in [Development](docs/DEVELOPMENT.md). That document also covers native
smoke testing. The [nested occurrence](docs/OCCURRENCE_VERIFICATION.md),
[sparse override](docs/OVERRIDE_VERIFICATION.md), [picture plan and migration](docs/PLAN_MIGRATION_VERIFICATION.md), [editing foundation](docs/FOUNDATION_VERIFICATION.md), and [setup report](docs/SETUP_VERIFICATION.md) preserve earlier evidence. The initial product deployment target is Apple Silicon macOS 15, subject to dependency qualification; that target is not a tested release support claim.

## Specification and project map

The complete original design package is preserved in [`docs/spec`](docs/spec/README.md), with [source checksums](docs/SPEC_PROVENANCE.md):

- [Full product specification](docs/spec/DEADPAN_SPEC.md), the normative 33-section source.
- [PDF reading edition](docs/spec/DEADPAN_SPEC.pdf).
- [Implementation-agent handoff](docs/spec/AGENT_HANDOFF.md).
- [Keyboard reference](docs/spec/KEYBOARD_REFERENCE.md), specifying the planned editing language.

[Agent instructions](AGENTS.md) include the design philosophy and exact validation gate. [Architecture](docs/ARCHITECTURE.md) maps the current crates to the complete component design. [Requirements and delivery gates](docs/REQUIREMENTS.md) distinguish implemented foundations from the evidence still needed for release.

Work begins with dependency qualification and the pure editing foundation. Completing an early gate does not reduce the full specification or establish release readiness.
