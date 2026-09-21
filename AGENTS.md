# Deadpan

Deadpan is a native macOS structural video editor for timing and attention, built around editable beats and a keyboard-first workflow.

## Product authority and current scope

Read [the full specification](docs/spec/DEADPAN_SPEC.md) and [agent handoff](docs/spec/AGENT_HANDOFF.md) before feature work. The Markdown specification is normative; summaries here do not reduce its scope. [Requirements](docs/REQUIREMENTS.md) tracks DP-01 through DP-24 and Gates A through G. Keep code, tests, evidence, and remaining work current there.

The current foundation includes validated beat documents, reversible structural commands, persistent marks with edit transforms, sparse per-play overrides and automatic nested occurrence isolation, stable repeat identities, exact indexed picture plans and boundary queries, SQLite project/history storage with schema migration, and a shared headless command entrypoint. The native UI previews original source frames through a persistent decoder and shared SDR GPU pipeline. It is not yet a usable video editor or release candidate. All product requirements remain open or partial. A button, mock worker, downloaded model, ignored test, or proposed target does not prove implementation.

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
- `crates/deadpan-store`: authoritative SQLite packages, immutable revisions, atomic writes, durable undo/redo, generation request relevance, persistent attempts and restart recovery, and database checkpoints.
- `crates/deadpan-plan`: immutable indexed picture mappings through structural beats, using exact frame centers and original source identities; no decoding or DSP.
- `crates/deadpan-jobs`: bounded worker protocol, pure attempt lifecycle, process supervision, contained artifact snapshots, and exact bridge-generation planning. The real MLX adapter in `tools/model-qualification` is a development harness; app inference remains open.
- `crates/deadpan-models`: native bridge bundle qualification, retained inputs, measured source spans, and immutable host provenance. Model installation, app scheduling, audition, and application integration remain open.
- `crates/deadpan-media`: shared verified source snapshots, measured video/audio indexes, exact video seeks and bounded private PCM caches, plus isolated conversion, strict helper reports and private BLAKE3 output. No database or authored-state mutation.
- `native/deadpan-source`: separate persistent descriptor-only FFmpeg video/audio decoders, raw metadata, owned RGBA and original-rate interleaved f32. Unsafe code stays in this narrow adapter; unsupported interpretations fail explicitly.
- `native/deadpan-fileclone`: bounded safe descriptor-clone interface around the macOS system call. The store owns copying, checksums, publication and durability.
- `crates/deadpan-render`: bounded shared SDR picture pipeline, linear Rec.2020 working textures, explicit sRGB display transform, aspect and rotation. No decoding, document mutation or encoding.
- `native/deadpan-media-worker`: process-isolated FFmpeg conversion and independent decode verification through bounded descriptor-only AVIO. Only the documented FFI call permits unsafe Rust. Requires the explicitly selected pinned LGPL FFmpeg development prefix.
- `native/deadpan-process`: narrow Darwin group-membership adapter; unsafe is denied except for its documented bounded libproc call. Higher layers continue to forbid unsafe.
- `crates/deadpan-app`: native `egui`/`eframe` source preview using `wgpu` on Metal; one background source worker and keyboard frame navigation. Project editing, playback and export remain open.
- `crates/deadpan-cli`: versioned headless project/command API, reused by `deadpan-app --headless`.

[Architecture](docs/ARCHITECTURE.md) records Section 24's full boundary map. Add crates only when an implemented responsibility needs isolation. Do not create empty crates or feature controls that pretend to work.

Use Rust 1.97.1 as pinned in `rust-toolchain.toml`, Cargo, rustfmt, Clippy, strong types, and meaningful unit/property tests. Keep platform-specific unsafe code inside qualified adapters and out of core. Avoid debug leftovers, broad suppressions, unchecked conversions, and unrelated dependency additions. Commit `Cargo.lock` and test locked dependencies.

Source preview owns one persistent service thread, one replaceable pending request
and one reply. Keep hashing, snapshot copying, indexing and decoding off the UI;
cancel superseded work and reject stale source/request identities. Open local
files nonblocking before checking regular-file metadata. `SourceSession` retains
a private SHA-256-verified copy and an original-PTS index; validate native hard
limits before starting that copy. Random seeks preroll
metadata and convert only the selected frame. Native limits are cooperative,
not a preemptive wall-time guarantee. Never invent missing terminal duration or
accept an unqualified color interpretation silently.

The shared picture baseline accepts owned, bounded, full-range straight RGBA8
with explicit transfer, primaries, SAR, rotation and source PTS. Decode transfer
before filtering into linear Rec.2020 `Rgba16Float`; preserve negative working
values until the explicit SDR display transform. Its encoded `Rgba8Unorm`
display texture is registered with egui without a second implicit sRGB decode.
Keep the registered target alive, release registrations on resize/shutdown,
and admit one picture submission at a time. This does not qualify HDR, physical
display color, editorial effects, playback or an encoded export path.

Every persisted edit, undo, and redo gets a never-reused revision ID. Core inverse patches can restore exact fixture identity; the store rebases them onto fresh revisions to prevent stale commands becoming valid after undo. Store writes use one transaction for the revision, history, and cursor. Keep `.writer.lock` held for the writable store lifetime; read-only inspection and dry runs may coexist. Take live database snapshots through SQLite's backup API, never copy only an open main database file.

Repeat play IDs are scoped by Repeat node and allocation revision, with an ordinal inside that allocation. Preserve surviving IDs through resizing and reorder; allocate fresh IDs for growth and inserted subtrees. Imported initial snapshots reserve their allocation names even after plays are removed. Keep compact runs bounded and never expand a repeat merely to seek. Core schema 5 retains these runs, marks, and sparse overrides, and adds generated Hold metadata. Database schemas 1 through 6 replay the complete chronology directly into the current schema on a consistent copy, compare every legacy snapshot/transaction, and promote through SQLite's backup transaction only after validation. Strict legacy adapters freeze nested provider vocabulary and reject new fields, commands, and unexpected mark or override changes. Preserve the pre-migration backup.

Database schema 10 stores core schema 5 and retains operational generation requests,
attempts, validation receipts, and candidate selection. Modern bundle receipts add
optional measured spans and retained-input admission evidence; legacy receipts
gain none. Legacy requests retain no plan and remain protocol 1. Schema-7/8
history is validated without rewriting. Migration upgrades older authored
JSON through strict replay, preserves existing operational rows and clocks, and
adds only missing operational tables. Request versions belong
to retained per-Hold clocks outside document history. Undo/redo must never restore
request relevance or decrement a clock. Every document mutation with a current
request requires complete revision-bound context observations and reconciles
relevance in the same SQLite transaction. The store independently checks Hold
existence, duration, and project rate; the host resolves all media and generation
dependencies into the context hash. Unrelated edits preserve matching requests.
See [generation request storage](docs/GENERATION_REQUESTS.md) for the boundary.

Attempt state is operational, separate from authored undo/redo and request
relevance. A retry uses a new attempt identity and ordinal with the same immutable
request inputs. Only one attempt per request can be nonterminal. Writer reopen
validates the database before marking abandoned nonterminal attempts interrupted;
read-only inspection never performs recovery. Never infer a worker's ownership
from a stored PID. Cancellation acknowledgement keeps the attempt cancelling
until the host has stopped and reaped the worker. Progress stays in memory.
Candidate receipts record a trusted host validator's declaration; metadata alone
does not establish durable file ownership, media validity, or acceptability.
See [attempt storage](docs/GENERATION_ATTEMPTS.md).

Protocol-2 bridge workers declare native footage and provenance, never the final
sampled master. Qualify the complete bundle against persisted intent with
`deadpan-models`, after clean worker teardown. Use controlled artifact snapshots
and a separately host-selected provider capability; recheck its exact plan with
`BridgeGenerationPlan::validate_for` before media work. Retain that capability in
host provenance. Native/sample objects may deduplicate only when video contracts agree.
Capture the context manifest and both prepared frame byte streams before worker
launch with `capture_bridge_conditioning`. Keep the snapshots outside worker
control and pass them into qualification. Require the reported context to match;
host provenance schema 3 records all three input identities and measured spans. These are opaque
prepared bytes, not source-clock or color qualification. Publish their immutable
objects with the masters/provenance. Admission-bearing Ready receipts require all
six objects; legacy three-object receipts cannot be accepted. Source evidence and
history dependency inventory remain open. Never upgrade old envelopes by assertion.
Use one shared deadline and bounded provenance serialization. Preserve exact worker
provenance as claims alongside independent host decode reports. Conversion report 2
records actual final decoded duration; derive source spans from observed PTS plus
that duration, never synthesized frame-count duration. The store rechecks retained
object hashes before its transaction.
Ready and selection never authorize an authored provider change. Keep legacy
sampled receipts separate. See [bundle validation](docs/GENERATION_BUNDLES.md).

Generated-object publication is a filesystem operation before authored
acceptance, never an edit by itself. Use the store's descriptor-relative
`Media/Generated` API and algorithm-tagged BLAKE3 references. Verify bytes before
exclusive publication, synchronize the file and namespace, and preserve any
unreferenced object if a later database commit fails. A verified byte object is
not proof of canonical media or user acceptance. Read through verified snapshots;
do not reopen a worker path or treat generated objects as evictable cache entries.
See [generated-object storage](docs/GENERATED_MEDIA.md).

Generated Hold intent retains sampled/native master references, an immutable
provenance object, the original compact exact bridge sampling map, and a captured
Background/Freeze fallback. Shortening or re-extending within the original sampled
range reuses its prefix without resampling. Extending beyond that range restores
the fallback; the host must separately allocate a replacement request. Acceptance
and reversion are atomic core commands, including occurrence isolation. Generic
store creation/commands reject new generated artifacts. Use the dedicated
`preview_generation_acceptance` / `accept_generation_bundle` host APIs: caller IDs
and expectations only, assets derived from the persisted receipt, all six objects
verified, selection/relevance/revision rechecked, one atomic history transaction.
Keep the accepted request's resolved context in complete relevance observations.
Bridge request allocation and acceptance require one effective Hold occurrence;
isolate repeated occurrences first. Retime ancestors remain conservatively rejected.
Do not retroactively reject valid legacy operational bindings during migration.
Ready bundles alone do not authorize an edit. See [acceptance](docs/GENERATION_ACCEPTANCE.md).
See [generated Hold semantics](docs/GENERATED_HOLDS.md).

Original byte ownership is operational and separate from stream readiness.
Schema 10 adds content-keyed original records with monotonic location versions;
schemas 1 through 9 migrate without rewriting modern authored history. Use the
shared descriptor-relative object engine for `Media/Originals` and
`Media/Generated`. Managed originals try APFS clone, then verified copy; retain
the entire container with BLAKE3 addressing and SHA-256 source-index identity.
Relinking requires identical content and the expected location version. Never
delete a published object after a later database failure. Snapshots verify both
original checksums and retain private bytes. No original eviction exists yet.

The decoder's audio inventory is a probe observation, not an audio index or
measured endpoint. Do not turn unqualified audio into `AssetRecord.audio: None`:
asset metadata is immutable. Retain original bytes first, then qualify every
selected stream before authored registration. `SourceNode.link` means editorial
A/V linkage, not filesystem ownership. See [original media](docs/ORIGINAL_MEDIA.md).

`VerifiedSourceInput` shares one private byte snapshot between independent source
decoders. `AudioSession` retains physical PCM in a bounded temporary cache and
indexes original sample coordinates from raw PTS, measured frame duration and
explicit skip/discard evidence. Keep those observations separate from container
duration and codec-parameter padding. Unknown priming remains present until
qualified origin evidence establishes an editorial selection. Never infer the
offset fixture's leading trim from a codec-wide constant. Exact range reads fail
on gaps, excluded samples and unsupported clocks; they do not insert silence.
PCM cache access is media-thread work, not audio-callback work. See
[source audio](docs/SOURCE_AUDIO.md) for limits and qualification boundaries.
Audio container admission precedes FFmpeg parsing: bound declared packet sizes,
all-track sample/table expansion and nested metadata lengths, not only bytes
already read. Keep unqualified grammars rejected. Header validation and native
opening share one deadline and input-byte allowance; later decoder operations
receive their normal per-call budgets. Do not use process-global allocator
changes to enforce one decoder's limit.

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

Build the pinned FFmpeg developer prefix and export `DEADPAN_FFMPEG_PREFIX` as
described in [Development](docs/DEVELOPMENT.md). Run the exact repository gate
after implementation:

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

FFV1/Matroska container timestamps are not an exact authored clock. The qualified
FFmpeg muxer uses millisecond timestamps; retain the native rational frame rate,
frame ordinals, and sampling map separately. A complete pixel decode can succeed
after trailer truncation, so also verify immutable artifact length and hash.
The generated-video converter uses a separate private helper protocol: seekable
input/output descriptors carry media, one bounded JSON argument carries the
contract, and stderr carries one bounded JSON result. Do not route codec work
through the model worker's framed-message protocol. Native conversion stays on
the job service, outside UI/audio callbacks and database transactions. A returned
private FFV1 file is neither a Ready candidate bundle nor authored acceptance.
Derive bridge masters with `canonicalize_bridge` from one verified native input
and the original `BridgeSamplingMap`. The pair shares one hard deadline; native
scratch is bounded independently of output count. Use exact encoded-sRGB RGB8
linear half-up interpolation, then independently decode and compare the emitted
frames. Preserve the original map when shortening or re-extending an accepted
Hold; do not recompute it from the changed duration. The paired result still
requires complete provenance, candidate relevance, and explicit store admission.

For native startup or lifecycle changes, also run `cargo run -p deadpan-app -- --smoke-test` on supported Apple Silicon macOS. This checks startup and the shutdown callback, not media or accessibility qualification. Choose interactive checks for affected behavior when they add evidence; do not repeat them mechanically for unrelated changes. Add relevant media, persistence, worker, accessibility, or packaging checks as those systems are implemented. Record skipped checks and exact failures in the delivery report. [Development](docs/DEVELOPMENT.md) describes the workflow.

For authorized scoped work in this personal project, implement, review, verify, commit, and push to `main` using `git push`. Preserve concurrent changes and do not include unrelated files. Release publishing, signing, notarization, and external service actions need their applicable authorization; pushing source is not product release qualification.

Update these living instructions when implementation establishes a durable convention. Keep detailed procedures in the relevant documentation and keep the original specification intact; document any necessary product deviation explicitly.
