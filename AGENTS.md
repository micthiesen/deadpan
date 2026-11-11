# Deadpan

Deadpan is a native macOS, keyboard-first structural editor for turning one
original video into a weird YTP. One person uses it on one Mac.

## What to build

[The brief](docs/BRIEF.md) is the only requirements document. Read it before
any work. Scope is capped: what exists is the feature set, the Finish and
Simplify lists are finite, and unsupported combinations refuse with a message
instead of being extended. The old specification, requirements ledger,
progress score, gates and qualification records were dropped on 2026-11-10
(owner decision); do not restore them or write evidence documents.

Standing owner decisions:

- Personal app, used on this Mac. No Apple developer account: never require
  Developer ID signing or notarization. Sign ad hoc or with the dotfiles local
  identity (`~/.dotfiles/scripts/macos-sign.py`).
- No users and no existing projects worth keeping. Breaking project-format
  and schema changes are fine; write no migrations or compatibility code.
- The owner tests only when the brief's Done section is met. Until then,
  replay frames on real footage and the acceptance session stand in for a
  person. Do not ask product questions; choose, and say what you chose.
- Kestrel owns Cmd+Return and other global shortcuts. Whole-Original reuse is
  `,i` or `:insert`. Any shortcut change follows
  [keybinding compatibility](docs/KEYBINDING_COMPATIBILITY.md).
- GitHub CI is manual only. Verify locally and push without waiting.

## Build

Rust 1.97.1 (`rust-toolchain.toml`), Apple Silicon macOS. Every Cargo command
needs the pinned native prefixes:

```sh
export DEADPAN_FFMPEG_PREFIX="$HOME/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix"
export DEADPAN_ORT_PREFIX="$HOME/Library/Developer/Deadpan/onnxruntime-1.31.0-qualified-1/prefix"
```

See [development](docs/DEVELOPMENT.md) to rebuild them. Commit `Cargo.lock`;
build with `--locked`. No Bun, Node, Python or shell setup may become an
end-user requirement.

## Verify

Use the cheapest check that covers the change.

1. While editing: the changed tests by name,
   `cargo test -p <crate> --locked --lib <filter>`. Tests with wall-clock
   audio or export deadlines need `CARGO_PROFILE_DEV_OPT_LEVEL=1`.
2. Before a commit: `cargo xtask check` (format, lint with `-D warnings`,
   tests for changed packages). Add one named replay
   (`cargo xtask replays --scenario <name>`) when that UI path changed.
3. Once per coherent delivery: `cargo xtask gate` and `cargo xtask replays`.

A timeout while other builds run is machine load, not a result: rerun that
test alone. Reproduce a failure once with diagnostics, fix the cause, verify;
do not loop retries. UI changes are judged by looking at replay frames, on
real footage where it matters ([UI feedback](docs/UI_FEEDBACK.md)). Never
bless a visual baseline to make a failure pass.

`cargo xtask install` puts the current build in `/Applications`.
`cargo xtask hygiene` prunes the build directory; run it when `target`
passes 40 GiB.

## Shared checkout

Other agents and sessions may be editing this checkout.

- Never use `git stash`, `checkout`, `restore` or `reset` to test something;
  use a copy.
- Commit with explicit paths (`git commit -m "..." -- <paths>`), never a bare
  commit after `git add`. Push to `main` with `git push`.
- One Cargo job per target directory. A parallel lane gets its own
  `CARGO_TARGET_DIR` under `target/`; do not start a second build in someone
  else's.
- Scratch files go under `/tmp`, never in the repository.
- Format only files you changed.

## Crates

- `crates/deadpan-core`: exact time, the beat document, commands and
  reversible patches. No I/O.
- `crates/deadpan-plan`: immutable render plan (picture mapping, audio spans)
  compiled from a document.
- `crates/deadpan-audio`: source resampling, stretch stages, gain, edges,
  limiter; prepares PCM off the device thread.
- `crates/deadpan-store`: SQLite project packages, revisions, undo/redo,
  media objects, render jobs, backups.
- `crates/deadpan-media`, `native/deadpan-source`: verified source
  snapshots, indexes, FFmpeg decode.
- `crates/deadpan-render`: the shared GPU picture pipeline.
- `crates/deadpan-playback`, `native/deadpan-output`, `native/deadpan-dsp`:
  audition, device output, the pinned stretch engine.
- `native/deadpan-encode`, `native/deadpan-media-worker`: H.264/AAC and
  HEVC encode, conversion, proxies.
- `crates/deadpan-analysis`, `native/deadpan-transcribe`,
  `native/deadpan-align`, `native/deadpan-track`: transcript, word
  refinement, speech activity, shots, tracking.
- `crates/deadpan-jobs`, `crates/deadpan-models`: worker protocol and AI
  pause generation.
- `crates/deadpan-cli`: the headless project and command API, reused by the
  app.
- `crates/deadpan-app`: the egui/eframe app. One service owns the writable
  store; workers prepare media and previews.
- `crates/xtask`: check, gate, replays, bundle, install.

Unsafe code stays inside the `native/` adapters. Owned subprocesses launch
through `deadpan_native_process::spawn`.

## Invariants

- Exact typed frames, samples and source timestamps; half-open ranges;
  rational frame rates; checked arithmetic; boundaries from the common
  origin. Never accumulate rounded durations.
- Every input (keys, widgets, macros, CLI) becomes a typed command resolved
  against a revision and committed in one reversible transaction. No widget
  or background job mutates authored state.
- One render plan and the same picture and DSP code for preview and export.
  Never omit an unsupported effect silently.
- SQLite is authoritative. Originals and accepted generated media are not
  caches.
- The UI thread and the audio callback never block on decode, analysis,
  inference or the database.
- Describe every user action once in the action registry
  (`crates/deadpan-app/src/navigation/registry/entries.rs`); keys, help and
  `docs/COMMANDS.md` come from it.
- Preserve the measured encoder constraint in
  [native encoding](docs/NATIVE_ENCODING.md): AAC timing uses edit lists with
  explicit priming, verified on the emitted file.

## Where the detail lives

Each area has a contract document under `docs/`. Read the one for the code
you are changing; do not read them all.

- Old long-form conventions, by topic: [conventions](docs/CONVENTIONS.md).
  It predates the brief. Where it mentions DP requirements, gates,
  qualification records or migrations, the brief wins.
- Architecture map: [architecture](docs/ARCHITECTURE.md).
- Editing commands: `docs/INSERT_TIME.md`, `STRUCTURAL_SPLIT.md`,
  `EDITED_SLICES.md`, `SOURCE_TRIM.md`, `SOURCE_SLIP.md`, `SOURCE_ROLL.md`,
  `REPEAT_SELECTION.md`, `GROUP_EDITING.md`, `SCOPED_EDITING.md`,
  `SEMANTIC_MACROS.md`, `SEMANTIC_REPEAT.md`.
- Audio: `docs/AUDIO_PLAN.md`, `AUDIO_EDGES.md`, `AUDIO_GAIN.md`,
  `AUDIO_MASTERING.md`, `SOUND_EVENTS.md`, `PLAYBACK.md`.
- Picture: `docs/FRAMING.md`, `TARGETS.md`, `PROJECT_PICTURES.md`,
  `PROXIES.md`, `HDR_OUTPUT.md`.
- Export: `docs/RENDER_JOBS.md`, `ENCODED_RENDER.md`,
  `RENDER_PUBLICATION.md`, `PREVIEW_EXPORT_VERIFICATION.md`.
- Storage: `docs/TIMING_STORAGE.md`, `STORAGE.md`, `BACKUPS.md`,
  `RECOVERY.md`.
- UI: `docs/NATIVE_WORKSPACE.md`, `KEYMAP.md`, `UI_FEEDBACK.md`,
  [design targets](docs/design/README.md).
- Analysis and AI: `docs/TRANSCRIPTION.md`, `SPEECH_ACTIVITY.md`,
  `TRACKING.md`, `AI_HOLDS.md`.
- Packaging: `docs/PACKAGING.md`.

## Writing code and docs

- Comment only what the code cannot show. History goes in the commit message.
- Add tests for behavior that matters; do not add tests that pin an old
  implementation or compare against a saved copy of one.
- When you establish a convention, put it in the area document, not here.
  Keep this file under 15 KB.
- `AGENTS.md` is the instruction source for every harness. Never add a
  `CLAUDE.md`.
