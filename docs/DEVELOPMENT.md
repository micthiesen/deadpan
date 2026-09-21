# Development

Deadpan uses a Rust workspace, pinned to Rust 1.97.1. Its native application targets Apple Silicon macOS; the specification proposes macOS 15 as the initial deployment baseline, pending qualification. Cargo installs/builds the locked Rust dependencies. Native development requires the macOS build tools.

The CLI and source preview need no credentials or model weights. The complete
workspace builds an isolated FFV1 helper and persistent source decoder against pinned LGPL FFmpeg 8.0.3.
Build that developer dependency once on Apple Silicon macOS with Python 3, GnuPG,
Clang, and Make available:

```sh
python3 tools/media-qualification/compatible/build.py \
  --work /tmp/deadpan-ffmpeg-dev \
  --output /tmp/deadpan-ffmpeg-dev-build.json
export DEADPAN_FFMPEG_PREFIX=/tmp/deadpan-ffmpeg-dev/prefix
```

The work directory must be empty. The builder verifies the pinned archive hash
and release signature, disables GPL/nonfree/version-3 components and networking,
and records build/license/library evidence. The Cargo build refuses an absent or
incompatible prefix. It never falls back to a system FFmpeg installation.
Keep the prefix available when running the app or development helper; its dynamic
libraries have not yet been assembled into a portable signed app bundle.
CI builds the same pinned dependency before running the complete gate.
These developer tools must never become end-user requirements.

## Validation gate

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo run -p deadpan-cli -- doctor
```

Use `cargo fmt --all` to apply formatting. Keep `Cargo.lock` committed; dependency updates are explicit reviewed changes. Tests should establish meaningful behavior and failure modes rather than mirror implementation.

Prefer meaningful unit tests, integration tests, and deterministic headless harnesses for most verification. Keep keyboard grammar, focus routing decisions, command transactions, geometry, and worker state transitions testable without live UI automation. Review GUI aesthetics and natural keyboard navigation explicitly. Use computer interaction when visual quality, native focus/IME, accessibility, or end-to-end ergonomics need observation.

The gate verifies only the implemented foundation. It does not establish media accuracy, AI quality, accessibility conformance, signed distribution, or performance budgets. Those require the evidence in the [requirement tracker](REQUIREMENTS.md) and specification.

## Native application smoke test

On a supported Apple Silicon Mac:

```sh
cargo run -p deadpan-app -- --smoke-test
cargo run -p deadpan-app
```

`--smoke-test` opens the native window, closes it after frames have rendered, and checks the shutdown callback. Run it for native startup or lifecycle changes. Where an interactive check adds evidence, confirm the affected layout, focus, keyboard navigation, and close behavior. Do not repeat GUI testing for unrelated pure-core changes. Confirm no media/import/render controls imply unavailable functionality. For lifecycle changes, verify the intended quit/SIGTERM behavior and exit status. Record the actual OS/hardware and what was observed; a successful compile is not a UI smoke test.

The source preview opens explicitly tagged progressive 8-bit SDR H.264/FFV1 in
the [admitted MP4/Matroska grammar](SOURCE_ADMISSION.md). Use `cargo run -p deadpan-app -- --preview-source /absolute/video.mp4`,
or focus the path with `⌘O` and press Enter. Left/Right step original frames;
Home/End select the first/last frame. Text editing and IME events suppress frame
commands. The app does not yet import into projects, edit, play audio or export.

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

## Original storage checks

The [headless original commands](HEADLESS.md#original-media-ownership) retain
complete files, verify managed or linked snapshots, and relink identical bytes.
They do not register an authored asset or select project presentation timing.
Use `cargo test --locked -p deadpan-store --test original_media` and
`cargo test --locked -p deadpan-cli --test original_commands` for real-file
ownership, relocation, stale/wrong relinking and failure boundaries. Shared
object-storage unit tests force the positional-copy fallback; the native
fileclone tests exercise actual macOS clone independence. These checks do not
require opening the app. Native dialogs and the complete import workflow still
need implementation and their own focused verification.

For measured registration, run `cargo test --locked -p deadpan-media --test source_qualification`,
`cargo test --locked -p deadpan-store --test source_registration` and
`cargo test --locked -p deadpan-cli --test source_registration`. These open real
video/audio fixtures and cover persisted evidence, transactional insertion,
historical lookup, rollback and explicit stream selection. Run the migration
suite for schema changes. See [source registration](SOURCE_REGISTRATION.md).

## Source audio checks

Run `cargo test --locked -p deadpan-source -p deadpan-media` for native audio
decode, measured indexes, exact PCM ranges, AAC padding, shared video/audio
snapshots and failure boundaries. Verify fixture bytes with
`python3 native/deadpan-source/tests/generate_audio_fixtures.py --verify`.
The existing source/media sanitizer command above includes this path. These
headless checks establish no listening, device output, resampling or complete
import behavior. See [source audio](SOURCE_AUDIO.md).

## Implementation sequence

1. Read [the specification](spec/DEADPAN_SPEC.md), [handoff](spec/AGENT_HANDOFF.md), and [AGENTS.md](../AGENTS.md). Sections 1–8 define semantics, 12–14 define AI/runtime contracts, 17–24 define architecture, and 26–31 define verification and command details.
2. Consult [Architecture](ARCHITECTURE.md) for current responsibilities and planned boundaries. Keep the pure core independent; add a crate when working code benefits from isolation.
3. Work on Gate A qualification alongside the pure Gate B domain foundation. Record decisions with exact dependency revisions, licenses, failures, fixture inputs, and reproducible measurements.
4. Add meaningful tests at the relevant boundary, run the validation gate, and exercise the affected native behavior. Preserve concurrent changes and review the complete scoped diff.
5. Update the requirement tracker with implementation links, tests, measured acceptance evidence, and outstanding work. Commit and push authorized scoped work to `main`.

## Evidence discipline

For deterministic core behavior, cover fractional rates, origin-based frame/sample mapping, invalid inputs, overflow, half-open ranges, and repeat gaps. As structures arrive, add generated nested documents, inverse transaction properties, identity/anchor invariants, and serialization/render-plan equivalence.

For media work, generate frame-number/impulse/color fixtures and verify the actual encoded file, including VFR, negative/non-zero PTS, delay, rotation, color, and channel layouts. Preview and export must agree within documented stage-specific tolerances. Listen to audio boundary cases as well as measuring PCM.

For AI work, keep lifecycle test doubles separate from actual generation acceptance. Qualify a rights-cleared real-video corpus, exact seams/duration, stale-result handling, usable-output latency, memory pressure, failures, and accepted-project offline playback. A downloaded model or returned file does not satisfy DP-12.

For release, run the full keyboard-only workflow and clean-machine online/offline installer checks without developer tools or preexisting model caches. Publish reproducible performance results, fixture reports, dependency notices/SBOM, and migration policy. No gate passes through unrun or ignored checks.

## Documentation ownership

`docs/spec/` preserves the supplied complete design package. The Markdown specification is normative and the PDF is its reading edition. Evolving implementation decisions, qualification reports, and deviations belong in project documentation with links back to the relevant requirement and section. Keep [AGENTS.md](../AGENTS.md) concise and update durable conventions as they emerge.
