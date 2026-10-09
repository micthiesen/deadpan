# Deadpan

**A keyboard-native editor for making a moment last considerably too long.**

Deadpan is a Rust-native macOS editor for massaging one original video into a
weird YTP. Start with the full video, then reshape it through cuts, pauses,
repeats, reactions, reframing and local AI-generated holds. Reuse moments from
that same original and add external sound effects. New native projects belong
in Documents/Deadpan. The full design targets Apple Silicon and an eventual
self-contained application.

[Specification 1.1](docs/spec/DEADPAN_SPEC.md) defines this focused V1 workflow.
The original design is [preserved unchanged](docs/spec/archive/1.0/); useful
generic backend behavior and legacy multi-video projects remain supported.

This repository currently contains an **editing foundation**, not a working video editor:

- `deadpan-core`: exact time, validated beat documents, structural commands, nested occurrence edits, [audio copy lineage](docs/AUDIO_LINEAGE.md), reversible transactions, and [generated Hold intent and retained sampling](docs/GENERATED_HOLDS.md).
- `deadpan-store`: SQLite project packages, a [protected full-original baseline](docs/SINGLE_ORIGINAL.md), persistent marks and undo/redo, monotonic generation requests, durable attempts and interrupted-job recovery, schema-1-through-39 migration to schema 40, checkpoints, verified media storage, and [explicit durable bundle acceptance](docs/GENERATION_ACCEPTANCE.md). Generic new generated-provider ingress remains guarded.
- `deadpan-plan`: immutable indexed picture mappings and [exact structural audio spans](docs/AUDIO_PLAN.md), with separate [sample clocks and retained envelopes](docs/AUDIO_SAMPLING.md), [frozen reference policy clocks](docs/AUDIO_REFERENCE.md), [retained audio contexts](docs/AUDIO_CONTEXT.md), exact retiming, stable repeated-play identities, and sparse play overrides.
- `deadpan-jobs`: bounded worker messages, job lifecycle, subprocess supervision, contained artifact snapshots, and exact bridge-generation planning. A real MLX development adapter exercises this boundary; it is not connected to the app yet.
- [Durable render attempts](docs/RENDER_JOBS.md): immutable historical intent, retained complete encoded movies, interrupted-attempt recovery and explicit fresh verification retries. Destination crash reconciliation and the native Render workflow remain open.
- `deadpan-media`: shared verified originals, measured video/audio indexes, exact video seeks and bounded original-rate PCM caches through `native/deadpan-source`, plus isolated FFV1 conversion and exact interior bridge sampling.
- `deadpan-dsp`: a bounded [worker-side canonical stretch adapter](docs/AUDIO_DSP.md) with owned PCM and a shared preview/export preparation schedule.
- `deadpan-output`: a bounded [prepared-PCM queue and narrow macOS device adapter](docs/AUDIO_OUTPUT.md), generation revocation, delivery-clock intervals and owned sleep/wake observation. Full device and acoustic qualification remain open.
- `deadpan-playback`: [Original and edit audition](docs/PLAYBACK.md) from immutable source receipts, with separate preparation/control workers, bounded canonical PCM and explicit device-clock failures. The app exposes Space Play/Pause, Shift+Space selection loops with adjustable context, and independent monitor volume; full mastering remains open.
- `deadpan-audio`: [exact source resampling and explicit stereo mixing](docs/AUDIO_PREPARATION.md), [plan-driven source PCM](docs/SOURCE_STAGE_AUDIO.md), [continuous Preserve retimes](docs/AUDIO_STAGE_PREPARATION.md), [room-tone loops](docs/ROOM_TONE_AUDIO.md), [sampled-root transfer](docs/AUDIO_SIGNAL_TRANSFER.md) and [authored edge fades](docs/AUDIO_EDGES.md) and [owned timing bindings](docs/OWNED_AUDIO_BINDINGS.md) from verified originals; the full voice graph remains open.
- [Root sound events](docs/SOUND_EVENTS.md#native-root-placement): durable qualified recipes, native catalog placement, exact sample positioning and frame nudges, per-event edges/gain and shared limited mixing. Root ripple edits retain chronological sample phase. Each root contribution can explicitly play through one concrete silent Hold or default Repeat gap. Nested ownership, send/tail allowances, the full voice graph and remaining structural sound transforms stay open.
- [Independent source audio timing](docs/SOURCE_AUDIO_MAPPING.md): exact audio durations and offsets, reversible commands, and per-play edits that preserve picture timing.
- [Node gain and mute](docs/AUDIO_GAIN.md): reversible owner recipes and exact envelopes after time/pitch and edges, before the shared limiter. CLI authored-bus inspection exposes the result; native gain controls and Before/Draft audition remain open.
- [Exact source picture timing](docs/SOURCE_VIDEO_MAPPING.md): natural-rate frame selection independent of beat rounding, with persisted endpoint holding restricted to the selected trim.
- [Measured import timing](docs/SOURCE_IMPORT_TIMING.md): exact independent stream starts, preserved available audio, full-source enclosure and presentation-basis candidates.
- [Exact Original moments](docs/SOURCE_MOMENTS.md): measured VFR range candidates and fractional audio selections that retain the full source phase and stop at the selected boundary. Native `v`/`y` selection and `p`/`P` paste use one reversible Source splice in the selected Sequence group.
- [Source registration](docs/SOURCE_REGISTRATION.md): live stream qualification, durable indexes and revision-bound receipts, atomic asset registration and optional insertion through the headless host. [Automatic presentation basis](docs/PRESENTATION_BASIS.md) selects the first primary picture before timed edits; later geometry changes preserve the fixed clock. Native relinking and recovery UI remain open.
- [Original media ownership](docs/ORIGINAL_MEDIA.md): durable managed originals, APFS clone/copy, linked locations, identity-checked relinking and private snapshots. [Background import preparation](docs/IMPORT_PREPARATION.md) separates file verification and measured receipts from writer commits, with session and freshness checks at admission.
- `deadpan-render`: shared SDR GPU picture baseline, linear Rec.2020 composition, source aspect/rotation and explicit sRGB display conversion.
- `deadpan-models`: [native bridge bundles](docs/GENERATION_BUNDLES.md), retained conditioning inputs, provenance binding, and measured media identities/spans. Admission-bearing schema-9 receipts require all six objects before Ready and explicit acceptance.
- `deadpan-app`: an `egui`/`eframe` [native project workspace](docs/NATIVE_WORKSPACE.md) using Metal, with one-video creation in Documents/Deadpan, automatic full-original initialization, a separate audio catalog, same-original reuse, [current-depth Split](docs/STRUCTURAL_SPLIT.md), [silent pause insertion](docs/INSERT_TIME.md), repeat/delete/Hold-duration commands, [exact speed and pitch editing](docs/RETIME_EDITING.md), durable undo/redo, visible key hints and exact Original/Your edit frame inspection. Generic projects retain their broader compatibility workspace.
- `deadpan-cli`: headless project/command/history operations, migration, picture/audio-plan and source-PCM inspection, [physical audio contexts](docs/AUDIO_PHYSICAL_DOMAINS.md), [authored audio definitions](docs/AUDIO_DEFINITIONS.md), [owned recipes in explicit clocks](docs/OWNED_AUDIO_CLOCKS.md), and exact boundary selection, also available through `deadpan-app --headless`.

The app implements a local-video-to-full-timeline workflow with reversible current-depth edits, root sound placement and explicit silent-Hold allowances, limited Original/edit/sound audition with selection loops, and legacy reopening. The [shared limiter](docs/AUDIO_MASTERING.md) follows the current edge-faded bus; voice effects and the full mix remain incomplete. Arbitrary range cuts/replacement, persistent registers, nested sound ownership and Repeat/Retime sound transforms, send/tail allowances, full structural keyboard editing, mastered playback, app AI generation, YouTube acquisition and export remain open. All 24 full-product requirements remain open or partial in the [requirement tracker](docs/REQUIREMENTS.md). [Compatible native media qualification](docs/qualification/media-compatible-2026-09-20.md), [canonical audio qualification](docs/qualification/audio-canonical-2026-09-20.md), and a [real local model smoke](docs/qualification/model-smoke-2026-09-20.md) record actual tests, failed configurations, and measured limits separately from the application.

The [interface design boards](docs/design/README.md) contain the imagegen workspace,
screens and interaction targets, with exact prompts and reviewed behavior notes.
[Authored framing and Camera](docs/FRAMING.md) provide static/enveloped framing,
counted pan/zoom, numeric fields and temporary previews against the dedicated
Camera board. [Captured pause views](docs/CAPTURED_FRAMING.md) preserve prior
crops independently of new Camera settings. Saved targets, tracking and the
complete picture-operation surface remain required.

## Run the foundation

Development uses Rust 1.97.1, Cargo, and the macOS native build tools. The pinned toolchain is selected by `rust-toolchain.toml`.

```sh
cargo run -p deadpan-cli -- doctor
cargo run -p deadpan-app
```

Use `⌘N` to choose the one original video. Deadpan creates a package in
Documents/Deadpan and opens the full video on Your edit. Use `⌘O` to reopen a
project, `⌘I` to add audio to its sound catalog, and `,i` to reuse the full
original after the selected beat. For a moment, use `:source`, `v`, move with
`h/l`, and `y`; return with `:sequence`, then `p` after or `P` before a beat.
The Out boundary is excluded; copying leaves the Original intact. In Your edit, `3rr` wraps the
selected beat in three total plays, `dd` deletes it, and `u` undoes the edit.
`Enter` opens an ordinary Sequence group; `Backspace` returns to its parent.
The [group breadcrumbs](docs/GROUP_NAVIGATION.md) keep the editing depth visible.
Undo stops at the full-original baseline.

For a catalog sound, `,s` or `:sound-place` places its whole measured span at the
retained edit cursor without changing picture duration. In Placed sounds,
`j/k` selects, `h/l` nudges by exact project frames, Enter opens fine 48 kHz sample
positioning, `+` / `-` adjusts gain by 3 dB, and `dd` removes the event. Accepting
the unchanged position preserves fractional phase; `:sound-at N` explicitly
sets a whole sample onset. Routed events
retain cuts through gain/edge changes and reject moves. Overflow fails visibly.
At a silent Hold or default Repeat gap, `:sound-allow` permits the selected
effect in that exact occurrence; `:sound-silence` revokes the allowance. Command
entry captures the sound, project session, revision, Edit frame and issuer.
Each contribution keeps its complete input before applying its own Hold gates;
the Original and other sounds retain their silence. An allowance cannot add
media to a retained routing gap.
See the [native placement qualification record](docs/qualification/native-sound-placement-2026-09-27.md)
for checks and limits.

Use `?` or `:help` for the implemented
keyboard vocabulary. Legacy projects keep register/insert compatibility behavior.
`--project PATH` opens a
project at launch; `--preview-source PATH` opens a non-destructive source preview.
Qualified sources include explicitly tagged SDR H.264, HEVC and VP9 in MP4,
SDR VP9 with mono/stereo Opus in WebM/Matroska, standalone Opus sound in those
containers, mono/stereo MP3 sounds, FFV1 in Matroska, and the HDR/interlaced
cases described in
[source admission](docs/SOURCE_ADMISSION.md). Unsupported interpretations fail
visibly. See [Headless commands](docs/HEADLESS.md) for project operations.
`doctor` reports the foundation, not release qualification. No credentials or
model downloads are required.

Before running the app or complete workspace gate, build the pinned FFmpeg developer dependency
as described in [Development](docs/DEVELOPMENT.md). That document also covers native
smoke testing. The [nested occurrence](docs/OCCURRENCE_VERIFICATION.md),
[sparse override](docs/OVERRIDE_VERIFICATION.md), [picture plan and migration](docs/PLAN_MIGRATION_VERIFICATION.md), [editing foundation](docs/FOUNDATION_VERIFICATION.md), and [setup report](docs/SETUP_VERIFICATION.md) preserve earlier evidence. The initial product deployment target is Apple Silicon macOS 15, subject to dependency qualification; that target is not a tested release support claim.

## Specification and project map

The current design and unchanged original archive are in [`docs/spec`](docs/spec/README.md), with [source checksums and revision provenance](docs/SPEC_PROVENANCE.md):

- [Full product specification](docs/spec/DEADPAN_SPEC.md), the normative 33-section source.
- [Original 1.0 PDF reading edition](docs/spec/archive/1.0/DEADPAN_SPEC.pdf), historical; current 1.1 authority is Markdown.
- [Implementation-agent handoff](docs/spec/AGENT_HANDOFF.md).
- [Keyboard reference](docs/spec/KEYBOARD_REFERENCE.md), specifying the planned editing language.

[Agent instructions](AGENTS.md) include the design philosophy and exact validation gate. [Architecture](docs/ARCHITECTURE.md) maps the current crates to the complete component design. [Requirements and delivery gates](docs/REQUIREMENTS.md) distinguish implemented foundations from the evidence still needed for release.

Work begins with dependency qualification and the pure editing foundation. Completing an early gate does not reduce the full specification or establish release readiness.
