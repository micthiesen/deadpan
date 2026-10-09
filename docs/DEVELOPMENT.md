# Development

Deadpan uses a Rust workspace, pinned to Rust 1.97.1. Its native application targets Apple Silicon macOS; the specification proposes macOS 15 as the initial deployment baseline, pending qualification. Cargo installs/builds the locked Rust dependencies. Native development requires the macOS build tools.

The CLI and source preview need no credentials or model weights. The complete
workspace builds its media helpers against pinned LGPL FFmpeg 8.0.3 with BSD
libopus 1.6.1 statically linked into libavcodec.
Build that developer dependency once on Apple Silicon macOS with Python 3, GnuPG,
Clang, and Make available:

```sh
python3 tools/media-qualification/compatible/build.py \
  --work "$HOME/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-qualified" \
  --output "$HOME/Library/Developer/Deadpan/ffmpeg-opus-build.json"
export DEADPAN_FFMPEG_PREFIX="$HOME/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-qualified/prefix"
```

The work directory must be empty. The builder verifies the pinned archive hash
and FFmpeg release signature, runs libopus's tests, disables GPL/nonfree/version-3 components and networking,
and records build/license/library evidence. The Cargo build refuses an absent or
incompatible prefix. It never falls back to a system FFmpeg installation.
Keep the prefix available when running Cargo-built executables. A packaged
`Deadpan.app` carries its own relocated copies of these libraries and never
reads the prefix at run time; see [Packaging](#packaging).
CI builds the same pinned dependency before running the complete gate.
These developer tools must never become end-user requirements.

Use a durable development directory for this dependency. macOS's temporary-file
cleanup removed headers from the former `/private/tmp/deadpan-ui-ffmpeg/prefix`.
On 2026-10-09 the pinned builder created the libopus-qualified prefix above;
all sixteen libopus upstream tests passed. The native FFmpeg Opus decoder is
disabled because measured SILK/hybrid output failed the reference comparison.
The earlier durable `ffmpeg-8.0.3` prefix remains intact for archived binaries;
new source builds require the libopus prefix. Do not relocate dylibs by copying alone: their
install names must match the new prefix. The build report and command logs live
beside the durable build, outside the repository.

## Validation workflow

During implementation, select the changed crate, relevant integration targets
and affected dependants. For example, while changing playback:

```sh
cargo fmt --all -- --check
cargo clippy -p deadpan-playback --all-targets --locked -- -D warnings
cargo test -p deadpan-playback --lib --locked
```

After independent review, run one full gate for the completed milestone from
the repository root:

```sh
cargo xtask gate
```

The adversarial tests (`adversarial_*`) run in deterministic regression mode
within these workspace tests. For a time-bounded fuzzing campaign, a sanitized
decoder campaign or the full long-project stress, run `cargo xtask chaos`,
`cargo xtask chaos --sanitize` or `cargo xtask chaos --stress`; see
[the adversarial suite](ADVERSARIAL.md).

The gate runs build-directory hygiene, then `cargo fmt --all -- --check`, workspace
Clippy with `-D warnings`, `deadpan-app` Clippy with `ui-harness`, and the
workspace and `ui-harness` tests (nextest when installed, then doc tests;
otherwise `cargo test`). CI runs the same commands individually:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Locally, `cargo nextest run --workspace --locked` followed by
`cargo test --workspace --locked --doc` replaces the last command with the
same tests. Nextest runs each test in its own process across all test binaries
in parallel; on 2026-10-04 an M5 Max ran all 3,604 workspace tests in 96 s.
CI keeps plain `cargo test`. Install nextest with `brew install cargo-nextest`.

Cargo never deletes superseded artifacts. On macOS, unpacked debug info keeps
object files beside every test binary, so each rebuild adds more. A directory
with millions of entries makes every rustc invocation slow in the kernel: on
2026-10-04, 2.87 million `.o` files (494 GB) made an incremental
`deadpan-core` test build take 376 s; after `cargo clean` it took 16 s. One day
later, with that fixed, `target/debug` had regrown to 78 GiB: 40 GiB of
superseded dependency artifacts and 39 GiB of incremental state in 1,555
per-crate directories. `cargo xtask gate` (or `cargo xtask hygiene` alone)
therefore measures the Cargo profile directories first and, above 40 GiB
(`DEADPAN_TARGET_LIMIT_GIB`), removes the oldest artifacts with
`cargo sweep --maxsize` and the least recently changed incremental directories
until 20 GiB remain; recent builds stay warm and rustc rebuilds missing
incremental state. Install `cargo-sweep` with `brew install cargo-sweep` (it is
in the dotfiles Brewfile); without it only incremental state is pruned. The dev
profile keeps line tables for workspace crates and omits dependency debug info
to slow the growth.

Workspace integration tests already build and exercise the normal application,
CLI and media-worker executables, including doctor. A separate workspace build
and doctor invocation add no required coverage after those tests. Use builds
for packaging/native startup work and doctor for environment diagnosis.

For app changes, also run the optional harness lint/tests documented below.
Keep base-app checks: `ui-harness` has different dialog, playback and reply paths,
so combining everything into one feature-enabled run would lose coverage. CI
checks both. Backend-only changes do not need another unchanged app-feature
run or painted replay.

One session owns Cargo execution. After an interruption, inspect the existing
PID and log before launching anything else. A quiet command or yielded tool
session is not a failed test. Preserve finished results and run only the
affected targets after a small review fix. Do not rerun focused tests already
covered by the workspace gate, or restart the whole gate for a documentation
edit. Record exactly which source changes each later check covers. Repeat or
broaden verification only for a changed behavior, failure or unresolved concern.
Reproduce a failure once with phase/error diagnostics, fix the cause and verify;
never keep retrying until green. Retry a capability failure only after that
capability changes. Keep one concise result record and use Git checkpoints
instead of creating another full archive and gate-script copy per iteration.

Owned runtime launches use the shared process adapter. On macOS, qualify changes
to that boundary with `python3 tools/process-qualification/check_pipe_inheritance.py
--output /tmp/deadpan-pipe-proof-NEW`. The [process-launch record](qualification/process-launch-2026-09-27.md)
explains the deterministic raw/fixed pipe-inheritance witness and its cooperative
scope. A passing rerun alone does not diagnose a worker pipe-EOF failure.

Use `cargo fmt --all` to apply formatting. Keep `Cargo.lock` committed; dependency updates are explicit reviewed changes. Tests should establish meaningful behavior and failure modes rather than mirror implementation.

Use unit and integration tests for command transactions, grammar, geometry and
worker state transitions. For meaningful UI changes, add or update an application
replay scenario and use the [UI feedback loop](UI_FEEDBACK.md). Inspect the actual
offscreen application images as part of the change, including mouse interaction,
readability, focus, selection and intermediate feedback. Run its separate release
performance mode when the change can affect responsiveness. Native interaction
covers the OS behavior that replay cannot observe: file panels, physical key
delivery, native focus/IME, VoiceOver and physical display presentation.

The gate verifies only the implemented foundation. It does not establish media accuracy, AI quality, accessibility conformance, signed distribution, or performance budgets. Those require the evidence in the [requirement tracker](REQUIREMENTS.md) and specification.

Source audio preparation has a deterministic headless signal/performance probe:

```sh
cargo run -p deadpan-audio --release --example qualify_source --locked
```

It prints JSON for seven source-rate/phase cases, actual output hashes, sampled
signal errors and seven block timings per case. It measures the production
sampler and matrix on synthetic stereo windows, including staging. It does not
measure decoding, a full mix, device deadlines or listening quality. The crate's
integration tests separately exercise verified PCM/AAC sessions and explicit
speaker interpretation. See [source preparation](AUDIO_PREPARATION.md).

## Application UI feedback

With the pinned FFmpeg prefix available, write each run to a new scratch directory:

```sh
cargo test -p deadpan-app --features ui-harness --locked
cargo run -p deadpan-app --features ui-harness --locked -- --ui-check --output /tmp/deadpan-ui-visual-NEW
cargo run -p deadpan-app --release --features ui-harness --locked -- --ui-check --mode performance --output /tmp/deadpan-ui-performance-NEW
```

For a meaningful UI milestone, replay every scenario through one debug harness
build, each in its own process and report directory:

```sh
cargo xtask replays                                  # all scenarios, one at a time
cargo xtask replays --scenario room-tone,delete-range --jobs 3 --output /tmp/deadpan-replays-NEW
```

It lists scenarios from the built harness (`deadpan-app --ui-check
--list-scenarios`), prints PASS/FAIL, seconds, check counts and layout-retry
warnings per scenario, writes `summary.json` beside the per-scenario reports and
`NAME.log` files, and exits nonzero on any failure. Fixture-only scenarios
(`generated-picture`, `ai-pause-ready`) are reported as skipped. Scenarios run
a private copy of the app and its beside-executable helpers under `NEW_DIR/bin`,
with their SHA-256 in `summary.json`, so concurrent rebuilds of `target/debug`
cannot affect a run. More than one job shortens the run but shares CPU and GPU, so rerun a timing
failure alone before diagnosing it. See [UI feedback](UI_FEEDBACK.md#all-scenarios).

Visual mode is the normal agent UI loop. It must use the production widgets,
event routing, real project service and shared GPU video path. Read the report,
inspect the contact sheets and affected full-size frames, then fix defects and
repeat the affected scenarios. Compare with [the boards](design/README.md), not
only the previous screenshot. Never automatically accept changed images as a new
baseline. Performance mode measures monotonic stage latency independently of
replay time and screenshot work. Neither mode establishes native accessibility or
subjective ease of use. See [UI feedback](UI_FEEDBACK.md) for current coverage,
measurement limits and the authoring contract; record unavailable checks as such.
CI compiles, lints and tests the optional harness and shortcut contracts. Rendered
replay requires a Metal-capable host; a missing adapter fails with a report rather
than passing without images. Host GPU replay remains an agent verification step.

## Packaging

`cargo xtask bundle` builds a relocatable, hardened-runtime `Deadpan.app` with
release executables, relocated FFmpeg libraries, the bundled downloader
baseline, notices, an SBOM and provenance. It builds in its own path-remapped
`target/bundle` directory, so it never disturbs `target/release`.
`bundle-verify` exercises a copy in a scrubbed environment with an isolated
`HOME`, including tampered and missing helper cases. Personal bundles use ad hoc
signing and require neither Developer ID nor notarization (§27.1). The optional
Developer ID/notarization path needs a signing identity and credentials. See
[Packaging](PACKAGING.md) for the layout, runtime lookup, entitlements,
notarization steps and current evidence.

```sh
export DEADPAN_FFMPEG_PREFIX="$HOME/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix"
cargo xtask bundle --output /tmp/deadpan-bundle
cargo xtask bundle-verify /tmp/deadpan-bundle/Deadpan.app
```

Run both after changing helper, worker, model or FFmpeg lookup, adding an
executable, native library or runtime dependency, or changing signing. Never
install the bundle into `/Applications` as part of a check.

## Native application smoke test

The [Deadpan identity assets](design/brand/README.md) include the editable macOS
icon, complete legacy iconset and logo exports. Bare Cargo launches use the
embedded PNG. A bundle that declares its own icon keeps macOS appearance handling.

To wrap an already-built executable in a quick native developer app:

```sh
python3 tools/build-app.py --binary target/debug/deadpan-app --output /tmp/Deadpan.app
```

Choose a new output path. The script compiles the layered icon into the bundle
and includes the ICNS fallback. It still loads FFmpeg from the build prefix and
is not relocatable; use [`cargo xtask bundle`](#packaging) for a self-contained
bundle.

On a supported Apple Silicon Mac:

```sh
cargo run -p deadpan-app -- --smoke-test
cargo run -p deadpan-app
```

`--smoke-test` opens the native window, closes it after frames have rendered, and
checks the shutdown callback. Run it for native startup or lifecycle changes.
Use the offscreen loop for routine layout and interaction review, then choose
native checks for affected OS behavior or unresolved real-use questions. Confirm
no media/import/render controls imply unavailable functionality. For lifecycle
changes, verify the intended quit/SIGTERM behavior and exit status. Record the
actual OS/hardware and what was observed; a successful compile is not a UI smoke
test. Unrelated pure-core changes do not require GUI runs.

The [native workspace](NATIVE_WORKSPACE.md) uses `⌘N`, `⌘O` and `⌘I` for
project creation, project opening and media import through macOS panels. Import
registers a source; `,i` explicitly inserts the entire source after the
selected beat, or at sequence end. In Original, `v` plus h/l selects a
half-open moment and `y` copies it. Your edit also supports `v`, motion and `y`
for historical edited contents, including complete owned beats and supported
Source/Hold fragments. `:splice` previews local In/Out refinement and insertion
or replacement. Fast `p`/`P` paste after/before the selected beat or replace a
selected range in the current ordinary Sequence group. `⌘Z` / `⌘Shift Z` navigate saved history.
Use `--project /absolute/project.deadpan` to reopen directly, or
`--preview-source /absolute/video.mp4` for standalone source inspection.
The source decoder retains the [admitted MP4/Matroska grammar](SOURCE_ADMISSION.md).
Space plays/pauses [limited Original/edit audition](PLAYBACK.md); Shift+Space loops the selected moment or beat. Run
`cargo test -p deadpan-playback -p deadpan-output --locked` for canonical PCM,
delivery-clock, cancellation and queue tests without a native device. Full
mastering, range editing, generated-provider preview and export remain open.

Playback unit tests reserve one real-media workload per test binary before
fixture preparation. Pass that permit to the test engine helper: its shared
repaint callback retains the reservation until both asynchronous workers exit.
Dropping an engine or receiving `Stopped` only requests or acknowledges a stop;
neither proves preparation teardown. Other PCM readers in the same binary use
the same reservation. Admission waits at most ten minutes, including queueing
behind complete scenarios, so a leaked worker fails instead of stranding the
suite. Pure arithmetic tests remain parallel. This isolates
debug DSP cost without changing playback prefill, watchdogs or test deadlines.
It does not establish production throughput or device latency. See the
[playback test scheduling record](qualification/playback-waits-2026-09-27.md).

`h/l` or Left/Right move the boundary cursor, accepting counts such as `12l`.
`j/k` choose sources or sequence beats according to pane focus. `gg/G` and
Home/End choose start/end, with the final preceding frame shown at the end.
Tab/Shift Tab cycle panes. `/` enters source search; `:` enters a command.
Native text editing and IME events suppress editing bindings. Escape/Return leave
text entry after its final input is applied. `:help` lists the implemented keys.
Run `cargo test -p deadpan-app --locked` for actual-media service/preview tests,
deterministic worker interleavings, binding transitions, dialog polling, headless
egui focus and final-text routing, canvas geometry, and requested/decoded/displayed
picture transitions. Camera adds `,f`, visible counted movement/scale hints,
validated numeric fields and Enter/Escape draft handling; see [framing](FRAMING.md).
The viewer caption identifies the submitted picture;
the bottom boundary can advance while the next picture is preparing. Tests cover
GPU-delay state, stale replies, picture-error recovery and actual decoded freezes
at distinct sequence positions. See the
[presentation evidence](qualification/preview-presentation-2026-09-21.md).

Native sound authoring uses `cargo test -p deadpan-app --locked project::tests::sound`
for qualified service placement, history, receipt rejection and exact mapping
nudges. The optional `sound-placement` UI replay covers real input, text entry,
event focus, delayed command targets and painted controls. Run it through the
[UI feedback loop](UI_FEEDBACK.md), with the production Kestrel source when
available. It does not replace decoded-PCM, device or listening qualification.

Headless source evidence uses `cargo run -p deadpan-media --example inspect_source -- /absolute/video.mp4 /tmp/new-source-report.json`.
The report preserves original clocks, observed terminal duration, content identity,
retained-pixel hashes and seek timings. The input must be a regular file at most
256 MiB. Source indexing has independent memory, frame and cooperative time bounds.
Missing terminal duration is an error, not an inferred nominal endpoint.

Run `cargo run -p deadpan-render --example qualify_picture -- /tmp/new-picture-report.json`
for actual offscreen Metal comparisons against the CPU reference. Run
`python3 tools/media-qualification/host/build_sanitized.py --work /tmp/new-empty-source-sanitizers --package deadpan-source --package deadpan-media`
for C-adapter ASan/UBSan with source-session integration tests. These harnesses
complement [native visual and keyboard evidence](qualification/source-preview-2026-09-21.md);
they do not establish playback, physical display calibration or preview/export equivalence.

The committed encoder-picture host has an actual Metal probe:

```sh
cargo run --release -p deadpan-cli --example qualify_project_picture --locked -- \
  /tmp/new-export-pictures.json /tmp/new-export-pictures
```

It checks immutable revisions, exact output timestamps, nonzero ranges, Original
frames, captured Holds, Repeat gaps, Background, odd-canvas output and one retained
I420 result. Add a third argument pointing to the retained `accepted.deadpan`
fixture to check all 30 Generated frames against independent complete-plane
references; omitting it records a skip. The package needs its adjacent
`generated-picture-fixture.json`, produced by the bundle integration test's
`DEADPAN_GENERATED_PICTURE_FIXTURE_ROOT` option. Give native qualification an
external timeout as well as its cooperative deadline. These are encoder-input
checks, not an encoded file or complete product Render. See
[the output contract](EXPORT_PICTURES.md).

To exercise [process isolation](RENDER_WORKER.md), build both the CLI binary and
this example in the same locked release build. Pass the retained accepted fixture
as the third argument and that build's absolute `deadpan-cli` executable as the
fourth. The example records worker output separately and compares every isolated
frame with the direct producer, including independent odd-canvas and Generated
references. It also checks live edit/undo/redo and cancellation/recovery. Use an
outer timeout above its 300-second cooperative deadline. Record the exact Cargo
artifact paths and hashes; do not infer that an old `target/release` binary came
from the current build. Omitting the executable records an explicit worker skip.

## Native encoding checks

For the [encoded project worker](ENCODED_RENDER.md), build the current
`deadpan-cli` binary and `qualify_project_picture` example with Cargo's JSON
artifact output. The picture example accepts two additional arguments after the
worker executable: `--encoded MARKER_SOURCE`. Use the retained 120-frame 60 fps
`hardware-none-60.mp4` impulse fixture from native-encoding qualification as the
marker Original. Keep the accepted Generated package and its adjacent manifest
available. The example retains actual MP4 candidates and direct I420/PCM inputs.
Its cooperative deadline is 600 seconds; use a longer bounded outer timeout.

Run `compatible/qualify_project_encode.py` under `tools/media-qualification`
with `--work`, `--build-report`, `--worker` and `--picture-report` to independently
decode every complete picture plane and all authored PCM. It also checks native
audio, exact edit lists, visible markers and fresh-decoder GOP suffixes. Its
fixed fixture bounds and quality tolerances do not establish release performance
or a production verifier. `--sanitizers` instruments the independent C/Objective-C
readers; instrument the supplied worker's native dependencies separately when
qualifying the child itself under sanitizers.

The [native encoding boundary](NATIVE_ENCODING.md) has a separate bounded
synthetic fixture. Build `cargo build -p deadpan-encode --example qualify --locked
--message-format=json` and select the executable from that build's Cargo output.
Then run `tools/media-qualification/compatible/qualify_native_encode.py` with
`--encoder`, an empty `--work` directory and the qualified FFmpeg `--build-report`.
The runner independently decodes FFmpeg and AVFoundation audio, reads actual MP4
boxes and compares fresh-decoder GOP suffixes. Keep rejected hardware B-frame
attempts and all partial files in the evidence. `--sanitizers` instruments the
reader probes; the supplied Rust example must separately have its native C
adapter built with ASan/UBSan. Synthetic encoder evidence does not establish
project export. Run `cargo test -p deadpan-cli --test offline_audio --locked`
for immutable canonical PCM, exact nonzero-origin intervals and shared deadlines.

## Original storage checks

The [headless original commands](HEADLESS.md#original-media-ownership) retain
complete files, verify managed or linked snapshots, and relink identical bytes.
They do not register an authored asset or select project presentation timing.
Use `cargo test --locked -p deadpan-store --test original_media` and
`cargo test --locked -p deadpan-cli --test original_commands` for real-file
ownership, relocation, stale/wrong relinking and failure boundaries. Shared
object-storage unit tests force the positional-copy fallback; the native
fileclone tests exercise actual macOS clone independence. These checks do not
require opening the app. Native dialogs and the initial register/insert workflow are connected;
relink UI, bookmarks, complete formats and full-size latency remain open.

For measured registration, run `cargo test --locked -p deadpan-media --test source_qualification`,
`cargo test --locked -p deadpan-store --test source_registration` and
`cargo test --locked -p deadpan-cli --test source_registration`. These open real
video/audio fixtures and cover persisted evidence, transactional insertion,
historical lookup, rollback and explicit stream selection. Run the migration
suite for schema changes. See [source registration](SOURCE_REGISTRATION.md).

For background preparation, run the store `original_preparation` and
`prepared_source_registration` integration suites. They exercise transferable
preparation handles, writer edits during preparation, stale intent, closed or
different sessions, changed originals and atomic rollback. These tests use actual
files and decoders without opening the app. They establish the
[import boundary](IMPORT_PREPARATION.md), not a full-size import latency budget.

For presentation policy, core `presentation_basis` tests exercise origin and
lock transitions, marks, occurrence edits and inverse patches. Store
`presentation_basis` tests use real sources for first-primary selection,
unchanged audio coordinates, geometry-only adoption, admission guards and
rollback. The CLI `source_registration` suite covers automatic creation and
versioned geometry preview/commit. [Presentation policy](PRESENTATION_BASIS.md)
documents the boundary; these tests establish no native canvas-preview quality.

## Source audio checks

Persisted root sound events use core/plan `sound_events` integration tests,
store `migration sounds::` and `source_registration sounds::`, audio
`sound_events`, playback `source_voice::events` and CLI `audio_inspection sounds::`.
These cover durable commands, strict old histories, exact RoundEven placement,
independent decoded PCM sums, the shared limiter, source revocation and bounded
preparation. Original and catalog audition remain independent source views.
Root ripple history adds core `sound_routing`, plan `root_sound_routes` and
`sound_route_sampling`, store `migration sound_routes::`, and real playback
`source_voice::events` coverage. Retain the actual old-CLI fixture provenance;
use schema relabeling only in negative forgery tests. Remaining structural sound
editing and native event placement stay open. See
[root sound events](SOUND_EVENTS.md#persisted-root-sounds).

Structural speed edits use `:retime 0.75 pitch=preserve` or `pitch=tape`, with
exact fractional input also accepted. The inspector shows the final duration
before Enter applies the command. Run the `retime` application replay alongside
the core Retime, store migration/history and decoded audio tests when changing
this path. See [speed editing](RETIME_EDITING.md) for its clock and history rules.

`cargo test --locked -p deadpan-dsp` checks both legacy count-derived and explicit
rational-rate recipes against all 50 prior reference hashes, plus independent
allocation, replay and large-rational boundary cases. The
[native DSP README](../native/deadpan-dsp/README.md) describes the legacy and
exact-rate C ABI sanitizer probes.

Run `cargo test --locked -p deadpan-audio --test sequence` and
`cargo test --locked -p deadpan-cli --test audio_inspection` for plan-driven
source PCM, exact fractional phase, trimmed filter context, historical receipt
binding and read-only inspection. `inspect-audio PROJECT --samples START END`
returns at most 256 stereo samples before effects; the app exposes the same
command through `--headless`. See [source-stage audio](SOURCE_STAGE_AUDIO.md).
`crates/deadpan-plan/tests/audio_signal.rs` checks virtual point-grid semantics
and opaque processing stages. `crates/deadpan-audio/tests/stages.rs` compares
continuous/nested retimes against independently composed DSP and resampling,
including output-only Hold policies, provenance changes and bounded retries.
Use `inspect-audio ... --time-mapped` for the pre-effects Preserve path; see
[stage preparation](AUDIO_STAGE_PREPARATION.md).
Use `--edge-faded` instead for the shared post-mapping edge stage. Core/plan
policy tests, the audio `stages` suite and headless `audio_inspection` cover
authored hard choices, shortened fragments and identical chunked/cropped output.
See [audio edges](AUDIO_EDGES.md) for the fixed envelope and remaining scope.
Room-tone tests add scalar overlap references and a 44.1 kHz source whose exact
loop period differs from rounded storage. The host integration verifies an
explicit AAC loop, separate silence and following speech without history writes.
See [room-tone audio](ROOM_TONE_AUDIO.md).

Scoped sound-mixing preparation is covered by the plan's `audio_tape` and
`audio_definition` tests and the audio crate's `audio_definition` tests. The
`mix::` PCM cases decode distinct 44.1 kHz mono and 48 kHz stereo sources, then
compare exact placement, scoped gates, independent sums and one enclosing
canonical Preserve. They also exercise hidden unsupported histories, dependency
revocation and shared limits. These are headless preparation checks; they do not
qualify sound placement, the complete final mix or acoustic output. See
[sound events](SOUND_EVENTS.md).

The plan's `source_voice` and `source_voice_policy` suites exercise catalog
admission, exact source phase, separate input/output Hold policy and remapped
issuer grids. `cargo test --locked -p deadpan-playback source_voice` registers
real audio without insertion and reads it through production source admission,
the existing tape reader and canonical Preserve preparation. The 44.1 kHz fixture
is rewrapped in a temporary extensible WAV with an explicit mono-center layout;
its PCM bytes stay unchanged. The original unspecified-layout file remains an
explicit rejection case. These tests do not insert persisted sound events or
qualify physical listening. See [source operands](SOUND_EVENTS.md#independent-catalog-source-operands).

The plan's `routed_voice` tests check complete provider captures and exact grid
identity for retained sample routes. Run `cargo test -p deadpan-audio --locked
--test audio_definition projection::routed` for full hidden-history admission,
shared preparation limits and recovery. Run `cargo test -p deadpan-playback
--locked source_voice::routed` for real catalog PCM copied through successive
NTSC insertions, cold Preserve suffixes, fractional windows, prior gaps and
source revocation. The references use independent boundary arithmetic and dense
sample-array copies. These routed reads precede current consuming Hold gates,
creative effects and final mixing. See [routed preparation](SOUND_EVENTS.md#routed-pcm-preparation).

`cargo test --locked -p deadpan-core --test sound_routes` checks exact route
admission and indexed full-leaf point lookup. The plan's `sound_route_sampling`
tests compare retained sample histories with dense sample-copy references,
including repeated NTSC cuts, fractional windows, shifted origins, endpoint
exhaustion, chunked reads and compact billion-period routes. Its `hold_policy`
tests retain distinct current issuers across retimes, stable plays, scoped
definitions and root/intrinsic grids. Playback's `sources::cache_tests` opens
qualified AAC PCM to exercise a seventeenth cache entry, recency, reopening and
revoked/cancelled cold reads. These tests do not place or mix authored sounds.

`cargo test --locked -p deadpan-audio --test signal_transfer` compares bounded
root-to-point reads with materialized masked PCM and canonical DSP. The `stages`
suite adds decoded-source integration, preparation-budget and provenance checks
across the complete transfer halo. See [signal transfer](AUDIO_SIGNAL_TRANSFER.md).

`cargo test --locked -p deadpan-core --test audio_binding_capture` checks compact
capture, existing-binding preservation and configured Repeat-gap ownership.
`cargo test --locked -p deadpan-plan --test audio_fades` checks retained virtual
fade clocks without media. `cargo test --locked -p deadpan-audio --lib bound_reads`
compares actual decoded PCM through moved/resumed bindings, stable Repeat scope,
current Edit support, Preserve input/output policies and shared preparation
limits. See [owned audio bindings](OWNED_AUDIO_BINDINGS.md). These are engine
tests. [Pause insertion](INSERT_TIME.md) connects retained clocks to a native
command; playback and listening remain separate work.

`cargo test --locked -p deadpan-core --test allocation_projection --test audio_bindings`
checks retained visible allocation and chronological reanchors, including hidden
occurrences, nested birth scopes and bounded billion-play layouts. The
`deadpan-audio` integration test `reanchors` uses decoded 44.1 kHz PCM to check
first/later NTSC plays, subsequent pause insertion, fresh seeks and opaque
Preserve preparation. Store migration tests replay the actual core-20/database-26
fixture through core 21/database 27. See [compact audio reanchors](AUDIO_REANCHORS.md)
for the contract and retained verification logs.

`cargo test --locked -p deadpan-core --test audio_bindings --test audio_binding_capture`
checks gap ownership, survival/birth, copied scopes and bounded admission.
The `gap_bindings` integration suites in `deadpan-plan` and `deadpan-audio`
exercise exact root/point clocks, actual decoded PCM, current gap policies and
fresh seeks. Store migration tests replay actual core-21/database-27 history
through core 22/database 28 and reject new vocabulary in frozen histories.
CLI `audio_inspection` checks the same gap path with qualified managed originals.
See [authored Repeat-gap bindings](GAP_AUDIO_BINDINGS.md).

`cargo test --locked -p deadpan-core --test repeat_gap_layout --test gap_overrides`
checks sparse gap geometry, identity lifetime, occurrence isolation, marks and
inverse patches. Plan `gap_branches` and audio `gap_bindings` cover current
picture/audio paths and decoded PCM after materialization, including canonical
born gaps and a real Preserve stage. See [editable Repeat gaps](REPEAT_GAP_BRANCHES.md).

Run `cargo test --locked -p deadpan-audio --test loudness --test true_peak` for
informational metering, generated standard cases, channel/EOF behavior and
transactional admission. [Audio measurement](AUDIO_METERING.md) describes the
compiled raw-PCM harness and independent reference script. These tests need no
GUI or device; metering is not limiting or app audio playback.

Run `cargo test --locked -p deadpan-source -p deadpan-media` for native audio
decode, measured indexes, exact PCM ranges, AAC padding, shared video/audio
snapshots and failure boundaries. Verify fixture bytes with
`python3 native/deadpan-source/tests/generate_audio_fixtures.py --verify`.
The existing source/media sanitizer command above includes this path. These
headless checks establish no listening, device output, resampling or complete
import behavior. See [source audio](SOURCE_AUDIO.md).

## Audio preparation checks

Structural audio queries are covered by `cargo test --locked -p deadpan-plan
--test audio_plan`, including independently computed sample allocation and
expanded small-repeat references. `cargo test --locked -p deadpan-dsp` exercises
the actual pinned engine, prior PCM hashes, ownership, partitions, replay and
failure bounds. The crate's native sanitizer harness separately checks the new
C ABI. [Audio planning](AUDIO_PLAN.md) and [DSP](AUDIO_DSP.md) state the remaining
integration work; these checks do not require GUI or device output.

## Implementation sequence

Prepared output tests run with `cargo test --locked -p deadpan-output` without
opening hardware. The explicit `qualify_output` release example opens the
current admitted macOS output and records callback/seek/starvation evidence.
See [audio output](AUDIO_OUTPUT.md) for its optional quiet tone, limits and
dependency audit. Device probes are intentionally excluded from ordinary CI.

1. Read [the specification](spec/DEADPAN_SPEC.md), [handoff](spec/AGENT_HANDOFF.md), and [AGENTS.md](../AGENTS.md). Sections 1–8 define semantics, 12–14 define AI/runtime contracts, 17–24 define architecture, and 26–31 define verification and command details.
2. Consult [Architecture](ARCHITECTURE.md) for current responsibilities and planned boundaries. Keep the pure core independent; add a crate when working code benefits from isolation.
3. Work on Gate A qualification alongside the pure Gate B domain foundation. Record decisions with exact dependency revisions, licenses, failures, fixture inputs, and reproducible measurements.
4. Add meaningful tests at the relevant boundary, use focused verification while implementing, review the complete scoped diff, then run one milestone gate. Exercise affected native behavior where it adds evidence and preserve concurrent changes.
5. Update the requirement tracker with implementation links, tests, measured acceptance evidence, and outstanding work. Commit and push authorized scoped work to `main`.

## Evidence discipline

For deterministic core behavior, cover fractional rates, origin-based frame/sample mapping, invalid inputs, overflow, half-open ranges, and repeat gaps. As structures arrive, add generated nested documents, inverse transaction properties, identity/anchor invariants, and serialization/render-plan equivalence.

For media work, generate frame-number/impulse/color fixtures and verify the actual encoded file, including VFR, negative/non-zero PTS, delay, rotation, color, and channel layouts. Preview and export must agree within documented stage-specific tolerances. Listen to audio boundary cases as well as measuring PCM.

For AI work, keep lifecycle test doubles separate from actual generation acceptance. Qualify a rights-cleared real-video corpus, exact seams/duration, stale-result handling, usable-output latency, memory pressure, failures, and accepted-project offline playback. A downloaded model or returned file does not satisfy DP-12.

For release, run the full keyboard-only workflow and clean-machine online/offline installer checks without developer tools or preexisting model caches. Publish reproducible performance results, fixture reports, dependency notices/SBOM, and migration policy. No gate passes through unrun or ignored checks.

## Documentation ownership

`docs/spec/` preserves the supplied complete design package. The Markdown specification is normative and the PDF is its reading edition. Evolving implementation decisions, qualification reports, and deviations belong in project documentation with links back to the relevant requirement and section. Keep [AGENTS.md](../AGENTS.md) concise and update durable conventions as they emerge.
