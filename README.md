# Deadpan

**A keyboard-native editor for making a moment last considerably too long.**

Deadpan is a Rust-native macOS structural editor for massaging one original
video into a weird YTP. Start with the full video, then reshape it through
cuts, pauses, repeats, reframing, sound and local AI-generated Holds. Reuse
moments from that Original and add external audio effects. New native projects
belong in Documents/Deadpan.

[The brief](docs/BRIEF.md) says what Deadpan must do and when it is done. It
replaced the full specification and its requirement tracker on 2026-11-10.

## Current implementation

The current application and shared command path include:

- **Projects and media:** one-Original initialization with a protected full-source
  Undo baseline; local and [YouTube import](docs/YOUTUBE_IMPORT.md); separate
  external-audio catalog; managed or linked media; durable history, named takes,
  backups and [recovery/relinking](docs/RECOVERY.md).
- **Structural editing:** Source, Sequence, Hold, Repeat and Retime edits, exact
  cuts and replacement, scoped occurrence edits, [persistent named
  registers](docs/NAMED_REGISTERS.md), recording, named calls and
  [semantic dot-repeat](docs/SEMANTIC_MACROS.md). Whole-node, interval and
  transcript-search sets retain separate components rather than filling their
  gaps.
- **Picture and analysis:** [Camera and authored framing](docs/FRAMING.md),
  captured pause views, saved targets and [selected-target
  tracking](docs/TRACKING.md); local [transcription](docs/TRANSCRIPTION.md),
  correction, speech-activity and shot proposals.
- **Audio:** Original/edit/sound [audition](docs/PLAYBACK.md), independent monitor
  volume, sound placement, gain/mute, time/pitch processing, room tone,
  [EQ and filters](docs/EQUALIZER.md), saturation, tails and
  [continuous wet returns](docs/FINITE_WET_EFFECTS.md). Retained sound-processing
  scopes support the listed editing families; other combinations refuse with a
  message naming the alternative.
- **AI pictures:** native [AI Hold and extension workflows](docs/AI_HOLDS.md)
  insert a deterministic fallback, generate local candidates, compare variants,
  and require explicit undoable acceptance. Accepted pictures reopen and render
  without their model. Model installation and offline packs are implemented.
- **Playback and export:** shared picture/DSP semantics, committed-revision
  [native and headless Render](docs/RENDER_JOBS.md), automatic SDR/HDR encoder
  admission, emitted-file verification, cancellation, retained attempts and
  destination recovery.

Remaining work is the finish list in [the brief](docs/BRIEF.md).

The native [interface boards](docs/design/README.md) and
[identity assets](docs/design/brand/README.md) remain the visual targets.
[Architecture](docs/ARCHITECTURE.md) maps the implemented crate boundaries.
[Performance](docs/PERFORMANCE.md) distinguishes direct store measurements,
offscreen GPU composition and physical-display claims.

## Run the development build

Use Rust 1.97.1 from rust-toolchain.toml and the macOS native build tools.
First build the pinned FFmpeg developer dependency and configure its prefix as
described in [Development](docs/DEVELOPMENT.md).

    cargo run --locked -p deadpan-cli -- doctor
    cargo run --locked -p deadpan-app

The [fast feedback guide](docs/BUILD_FEEDBACK.md) separates routine incremental
builds from release builds. Prefer its installed Blaze command for repeated
pure Core library body edits. Plan, integration tests and the native app/media
stay on Cargo; the native Blaze JIT paths currently panic.

Use ⌘N to choose one local Original, or the start surface's YouTube URL field
to inspect a video before import. ⌘O reopens a project; ⌘I adds audio to the sound
catalog. New projects open with the complete Original on Your edit.

For a source moment, use :source, v, h/l and y; return with :sequence and
paste with p/P. The Out boundary is excluded. ,i reuses the full Original.
In Your edit, 3rr wraps the selected beat in three total plays, dd deletes it,
and u undoes. Enter opens an ordinary Sequence group; Backspace returns to its
parent. Undo stops at the full-source baseline. See
[Original moments](docs/SOURCE_MOMENTS.md) and
[group navigation](docs/GROUP_NAVIGATION.md).

Space plays/pauses, Shift+Space loops the selection, and ⌘E opens Render.
Use ? or :help for the implemented vocabulary and current scope;
[keymaps](docs/KEYMAP.md) are configurable. See
[headless commands](docs/HEADLESS.md) for revision-aware inspection and editing,
also available through deadpan-app --headless.

Ordinary local editing needs no model download. AI generation uses an explicitly
installed model pack; YouTube import uses verified downloader helpers.
[Source admission](docs/SOURCE_ADMISSION.md) records supported media
interpretations and truthful refusals. [Development formats](docs/DEVELOPMENT_FORMATS.md)
defines the current package format: obsolete development schemas are refused
without mutation, rather than implicitly migrated. Supported-format generic
projects retain the compatibility workspace.

## Bundle and verification

[cargo xtask bundle](docs/PACKAGING.md) builds a self-contained, relocatable
Deadpan.app with its pinned helpers and runtime resources; bundle-verify checks
a scrubbed relocated copy. The personal app uses ad hoc or local signing and
requires neither Developer ID signing nor notarization.

[Development](docs/DEVELOPMENT.md) documents the checks and the gate.

## Project map

- [Brief](docs/BRIEF.md): what Deadpan must do, the finish list and the
  definition of done.
- [Agent instructions](AGENTS.md): how to build, verify and work in this
  repository.
- [Commands](docs/COMMANDS.md): every action, generated from the registry.
- [Architecture](docs/ARCHITECTURE.md) and the per-area documents under
  `docs/`.
