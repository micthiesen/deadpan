# Deadpan

**A keyboard-native editor for making a moment last considerably too long.**

Deadpan is a Rust-native macOS structural editor for massaging one original
video into a weird YTP. Start with the full video, then reshape it through
cuts, pauses, repeats, reframing, sound and local AI-generated Holds. Reuse
moments from that Original and add external audio effects. New native projects
belong in Documents/Deadpan.

Deadpan is still in development and is **not yet a usable video editor or
release candidate**. Implemented features have local verification evidence;
that does not establish the complete editing workflow or release acceptance.
The [full specification](docs/spec/DEADPAN_SPEC.md) remains normative.

## Current implementation

As of 2026-10-23, the [requirement tracker](docs/REQUIREMENTS.md) marks **8 of
24 requirements Complete and 16 Partial**, under the specification's local
verification scope (§29.1). Completed sections cover projects/recovery, exact
timing, model management, YouTube and local import, accepted-media portability,
personal-app packaging, and license/privacy/security requirements. Every
remaining requirement and Gate A through G remains in scope.

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
  scopes support qualified editing families; broader transforms and complete
  audio qualification remain open.
- **AI pictures:** native [AI Hold and extension workflows](docs/AI_HOLDS.md)
  insert a deterministic fallback, generate local candidates, compare variants,
  and require explicit undoable acceptance. Accepted pictures reopen and render
  without their model. Model installation and offline packs are implemented;
  resident-model and remaining performance work stay open.
- **Playback and export:** shared picture/DSP semantics, committed-revision
  [native and headless Render](docs/RENDER_JOBS.md), automatic SDR/HDR encoder
  admission, emitted-file verification, cancellation, retained attempts and
  destination recovery. Rendering is implemented; the full format/effect,
  mastering, HDR-display and workflow qualification remains incomplete.

Remaining work includes temporal and attached-sound scope combinations,
selectors and operators, framing/target controls, analysis/model refinement,
complete playback/export qualification, and measured latency/memory budgets.
The tracker records exact limits and evidence. Physical keyboard, IME,
VoiceOver speech, listening and other owner-only checks are recorded separately
there with steps; they do not silently reduce the specification.

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
requires neither Developer ID signing nor notarization. Local packaging
qualification does not establish support on another Mac or overall release
readiness.

[Development](docs/DEVELOPMENT.md) documents the appropriate checks and full
gate. Qualification records preserve executed identities, real-media results,
failed configurations, skipped checks and remaining limits. A build, generated
design image or downloaded model is not acceptance evidence.

## Specification and project map

- [Full product specification 1.1](docs/spec/DEADPAN_SPEC.md), the normative
  Markdown source; [specification provenance](docs/SPEC_PROVENANCE.md).
- [Original 1.0 archive](docs/spec/archive/1.0/), historical rather than current
  authority.
- [Requirements and delivery gates](docs/REQUIREMENTS.md), current status,
  evidence and owner-verification steps.
- [Implementation-agent handoff](docs/spec/AGENT_HANDOFF.md), current
  continuation and retained checkpoints.
- [Agent instructions](AGENTS.md), working agreement and project constraints.

Completing an early gate does not reduce the full specification.
