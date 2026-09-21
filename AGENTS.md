# Deadpan

Deadpan is a native macOS structural video editor for timing and attention, built around editable beats and a keyboard-first workflow.

## Product authority and current scope

Read [the full specification](docs/spec/DEADPAN_SPEC.md) and [agent handoff](docs/spec/AGENT_HANDOFF.md) before feature work. The Markdown specification is normative; summaries here do not reduce its scope. [Requirements](docs/REQUIREMENTS.md) tracks DP-01 through DP-24 and Gates A through G. Keep code, tests, evidence, and remaining work current there.

The current foundation includes validated beat documents, reversible structural commands, persistent marks with edit transforms, sparse per-play overrides and automatic nested occurrence isolation, stable repeat identities, exact indexed picture plans and boundary queries, SQLite project/history storage with schema migration, and a shared headless command entrypoint. The native UI remains a welcome shell. It is not yet a usable video editor or release candidate. All product requirements remain open or partial. A button, mock worker, downloaded model, ignored test, or proposed target does not prove implementation.

## Design philosophy

- Preserve the editor's timing decisions. Use exact typed frame, sample, and source coordinates, half-open ranges, rational frame rates, checked arithmetic, and origin-based sample boundaries. Never accumulate rounded durations. Three plays means three total plays, with gaps only between them; a Hold inserts exactly its authored frames and preserves subsequent original speech.
- Build composable structures. Source, Sequence, Hold, Repeat, and Retime form the small primitive set; attention, sound, captions, and cutaways attach to it. Gags expand to ordinary editable primitives. Keep repeats structural and occurrence identities stable.
- Make the keyboard workflow native and discoverable. Picture dominates the interface; controls explain current mode, context, selection, units, and scope. Respect macOS text editing, IME composition, focus, accessibility, and non-US layouts. A user must be able to complete the workflow without a mouse.
- Preserve intentional dynamics. Silence, room tone, and permitted tails are different policies. Do not normalize individual words or quietly level breaths and pauses. Monitoring volume is independent of export gain.
- Route every input through typed commands, revision-aware resolution, validation, and atomic reversible transactions. Widgets, macros, CLI calls, and future agents share this path. No widget or background job mutates authored state directly.
- Keep the core independent of UI, media handles, databases, workers, and networking. Use focused typed modules and narrow provider boundaries rather than a general plugin framework.
- Keep ordinary editing immediate and local. Bound queues, memory, and work; prioritize audio and current-frame preview. No analysis or inference job may block a structural edit. Sources, transcripts, and generated footage stay local.
- Use one render plan and the same picture and DSP semantics for preview and export. Export one immutable committed revision and verify the emitted file. Never omit unsupported effects silently.
- Make AI acceptance explicit. Inserting time immediately commits a deterministic fallback; actual local generation produces a candidate. Only an explicit undoable acceptance changes the provider. Stale jobs cannot overwrite newer edits, and accepted media must work without its model.
- Preserve user assets and history. SQLite is authoritative; JSON dumps are derived. Originals and accepted artifacts are not disposable caches. Recovery, migration, relinking, and worker failures must retain user intent and report failures truthfully.
- Measure capability. Record hardware, revisions, licenses, fixtures, failures, and actual results. Gate A qualifies media, GPU, audio, model, and packaging choices before large integration work. Performance targets and upstream benchmarks are not Deadpan measurements.

## Architecture and conventions

Current crates:

- `crates/deadpan-core`: exact time, validated documents, structural commands, and reversible patches; no I/O or identity generation.
- `crates/deadpan-store`: authoritative SQLite packages, immutable revisions, atomic writes, durable undo/redo, and database checkpoints.
- `crates/deadpan-plan`: immutable indexed picture mappings through structural beats, using exact frame centers and original source identities; no decoding or DSP.
- `crates/deadpan-jobs`: bounded worker protocol, pure attempt lifecycle, process supervision, contained artifact snapshots, and exact bridge-generation planning. The real MLX adapter in `tools/model-qualification` is a development harness; app inference and job persistence remain open.
- `native/deadpan-process`: narrow Darwin group-membership adapter; unsafe is denied except for its documented bounded libproc call. Higher layers continue to forbid unsafe.
- `crates/deadpan-app`: native `egui`/`eframe` application using `wgpu` on Metal; development welcome shell.
- `crates/deadpan-cli`: versioned headless project/command API, reused by `deadpan-app --headless`.

[Architecture](docs/ARCHITECTURE.md) records Section 24's full boundary map. Add crates only when an implemented responsibility needs isolation. Do not create empty crates or feature controls that pretend to work.

Use Rust 1.97.1 as pinned in `rust-toolchain.toml`, Cargo, rustfmt, Clippy, strong types, and meaningful unit/property tests. Keep platform-specific unsafe code inside qualified adapters and out of core. Avoid debug leftovers, broad suppressions, unchecked conversions, and unrelated dependency additions. Commit `Cargo.lock` and test locked dependencies.

Every persisted edit, undo, and redo gets a never-reused revision ID. Core inverse patches can restore exact fixture identity; the store rebases them onto fresh revisions to prevent stale commands becoming valid after undo. Store writes use one transaction for the revision, history, and cursor. Keep `.writer.lock` held for the writable store lifetime; read-only inspection and dry runs may coexist. Take live database snapshots through SQLite's backup API, never copy only an open main database file.

Repeat play IDs are scoped by Repeat node and allocation revision, with an ordinal inside that allocation. Preserve surviving IDs through resizing and reorder; allocate fresh IDs for growth and inserted subtrees. Imported initial snapshots reserve their allocation names even after plays are removed. Keep compact runs bounded and never expand a repeat merely to seek. Schema 4 retains these runs and marks, and adds sparse override subtrees. Schema-1, schema-2, and schema-3 migrations replay the complete chronology directly into the current schema on a consistent copy, compare every legacy snapshot/transaction, and promote through SQLite's backup transaction only after validation. Strict legacy adapters reject new fields and unexpected mark or override changes. Preserve the pre-migration backup.

The setup workflow's TypeScript/Bun/mitools/Biome defaults do not apply to this Rust-native product. The maintained Rust sibling `beastie` supplies the initial workspace conventions; consult maintained siblings for evolving personal tooling patterns. [Dependency decisions](docs/DEPENDENCIES.md) records the pins and qualification boundaries. Do not introduce Bun, Node, Python, or shell setup as an end-user requirement. Future model workers use an app-managed private runtime selected through measurement.

## Validation and delivery

Sparse play overrides are owned subtrees keyed by Repeat node and stable play
identity. Use `ProjectDocument::children()` for structural traversal; the
primitive `NodeKind::children()` list omits override roots. Share `RepeatLayout`
between validation, anchors, mark transforms, and picture plans so variable play
durations and gaps use the same exact mapping. Reject paths into a default child
for an overridden play. Shrinking or clearing an override removes its owned
subtree atomically, preserving inverse history and applying mark loss policy.
Imported subtrees normalize both their Repeat allocations and override keys.

`EditOccurrence` isolates repeated ancestors from outside inward and applies the
node operation in one transaction. Reuse existing override branches. Transparent
internal copies retain compact play orders under fresh caller-supplied node IDs;
they share immutable media references, never authored nodes. Preserve exact
coordinates during isolation, then transform marks against that isolated tree
for the actual edit. Copy owned Local/Source marks with fresh IDs, relocate
concrete occurrence marks once, keep sequence-pinned events once, and never
rebind unresolved marks. Ownership determines inheritance even when a mark's
coordinate host differs from its owner. Reject exhausted identity pools or
document-limit growth atomically.

Boundary queries use `AnchorIndex` against one immutable revision. Keep source
clocks, local fractions, and complete occurrence paths explicit; never infer a
repeated play or clamp a missing source interval. Round once at the final project
boundary and return the exact coordinate too. Persist mark ownership separately
from its coordinate. Structural commands transform marks in the same forward and
inverse patch as the tree edit. Follow stable content/play/gap identities inside
the retained host, apply explicit loss policy, and never automatically reattach
an unresolved mark. Original source coordinates and sequence-pinned coordinates
stay fixed in their respective clocks. Named-mark queries still require explicit
occurrence scope when the stored coordinate is ambiguous.

Run the exact repository gate after implementation:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo run -p deadpan-cli -- doctor
```

Prefer unit tests, integration tests, and deterministic headless harnesses for most verification. Keep command resolution, keyboard state transitions, geometry, job lifecycle, and persistence testable without opening the app. Reserve computer use and live GUI testing for valuable evidence that those tests cannot provide: visual quality, native focus/IME, accessibility, natural keyboard navigation, and end-to-end interaction. Review GUI aesthetics and keyboard ergonomics explicitly as the interface develops.

Audio preparation owns the DSP call schedule; device/export consumer block sizes
must not change it. Use origin-based input boundaries and explicit context/crops
for short clips. A fresh suffix render is not a restored phase state. Seek from
a matching checkpoint, exact canonical replay, or prepared PCM, and measure the
work and cache lifecycle. DSP preparation and file reads stay off the device
callback. The isolated canonical prototype is evidence for that boundary, not
application playback or listening qualification.

Worker stdout contains only versioned, bounded, length-framed control messages;
stderr is drained into a bounded diagnostic tail. A completed manifest enters
host validation, never automatic acceptance. Preserve the original revision as
provenance while comparing actual generation dependencies for relevance, so an
unrelated edit does not stale a Hold. Stale or detached attempts never revive.
The job service polls process deadlines and owns shutdown; pipe pumping and
process reaping never belong to the audio callback. Vetted executable selection,
artifact containment/validation, and durable promotion remain host boundaries.
Pin `ArtifactWorkspace` before launching the worker. Snapshot only below the
host-selected output scope after clean teardown; consume that immutable copy
for subsequent media validation, never reopen a worker-supplied pathname.
The snapshot verifies bytes and containment, not media validity or acceptance.

Bridge plans select the nearest legal native count with exact rational boundary
duration and upward tie-breaking. Revalidate deserialized plans against the
selected provider capability, including nearest-count selection. Sample only at
`(j+1)*(M-1)/(N+1)` and keep the native sequence and interpolation policy. The
model's conditioning preprocessing and chosen RGB color interpretation belong
in provenance. A generated file passing hash, frame, and metadata checks still
needs continuity review, explicit acceptance, durable storage, and app integration.

For native startup or lifecycle changes, also run `cargo run -p deadpan-app -- --smoke-test` on supported Apple Silicon macOS. This checks startup and the shutdown callback, not media or accessibility qualification. Choose interactive checks for affected behavior when they add evidence; do not repeat them mechanically for unrelated changes. Add relevant media, persistence, worker, accessibility, or packaging checks as those systems are implemented. Record skipped checks and exact failures in the delivery report. [Development](docs/DEVELOPMENT.md) describes the workflow.

For authorized scoped work in this personal project, implement, review, verify, commit, and push to `main` using `git push`. Preserve concurrent changes and do not include unrelated files. Release publishing, signing, notarization, and external service actions need their applicable authorization; pushing source is not product release qualification.

Update these living instructions when implementation establishes a durable convention. Keep detailed procedures in the relevant documentation and keep the original specification intact; document any necessary product deviation explicitly.
