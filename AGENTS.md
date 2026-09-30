# Deadpan

Deadpan is a native macOS structural editor for massaging one original video into a weird YTP, built around editable beats and a keyboard-first workflow.

## Product authority and current scope

For the ongoing development goal/session, the user confirmed on 2026-09-30 that
the project has no users and will remain unused. Breaking project-format and
schema changes are authorized; omit migrations and compatibility with existing
development packages when that simplifies implementation. This overrides the
historical migration-preservation requirements below for new work during this
goal. Keep runtime correctness, truthful failures and eventual product recovery
requirements; do not spend effort removing existing compatibility solely to use
this permission.

Read [the full specification](docs/spec/DEADPAN_SPEC.md) and [agent handoff](docs/spec/AGENT_HANDOFF.md) before feature work. The Markdown specification is normative; summaries here do not reduce its scope. [Requirements](docs/REQUIREMENTS.md) tracks DP-01 through DP-24 and Gates A through G. Keep code, tests, evidence, and remaining work current there.

The current foundation includes validated beat documents, reversible structural commands, persistent marks with edit transforms, sparse per-play overrides and automatic nested occurrence isolation, stable repeat identities, exact indexed picture plans and boundary queries, SQLite project/history storage with schema migration, and a shared headless command entrypoint. The native UI creates one-Original projects in system Documents/Deadpan with an automatically initialized full-source baseline, opens legacy projects without changing their profile, registers audio into a separate sound catalog, reuses the whole Original, selects/copies half-open Original moments with v/y and atomically pastes with p/P at explicit Sequence slots, navigates ordinary Sequence groups with Enter/Backspace, splits their direct children at the cursor, wraps/updates Repeats, deletes selected beats, changes existing Hold durations, atomically inserts silent freezes into Source/Hold beats and their fragments under ordinary Sequence groups or at Sequence seams before composite suffixes, navigates durable undo/redo, and inspects exact Source/Sequence frames through a persistent decoder and shared SDR GPU pipeline. It is not yet a usable video editor or release candidate. All product requirements remain open or partial. A button, mock worker, downloaded model, ignored test, or proposed target does not prove implementation.

## Design philosophy

- Center V1 on one immutable Original. A new project starts with the entire video already on the timeline; cuts, repeats, holds, reframing and effects are reversible changes to that starting point. Reuse regions of the same video, allow external audio-only sound effects and accepted AI Hold extensions, and offer no additional source-video import in the V1 workspace. Preserve capable generic backend primitives and existing multi-video projects in an explicit compatibility view.
- Keep the original identity and full-source undo baseline in the authoritative SQLite workflow profile. Undoing edits or deleting every beat must never unlock a different Original. Create new native projects under the system Documents/Deadpan directory, independent of cwd and media location; preserve existing packages where they are. Initial picker cancellation creates nothing; interrupted preparation remains explicitly recoverable.
- Teach keys where actions live. Show ordinary action keycaps, exact pending prefixes and valid next keys, selection scope and a distinct focused-pane cue. Contextual searchable help is the secondary reference. Keep Original and edited clocks visibly separate.

Use the [imagegen interface boards and interaction contract](docs/design/README.md)
as visual targets for native UI work, especially the enlarged workspace target.
Keep the exact prompts and generated assets in the repository. Match hierarchy,
spacing, color, selection and keyboard discoverability while preserving normative
behavior. Review the coded GUI against the targets; generated labels, sample
measurements and pictured controls do not authorize invented capabilities.

Use the [Deadpan identity assets](docs/design/brand/README.md) for app branding.
Keep the editable Icon Composer layers, complete ICNS fallback, generated
references and exact prompts. Native bundles own their appearance-aware icon;
the embedded PNG is for bare executable launches. The developer bundle wrapper
does not establish signing, relocatable dependencies or release qualification.

- Preserve the editor's timing decisions. Use exact typed frame, sample, and source coordinates, half-open ranges, rational frame rates, checked arithmetic, and origin-based sample boundaries. Never accumulate rounded durations. Three plays means three total plays, with gaps only between them; a Hold inserts exactly its authored frames and preserves subsequent original speech.
- Framing reshapes the Original without changing its timing. Evaluate camera paths in their declared owner clock and compose them from provider to root on the canonical canvas. Preserve intermediate clips, curve ownership and the distinction between source-percent motion and canvas-percent values. A pause retains the cropped view entering its parent; inherited group framing stays live and applies once. Captured geometry belongs to the Hold recipe independently of provider changes and new Camera settings. Temporary Camera state is visibly unsaved; Enter commits once and Escape restores the entry state. Never replace an existing path with a static pose as an incidental consequence of opening Camera.
- Build composable structures. Source, Sequence, Hold, Repeat, and Retime form the small primitive set; attention, sound, captions, and cutaways attach to it. Gags expand to ordinary editable primitives. Keep repeats structural and occurrence identities stable.
- Make the keyboard workflow native and discoverable. Picture dominates the interface; controls explain current mode, context, selection, units, and scope. Respect macOS text editing, IME composition, focus, accessibility, and non-US layouts. A user must be able to complete the workflow without a mouse.
- Reserve Kestrel's global shortcuts. Whole-Original reuse is `,i` or `:insert`; Cmd+Return belongs to Kestrel. Keep plain Vim motions and visible pending prefixes while preserving native text editing. Any shortcut change must pass the production-router [compatibility audit](docs/KEYBINDING_COMPATIBILITY.md), compare the local Kestrel source when available, and update its controls, help and replay scenarios together. Treat keyboard, pointer, focus, layout, preview and response time as one interaction design.
- Preserve intentional dynamics. Silence, room tone, and permitted tails are different policies. Do not normalize individual words or quietly level breaths and pauses. Monitoring volume is independent of export gain.
- Route every input through typed commands, revision-aware resolution, validation, and atomic reversible transactions. Widgets, macros, CLI calls, and future agents share this path. No widget or background job mutates authored state directly.
- Keep the core independent of UI, media handles, databases, workers, and networking. Use focused typed modules and narrow provider boundaries rather than a general plugin framework.
- Keep ordinary editing immediate and local. Bound queues, memory, and work; prioritize audio and current-frame preview. No analysis or inference job may block a structural edit. Sources, transcripts, and generated footage stay local.
- Use one render plan and the same picture and DSP semantics for preview and export. Export one immutable committed revision and verify the emitted file. Never omit unsupported effects silently.
- Preserve the [measured encoder constraint](docs/qualification/encoder-timing-2026-09-28.md): native AAC with MP4 edit lists disabled shifts events 1,024 samples late in FFmpeg and fails the 60 fps bound. The [AVFoundation comparison](docs/qualification/native-audio-2026-09-28.md) loses opening events and shifts later events 1,088 samples early. The user approved §22.3 timing metadata on 2026-09-28: edit lists may represent encoder delay, padding and frame reordering, with explicit stream-start/sync and full emitted-file verification. Never hide failures through event-based PCM alignment, packet dropping or an AAC-block tolerance. Retain actual file/decoder evidence and qualify each output path explicitly.
- Make AI acceptance explicit. Inserting time immediately commits a deterministic fallback; actual local generation produces a candidate. Only an explicit undoable acceptance changes the provider. Stale jobs cannot overwrite newer edits, and accepted media must work without its model.
- Preserve user assets and history. SQLite is authoritative; JSON dumps are derived. Originals and accepted artifacts are not disposable caches. Recovery, migration, relinking, and worker failures must retain user intent and report failures truthfully.
- Measure capability. Record hardware, revisions, licenses, fixtures, failures, and actual results. Gate A qualifies media, GPU, audio, model, and packaging choices before large integration work. Performance targets and upstream benchmarks are not Deadpan measurements.

## Architecture and conventions

Current crates:

- `crates/deadpan-core`: exact time, validated documents, structural commands, and reversible patches; no I/O or identity generation.
- `crates/deadpan-store`: authoritative SQLite packages, immutable revisions, atomic writes, durable undo/redo, generation request relevance, persistent attempts and restart recovery, and database checkpoints.
- `crates/deadpan-plan`: immutable indexed picture mappings and bounded audio spans through structural beats, retaining original source identities, exact transforms and absolute sample boundaries; no decoding or DSP.
- `crates/deadpan-jobs`: bounded worker protocol, pure attempt lifecycle, process supervision, contained artifact snapshots, and exact bridge-generation planning. The real MLX adapter in `tools/model-qualification` is a development harness; app inference remains open.
- `crates/deadpan-models`: native bridge bundle qualification, retained inputs, measured source spans, and immutable host provenance. Model installation, app scheduling, audition, and application integration remain open.
- `crates/deadpan-media`: shared verified source snapshots, measured video/audio indexes, exact video seeks and bounded private PCM caches, plus isolated conversion, strict helper reports and private BLAKE3 output. No database or authored-state mutation.
- `native/deadpan-source`: separate persistent descriptor-only FFmpeg video/audio decoders, raw metadata, owned RGBA and original-rate interleaved f32. Unsafe code stays in this narrow adapter; unsupported interpretations fail explicitly.
- `native/deadpan-dsp`: bounded owned planar PCM and the canonical pinned stretch schedule through a safe Rust/C++ boundary. Construct it on a preparation worker; no device output or media decoding.
- `native/deadpan-output`: bounded prepared-PCM queue, delivery-clock intervals, generation-scoped stop tokens, owned sleep/wake observation and narrow macOS device boundary. Prepare a fresh channel-scoped generation, prefill, then explicitly activate; starvation and device faults never silently resume. No project/DSP ownership. See [output contract](docs/AUDIO_OUTPUT.md).
- `crates/deadpan-playback`: immutable-revision Original/edit/sound audition and bounded selection loops. Separate preparation and control workers own canonical PCM and monotonic device delivery. The UI never opens source media or owns the device; full mastering and acoustic/performance qualification remain open. See [audition contract](docs/PLAYBACK.md).
- `crates/deadpan-audio`: exact-phase source resampling, explicit speaker matrices, qualified PCM access, continuous Preserve preparation, informational meters and bounded finite oversampled limiting of the current edge-faded bus. Preparation/analysis is worker work; the playback crate owns device integration. Full voice graph, group mix and encoded master qualification remain open.
- `native/deadpan-fileclone`: bounded safe descriptor-clone interface around the macOS system call. The store owns copying, checksums, publication and durability.
- `native/deadpan-filesystem`: narrow descriptor-based APFS volume UUID adapter for publication recovery. The host owns path identities, file locks, hashing and publication; unsupported filesystems fail explicitly.
- `crates/deadpan-render`: bounded shared SDR picture pipeline, linear Rec.2020 working textures, explicit sRGB display transform, ordered framing/clipping, aspect and rotation. No decoding, document mutation or encoding.
- `native/deadpan-media-worker`: process-isolated FFmpeg conversion and independent decode verification through bounded descriptor-only AVIO. Only the documented FFI call permits unsafe Rust. Requires the explicitly selected pinned LGPL FFmpeg development prefix.
- `native/deadpan-encode`: bounded descriptor-only H.264/AAC MP4 encoder over composed I420 and canonical stereo PCM. Exact clocks, explicit hardware/software attempts, one shared deadline and restricted fast-start readback; no project, decoding, verification or publication ownership.
- `native/deadpan-process`: checked worker/leader teardown and Darwin group-membership adapter; unsafe is denied except for its documented bounded libproc call. Higher layers continue to forbid unsafe.
- `crates/deadpan-app`: native `egui`/`eframe` project workspace using Metal. One service owns the writable store, one import worker prepares media, and a separate bounded preview worker consumes immutable workspaces. Native dialogs, source registration, explicit insertion, history, current-depth Camera previews, limited Original/edit/sound audition and automatic SDR Render with explicit preview decisions are implemented. Full editing, mastered playback, complete export qualification and native render recovery remain open.
- `crates/deadpan-cli`: versioned headless project/command API, reused by `deadpan-app --headless`.

[Architecture](docs/ARCHITECTURE.md) records Section 24's full boundary map. Add crates only when an implemented responsibility needs isolation. Do not create empty crates or feature controls that pretend to work.

Use Rust 1.97.1 as pinned in `rust-toolchain.toml`, Cargo, rustfmt, Clippy, strong types, and meaningful unit/property tests. Keep platform-specific unsafe code inside qualified adapters and out of core. Avoid debug leftovers, broad suppressions, unchecked conversions, and unrelated dependency additions. Commit `Cargo.lock` and test locked dependencies.

All owned runtime subprocess launches use `deadpan_native_process::spawn`.
Its cooperative macOS guard covers pipe creation through spawn return to prevent
transient descriptor inheritance between participating launches. It does not
cover foreign launches, serialize worker execution or permit `pre_exec` callbacks.
Keep the deterministic raw/fixed pipe witness in `tools/process-qualification`.

Owned macOS worker groups use `deadpan_native_process::terminate_owned_group`
before reaping their leaders. A successful signal does not prove descendant
cleanup. Retain sole reaping ownership across membership checks and retries;
use a real monotonic cleanup deadline rather than an injected lifecycle clock.
Do not bypass an ownership or teardown failure with unchecked PID operations
in a destructor. Use the checked leader fallback when group cleanup fails, and
mark each reap attempted before waiting so an error can never retry a stale PID.
Drain bounded available control bytes through EOF or WouldBlock before judging
an exit-to-pipe grace period. Group cleanup does not contain escaped processes.

Source preview owns one persistent service thread, one replaceable pending request
and one reply. Keep hashing, snapshot copying, indexing and decoding off the UI;
cancel superseded work and reject stale source/request identities. Open local
files nonblocking before checking regular-file metadata. `SourceSession` retains
a private SHA-256-verified copy and an original-PTS index; validate native hard
limits before starting that copy. Random seeks preroll
metadata and convert only the selected frame. Native limits are cooperative,
not a preemptive wall-time guarantee. Never invent missing terminal duration or
accept an unqualified color interpretation silently.

Keep requested, decoded and displayed picture identities separate. Presentation
captures project session/revision and Source/Sequence coordinates, not just the
original source ordinal: a freeze or repeat can show that image at many sequence
positions. Admit only the latest stopped-frame reply; preserve an already accepted
picture while a newer request prepares. Advance the displayed caption and
accessibility label only with successful GPU submission or an explicit background
transition. Retain the old target until a resized replacement renders successfully.
Picture errors belong to presentation and clear on successful recovery. These
state transitions must remain testable without a native window.

Footer layout retries must preserve one outer frame's external state. Consume
service, playback and dialog updates once; defer Repeat continuation and picture
scheduling to the final pass, and do not render a discarded viewer size. Process
native text before closing command mode. Measure complete key-label pairs before
wrapped layout. See [footer qualification](docs/qualification/footer-layout-2026-09-27.md).

Measure changing bottom-panel text before its first paint, including wrapping
after resize; reuse that text geometry when drawing it. Keep empty structural
panels in the UI tree when omitting them would shift automatic widget IDs.
Zero-height inactive panels use no frame margins or separator; decoration can
expand the remaining parent space and let later panes overpaint the footer.
Visibility checks inspect actual paint clips and later opaque backgrounds as
well as accessible controls.

Audition uses the device's reported content intervals, never producer
progress as the heard clock. Retain past and future delivery reports; a terminal
callback's nonempty prefix remains pending until its reported playback deadline.
Tag pictures with the output generation and coalesce desired frames behind one
active decode/GPU submission. Stop invalidates pending work but retains the last
submitted picture and geometry. Pause/resume preserves the exact sample estimate;
navigation and edits discard it. Context-preserving stops never retarget an
inspector command. Device faults, lost reports and sleep/wake cannot silently
resume. Capture a distinct Original/Sequence domain and immutable half-open
window. Original descriptors are prepared off the UI thread from the full measured
A/V union, including audio lead/tail; interior picture coordinates use retained
PTS minus common origin, never project-frame ordinals. Keep selected-moment
endpoints separate: exact selections use first PTS/terminal video end and add
audio lead/tail only through explicit audition context. Cache descriptors by asset,
receipt and project rate, and canonical PCM by target as well as authored snapshot.
Loops keep monotonic delivery coordinates and wrap only the content coordinate;
retain full canonical DSP context across seams. Original playback cannot move
the edit cursor or grow Visual selection. Monitor gain follows canonical limiting and remains independent of
authored/export gain; never clip or normalize PCM to conceal a preparation failure.

Catalog Sound audition has a separate selected asset and exact sample cursor.
Only focused Sources routes its playback keys to that sound. Keep the retained
picture, caption, geometry, Original/edit cursors and beat/group selection intact;
sound delivery never requests a picture. Stop and revoke resume on sound, pane,
session or revision changes. Previously pending editor picture work may finish
normally; sound selection must not cancel or strand it. Its measured audio
endpoint, not the enclosing whole-frame beat, bounds ordinary playback and loops.
Reselecting the same registered video through Browse, its card or legacy Sources
navigation retains the Original cursor and displayed picture during refresh.
Reset the cursor only when selecting a different video or leaving a raw preview.
Use the shared qualified
playback service and temporary Source view; never persist an audition beat or
infer a missing speaker layout. Sound audition does not authorize placement.

Native sound placement uses a separate selected event and Placed sounds pane.
`,s` places the complete qualified catalog sound at the retained Edit cursor via
the revision-bound project service; it never adds picture time. Parameter entry
captures session, revision and event, including absence of an eligible target.
Late completions cannot supply or replace that target. Entering Placed sounds
from Original switches to Your edit while preserving both cursors and the beat.
Frame nudges translate the persisted exact mapping and audible selection while
retaining the independent sample offset; never round each nudge to samples.
Routed sounds permit gain/edge changes but reject repositioning until their
full timing lifecycle is implemented. `:sound-allow` and `:sound-silence` capture
the selected sound, project session, revision, retained Edit frame and exact
silent Hold or default Repeat-gap issuer on command entry. Missing targets and
late completions cannot retarget the command. Grant only where the selected
sound has retained sample support; permit revocation after support moves away.
Show the captured Edit frame and readable pause/play context; retain the complete
stable issuer in the command rather than exposing database IDs in the inspector.
See [sound events](docs/SOUND_EVENTS.md).

The shared picture baseline accepts owned, bounded, full-range straight RGBA8
with explicit transfer, primaries, SAR, rotation and source PTS. Decode transfer
before filtering into linear Rec.2020 `Rgba16Float`; preserve negative working
values until the explicit SDR display transform. Its encoded `Rgba8Unorm`
display texture is registered with egui without a second implicit sRGB decode.
Keep the registered target alive, release registrations on resize/shutdown,
and admit one picture submission at a time. This does not qualify HDR, physical
display color, editorial effects, playback or an encoded export path.

The [SDR encoder pixel boundary](docs/SDR_ENCODER_PIXELS.md) reads the composed
linear Rec.2020 working target, never the already encoded display texture.
Preserve signed values through the Rec.709 matrix before the explicit SDR clip
and OETF. Tight I420 uses limited range and the declared left-sited chroma
filter; do not substitute centered subsampling with left metadata. The host owns
output timestamps and completed-frame retention. A cancelled/dropped readback
keeps its single-flight permit until both mapping and submitted GPU work drain.
Run conversion/readback off the UI and preserve the immutable frame identity.
Library pixels and synthetic encoder fixtures do not qualify product export.

The [encoder picture host](docs/EXPORT_PICTURES.md) derives its output contract
from one committed picture session. Preserve the authored canvas for all framing;
map to a legal even raster only after composition. Output ordinals and rational
PTS are separate from absolute project frames and source PTS. Keep both absolute
project sample boundaries for a selected range. Admit one completed I420 result
at a time and retain cancelled GPU allocation permits until callbacks drain.
Run preparation off UI/audio threads. The [render worker](docs/RENDER_WORKER.md)
binds a full document hash and reconstructed output contract before child GPU
work. Admit its bounded raw output only after clean process/group/pipe teardown
and independent contained hash, geometry and code-range checks. Keep generation
and render wire messages separate over the shared process supervisor. Durable
render jobs, complete audio/effects, emitted-file verification and
publication remain required.

The [encoded render worker](docs/ENCODED_RENDER.md) binds separate picture and
audio readers to that same complete document hash before GPU/output allocation.
Feed one picture and one AAC input block directly under the native chronological
contract; never reuse the raw spool budget as a movie limit. Preserve the exact
absolute audio interval while rebasing only encoder PTS. Completion requires
native drain, same-descriptor hashing and clean supervised teardown, then a
contained host snapshot. A hash-admitted candidate is not verified media and
does not authorize publication. Keep encoded and raw wire contracts separate.

The [native encoder](docs/NATIVE_ENCODING.md) consumes consecutive authored
picture/sample clocks in exact chronological order and poisons every failed
session. Preserve B(end)-B(start), AAC priming/padding and video reorder metadata.
Its only secondary open is the same output descriptor for fast-start. Hardware
and OS software are separate attempts; requested B frames and queried codec
fields do not prove emitted behavior. Keep hardware B-frame PTS<DTS rejection
evidence. Independent finished-file admission is required before publication.

The [automatic encoder probe](docs/AUTOMATIC_ENCODER_ADMISSION.md) is separate
from project output. Require fresh bounded deterministic input at the actual
raster/rate, full emitted-file and expected-content checks, and explicit clean
teardown before selecting a path. Only exact admitted native kinds can permit
the next probe; later faults invalidate those claims. Preserve ordered rejected
attempts and exact sample observations. Serialized probe reports cannot restore
a live admission or replace a durable encoding decision and final-file verification.

Fresh automatic project encoding consumes that live admission once. Bind the
loaded helper and Avcodec/Avformat/Avutil/Swscale mapped vnodes and Mach-O UUIDs
to matching descriptors, hash those bytes before and after work, and retain the
descriptors throughout. Every eligible rejection and the project worker must
match the selected runtime. Missing or changed evidence invalidates selection;
serialized fingerprints grant no live authority. This assumes trusted installed
code and does not attest resident memory, OS frameworks or drivers. Preserve
frozen `EncodeContract::new_v1` controls and historical manifest grammar; new
defaults require a new policy identity. Database 42 retains the automatic policy
and each original encoding attempt's decision; checkpoint retries and publication
reconciliation preserve that decision while obtaining fresh verification.

The [committed project picture boundary](docs/PROJECT_PICTURES.md) captures an
explicit revision and nonempty half-open range through a read-only store.
Admit Original Source/Freeze frames against that revision's receipt, original
bytes and complete freshly measured index. Retain at most one private source
decoder; a hot snapshot survives linked-path loss, while cold admission must
revalidate it. Keep project frame/rate separate from original ordinal/PTS,
preserve odd committed canvases and carry provider-to-root/captured framing
unchanged through the shared native preview adapters. Authored Background/Blank
clears both renderer targets to opaque black; never use it for missing media.
Schema-3 Generated Holds use the shared cold reader and revocable generated
handle. Verify all six retained objects, strict durable provenance and both
asset records; freshly decode the sampled master against its complete canonical
interpretation, per-frame PTS and observed terminal duration. Carry the effective
artifact through Repeat gaps and overrides instead of looking up a current
candidate. Current request relevance, selection and allocation revision cannot
invalidate an accepted historical picture. Native cache identity includes the
session, artifact, both asset records and color policy; revision/framing changes
alone preserve it. Check handle liveness around warm and cold preparation.
Legacy Accepted providers without this evidence, Still and HDR fail explicitly.
Preparation is off the UI/audio threads and does not provide final-render isolation,
encoder geometry normalization, complete muxing or verified publication.

Every persisted edit, undo, and redo gets a never-reused revision ID. Core inverse patches can restore exact fixture identity; the store rebases them onto fresh revisions to prevent stale commands becoming valid after undo. Store writes use one transaction for the revision, history, and cursor. Keep `.writer.lock` held for the writable store lifetime; read-only inspection and dry runs may coexist. Take live database snapshots through SQLite's backup API, never copy only an open main database file.

Repeat play IDs are scoped by Repeat node and allocation revision, with an ordinal inside that allocation. Preserve surviving IDs through resizing and reorder; allocate fresh IDs for growth and inserted subtrees. Imported initial snapshots reserve their allocation names even after plays are removed. Keep compact runs bounded and never expand a repeat merely to seek. Core schema 33 retains these runs, marks, sparse overrides, generated Hold metadata, independent source mappings, audio edge policies, transparent Retime partitions and owned timing bindings, and binds qualified assets to immutable source receipts. Database schemas 1 through 38 replay the complete chronology directly into the current schema on a consistent copy, compare every legacy snapshot/transaction, and promote through SQLite's backup transaction only after validation. Strict legacy adapters freeze nested provider vocabulary and reject new fields, commands, and unexpected mark or override changes. Preserve the pre-migration backup.

Durable render jobs capture immutable project/revision/document/range/policy
intent outside authored undo/redo. Require fresh attempt IDs and cancellation
tokens, exact transition sequences and one active attempt per project. Retain
movie and strict manifest bytes under `Media/RenderCandidates`, independently
of Generated Hold objects. Worker handles contain no SQLite connection. Check
opaque session/attempt freshness around checkpoint commits; never hash full
movies on the writer. Reopen interrupts unfinished attempts and preserves
checkpoints. Explicit retries freshly hash retained objects and reconstruct the
historical contract before isolated verification. A saved Verified report grants
no live media or publication authority. Recheck handle liveness after verification.
Keep full integrity audits separate from bounded routine operations.
See [render jobs](docs/RENDER_JOBS.md).

Automatic intent version 2 pins `AutomaticSdrV1`; strict version 1 remains
engineering-only. Qualify on the stage worker while Queued, then insert the
immutable per-encoding decision and enter Encoding in one transaction. Consume
the matching live admission only after that commit. Keep runtime/probe/control
observations bounded and typed; serialized evidence never restores a capability.
Checkpoint retries and publication reconciliation resolve the original encoding
owner's decision and reverify fresh bytes. Cold encoding retries qualify again.
Freeze old intent grammar in both legacy jobs and nested publication records;
schema-42 migration adds no invented decisions and preserves all old cells.

Destination publication uses a separate operational journal and exact-stage
permits. Commit the phase, revoke the old permit, and require a checked direct
DB/WAL full-sync and namespace barrier before authorizing a rename. A failed
barrier revokes session publication permits and requires reopen. Stage and hash
files off the writer, publish the report before the movie, and preserve committed
movie knowledge through cancellation and later failures. Reopen never changes
destination files. Explicit reconciliation requires a fresh exact verification
attempt and APFS volume/birth/inode/path evidence plus complete byte hashes.
Never adopt a replacement merely because its bytes match. Keep recovered file
locks through the terminal journal write; missing or changed reports leave a
verified final movie PublishedUnconfirmed. Process-crash tests do not establish
physical power-loss behavior. See [publication](docs/RENDER_PUBLICATION.md).
Use the shared render coordinator for native service requests. Keep one workflow
lease through preflight, verification, publication and worker release. Persist
cancellation before signaling; preserve a racing movie commit through journal
failure. Unknown process cleanup cannot become Failed/Cancelled or release the
writer. Close/switch/shutdown must pump reliable completions while the old writer
is alive. Separate render status from editor command feedback. Native Render
captures preview decisions after field input and uses the exact durable commit
receipt. Closed-project headless Render owns its writer through cleanup; bounded
output failures and signals request cancellation without bypassing drain. Native
persisted-job recovery remains open.

Open-project commands use the [authenticated writer endpoint](docs/LIVE_PROJECT.md).
Bind discovery to the actual package and held writer; revoke it before unlocking.
Never log its secret, rediscover after admission, or replay after an unknown
delivery outcome. Preserve durable receipts across UI refresh and bounded reply
failures. Replies cannot consume or overwrite unread native commit continuations.
Remote Render refuses temporary previews and retains its exact target through
edits and cancellation; a separate canceller cannot release its observer's result.
Original retention, relinking, registration and checkpoints share the bounded
import worker. Keep the complete caller registration and stream choice through
preparation; never rebase a late result or borrow native selection. Retain the
worker slot across owner replacement until its reply drains. Operational
receipts are independent of authored revisions and survive compact replies,
workspace refresh failure and final stdout failure. Terminal cancellation needs
worker completion; a lost observer does not prove cancellation.

Database schema 42 stores core schema 33 and retains operational generation requests,
plus an optional validated single-Original workflow profile. Use the dedicated
`create_single_source` / `initialize_prepared_source` path to bind the full measured
Original, basis and protected baseline atomically. Undo never crosses that baseline;
deleting all current beats does not unlock a replacement video. Generic migrations
gain no profile. Native Open uses backed-up migration before replacing its current
session. See [the single-Original contract](docs/SINGLE_ORIGINAL.md).

The database also retains operational generation
attempts, validation receipts, and candidate selection. Modern bundle receipts add
optional measured spans and retained-input admission evidence; legacy receipts
gain none. Legacy requests retain no plan and remain protocol 1. Schema-7/8/9/10
history uses the frozen core schema-5 adapter; schema-11 history uses core schema 6; schema-12 uses frozen core schema 7; schema-13 uses frozen core schema 8; schema-14 uses frozen core schema 9; schema-15 uses frozen core schema 10 and retains presentation policy. All older nodes gain automatic audio edges. Schema-16/17 history uses frozen core schema 11; only ordinary Edit purpose is admitted. Migration preserves schema-17 workflow profiles and adds no invented single-source profile to older projects.
Migration upgrades authored
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
Database schema 42 retains content-keyed original records with monotonic location
versions introduced in schema 10; earlier schemas gain an empty inventory. Use the
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

Qualify selected streams from live decoded sessions sharing verified original
bytes. Serialized source qualification is stored evidence, never a fresh admission
token. Registration rechecks original bytes and binds canonical measured indexes,
source metadata and exact common origin to an immutable receipt. Commit the
receipt, asset, optional full-source insertion and relevance together through
`ImportSource`. Resolve source evidence by `(revision, asset)`, never the current
meaning of a reused alias. Validate receipt bindings across all historical
revisions, including abandoned branches. Legacy assets gain no invented evidence.
The first primary picture insertion chooses the basis only while provisional.
Choose the final rate before deriving Source placement; never rescale an already
rounded beat. Actual timed edits lock the rate, including audio/Hold insertion
and project-time marks; registration, source-clock marks and labels do not.
Persist origin and immutable first-primary identity; deleting content never
unlocks. Explicit canvas/primary geometry changes preserve all temporal values.
Keep basis and policy in one guarded reversible presentation patch. Old projects
remain explicit. Validate source-derived presentation against receipts across
all history. See [presentation basis](docs/PRESENTATION_BASIS.md) and
[source registration](docs/SOURCE_REGISTRATION.md).

Keep complete-file import preparation off the project writer as well as the UI.
Issue `OriginalImportHandle` from the writable store, prepare opaque retention or
snapshot results on an import worker, and admit them only in that same live
session. Prepared source qualification retains a private verified snapshot and
namespace freshness proof; metadata rechecks at commit must reject missing,
replaced or modified originals. Resolve authored targets, frame rate and relevance
against the current revision after preparation. Close revokes handles before
releasing the writer lock; handles never retain that lock. Retention may merge
ownership but cannot replace an established linked location; use explicit
versioned relinking. Prepared relinks retain verified replacement descriptors
and compare the complete captured record and version before the short inventory
commit. Checkpoint workers pin their own read-only SQLite transaction and copy
through the backup API. Publish only through their original owner, retaining the
actual snapshot revision and any published-but-unconfirmed durability receipt.
Never delete a renamed checkpoint or relabel it cancelled after a sync failure. See
[import preparation](docs/IMPORT_PREPARATION.md).

`SourceNode.audio_mapping` explicitly chooses `FitBeat` or an independent exact
project-frame duration. Use `SourceAudioMapping::natural_rate` for original-rate
selections and preserve signed `audio_offset` separately. Do not fit shorter audio
to picture duration implicitly. Legacy histories migrate to `FitBeat` to retain
their original meaning. Mapping commands use normal reversible transactions and
occurrence isolation. See [source audio mapping](docs/SOURCE_AUDIO_MAPPING.md).

Selected audio placements retain the complete measured integer-sample span and
its affine phase/rate, with a separate exact half-open selection before the mix
offset. Validate containment and shifted endpoints for typed and JSON callers.
Use measured video PTS boundaries to derive Original moments; round only their
beat occupancy, never their selected audio interval. Root, signal and bound
readers carry selection exhaustion on its physical clock, distinct from ordinary
placement gaps and silent Holds. Frozen audio context schema 2 retains the mapping;
schema 1 remains closed and matches historical content independently of version.
Native moment selection binds half-open original ordinals to session, asset and
receipt. Keep In/Out labels explicit and map the temporal bar through measured
PTS, not ordinal percentages. Copy never edits. Paste uses an explicit ordinary
Sequence owner/slot, including nested group edges, with one SpliceSource command.
The new Source starts unbound on the project-origin sample grid; independently
reanchor each old suffix owner. Admit the existing prepared receipt and final
Original freshness in the same history/relevance transaction. Never replace this
with generic child-index Insert or a separate receipt-registration preflight.
Preserve captured revision/scope through async preparation and return explicit
committed cursor/selection. Session-local copy is not persistent register support.
See [Original moments](docs/SOURCE_MOMENTS.md).

`SourceNode.video_mapping` independently chooses `FitBeat` or an exact duration
with an explicit endpoint policy. Natural-rate import uses the unrounded source
duration at project fps; integer beat rounding must not change picture speed.
Render plans retain the selected span and policy. Hold only presentation frames
intersecting that trim, never a later frame from the original. The measured index
must cover the entire selection, even when holding is permitted. Inverse source
anchors remain exact and reject out-of-host positions. Old histories gain
`FitBeat`, preserving their authored timing and existing audio mappings. See
[source picture mapping](docs/SOURCE_VIDEO_MAPPING.md).

Both source mapping enums also accept `Placement` with exact signed start and
positive duration in project frames. Audio adds its separate mix-clock offset;
picture sampling subtracts the start, while inverse source anchors add it.
Measured import candidates use the earliest selected stream start as a common
origin, preserve all original spans and available audio, and enclose the stream
union by rounding the beat end upward once. Never infer a missing priming trim.
Picture endpoint holding covers only the selected span. Cadence and even-raster
geometry results remain candidates with explicit evidence, not import readiness
or project basis adoption. See [import timing](docs/SOURCE_IMPORT_TIMING.md).

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
Source container admission precedes FFmpeg parsing: bound declared packet sizes,
all-track sample/table expansion and nested metadata lengths, not only bytes
already read. Keep unqualified grammars rejected. Header validation and native
opening share one deadline and input-byte allowance; later decoder operations
receive their normal per-call budgets. Do not use process-global allocator
changes to enforce one decoder's limit.

Video admission is a closed MP4/H.264 or finite Matroska/FFV1 grammar. Validate
FFV1 configuration expansion before opening the codec, and H.264 coded geometry
before its macroblock allocation. Never reintroduce an uncontrolled probing
decoder. Retain the controlled first frame for the caller and charge it to the
opening budgets. Container guards and post-demux packet guards serve different
allocation boundaries; preserve both. See [source admission](docs/SOURCE_ADMISSION.md)
for qualified limits and rejected grammars.

The setup workflow's TypeScript/Bun/mitools/Biome defaults do not apply to this Rust-native product. The maintained Rust sibling `beastie` supplies the initial workspace conventions; consult maintained siblings for evolving personal tooling patterns. [Dependency decisions](docs/DEPENDENCIES.md) records the pins and qualification boundaries. Do not introduce Bun, Node, Python, or shell setup as an end-user requirement. Future model workers use an app-managed private runtime selected through measurement.

Rapid explicit Repeat wraps may retain sixteen UI-owned waiting intents behind
one submitted command. Rebuild each successor only after adopting the exact
same-session committed wrapper, revision and Sequence scope. Never dispatch from
writer-idle alone, enlarge the writer mailbox, or replay a stale request. Keep
partial operator input through the chain's own commits; resolved other actions,
context changes, modal entry, blur, close and mismatched completions cancel the
waiting tail with a visible count. Each wrap has a separate durable undo step.
Keep this exception specific to explicit wraps; setters and unrelated commands
retain their normal rejection and revision behavior. See
[the workspace contract](docs/NATIVE_WORKSPACE.md).

The native UI submits typed project requests through one bounded service mailbox. Import preparation never owns SQLite. Cached insertion, current-depth editing and history may proceed while a new import prepares; an uncached insertion preserves its captured revision/target and fails if stale. Beat edits capture session, revision, absolute cursor and Sequence scope; reject non-direct targets and resolve through core/store commands. Consume explicit committed revisions and resulting selection, including an explicit clear, never infer completion from progress text. Preserve these markers across background updates and deduplicate them by revision. Repeat setters retain omitted gap parameters; explicit wrap-repeat always nests. Cancel pending keyboard operators when context, pane, selection or revision changes, except for a matching completion in the UI-owned explicit Repeat chain. Keep Source context non-destructive. Preview requests carry their immutable workspace, so cancellation of an earlier open cannot strand a later frame. Construct native dialogs on the main application thread, poll without blocking, and retain text focus until same-frame text and IME events are processed.

Keyboard routing precedes widget drawing. Use persistent `egui::Popup` state for
menu ownership; the current-pass `Context::any_popup_open` is empty at that point.
Help owns ordered input until Escape, including when it opens within one batch.
Discard its pointer/IME prefix on closing, preserve the command suffix, and keep
popup/dialog composition observation without consuming their input.

Native group navigation retains an ephemeral `SequenceScope` of direct ordinary
Sequence children. Keep cursor/card positions on the absolute project clock and
show the group-relative position separately. Restore captured scope before
applying a committed selection, including asynchronous Original reuse. Reconcile
history to the nearest surviving path without moving the cursor to the selected
beat. Root and scope-edge InsertTime still use core boundary ownership; refuse
an insertion above the viewed scope and teach Backspace. Full-edit audition may
leave the group: explicit pause/terminal completion follows its heard cursor,
while command/inspector stops preserve their target. Repeat/Retime occurrence
navigation remains open. See [group navigation](docs/GROUP_NAVIGATION.md).

## Validation and delivery

Retime edits keep input selection and output allocation distinct. WrapRetime
maps the complete selected beat beneath a fresh ordinary Retime. SetRetime changes
only duration and pitch on Edit purpose; wrap a Partition instead of rewriting
its retained clocks. A changed stage discards only its own output binding,
preserving descendant inputs and unrelated bindings. Identical parameters retain
all binding state. Native speed is exact decimal/rational input, resolved once
with ties-to-even against the retained input range, with explicit preserve/tape
pitch and displayed quantized duration. See [speed editing](docs/RETIME_EDITING.md).

Transparent Retime partitions are unity output selections over retained child
domains, not ordinary authored trims. Keep allocation, source sampling support
and envelope extent separate in every audio reader. Partition boundaries add no
DSP stage or fade; Source/placement and ordinary Edit crops still constrain
support. Preserve full short-envelope width, Preserve history and RoomTone phase.
Only matching meaningful edges inherit Sequence/Repeat edge policy. Exact Hold
resume anchors remain separate from pure Split. See [audio partitions](docs/AUDIO_PARTITIONS.md).

Consume `span.sampling` for PCM phase/rate, including prepared Preserve and
RoomTone output; structural transforms remain for frame coordinates and allocation.
Retain the owning grid's origin and round-even/point-ceil rule. Keep envelope
length/progress independent of allocation and gate exhausted envelopes in raw
reads as well as faded reads. These derived plan values do not persist Hold
resume intent. Future insertion must compose the current map and retain old
reference audibility alongside current silence; compact Repeat phase follows
stable play identity, not live ordinal. See [sampling clocks](docs/AUDIO_SAMPLING.md).

`FrozenAudioLayout` owns old timing aliases, never live node references or media.
Keep its flat closed vocabulary, compact play order, exact placement, edge policy
and local Tail maximum. Preflight JSON collection counts before materializing
owned values; capture precharges before cloning. The frozen-only aggregate run
limit is 100,000, independent of play count. Reference policy queries require
their own root or Preserve input/output clock handle; policy flattening does not
bypass DSP.
`RetainedRootPolicy` unions old `SilentHold` policy with current explicit
suppression and outside-domain silence, validating fully before mutating PCM.
Source absence and placement gaps must preserve existing processed decay.
These read APIs do not persist a Hold binding. Authored bindings transform
live ownership separately from frozen placement and evaluate current policy
independently of retained timing. See [audio references](docs/AUDIO_REFERENCE.md).

`FrozenAudioContext` adds exact Source mapping/offset, Hold/gap inputs and full
referenced asset contracts to a frozen timing layout. Keep its standalone closed
schema and aggregate wire bound. Borrow raw layout/input JSON until preflight
has bounded it. Preserve picture-only Source absence as such, never as an
explicit silent Hold; processed decay and Source edges differ. Compile directly
to an audio-only plan and reject picture evaluation. Do not fold signed offsets
into Placement if doing so narrows valid coordinates. Serialized contexts are
not admission: `source_for_context` defaults to rejection and all recursive,
RoomTone and cache-hit paths must verify the retained contract. The project
host compares the complete context to its exact immutable history before
using qualified original bytes. See [audio contexts](docs/AUDIO_CONTEXT.md).

`AudioDomain` borrows one plan and retains a physical Source, Hold/gap or opaque
Preserve context on its captured absolute root grid. Meaningful support may lie
outside visible Partitions or before root zero. Seed both processing and policy
queries at that physical subtree; whole-root lookup can select an unrelated
sibling at the same coordinate. Preserve ordinary Edit/placement constraints,
ancestor edge ownership and compact occurrence/gap identity. Domain PCM readers
must reject foreign handles and retain source admission, cache provenance and
full DSP history. `DomainSignalTransfer` rebases only allocated integer labels,
never frame origins or round-even boundaries, and shares one preparation budget
and deadline across its halo. Retain exact old silence before interpolation and
after transfer; current consuming-stage policies remain separately required.
See [physical audio domains](docs/AUDIO_PHYSICAL_DOMAINS.md).

New Repeat plays have no captured old occurrence. `AudioDefinition` selects an
authored Node or the actual Repeat default directly, including all-overridden
Repeats. Its local-zero point-ceil signal is explicitly branded by its selector;
relative paths and nested Preserve/RoomTone cache keys must retain that scope.
Never invent an outer play or use a neighboring root probe as a prototype.
Definition counts do not replace final root allocation. Retained sources still
require historical admission. Authored birth bindings use these explicit scopes;
definition inspection alone installs no binding.
See [audio definitions](docs/AUDIO_DEFINITIONS.md).

`AudioDefinition::in_root_clock` evaluates a current owned Source, Hold or
nonunity Preserve in a checked `AudioRootPlacement`. Keep the signed absolute
round-even grid, local support and definition scope explicit. A placement is
coordinate metadata, never an old raw recipe or media admission. Read current
policies from the selected plan; keep full intrinsic Preserve preparation and
source re-admission on cache hits. Do not treat a Sequence/Repeat as one domain
or infer authored bindings from this inspection API. Owned-tree bindings
separate intrinsic resume phase from enclosing Repeat placement and resolve
compact birth/survivor rules on their retained clocks.
See [owned audio clocks](docs/OWNED_AUDIO_CLOCKS.md).

Reference processing lookup stops at the first nonunity Preserve and retains
meaningful context separately from visible allocation. Borrowed physical-domain
identity includes its plan and clock; equal aliases across plans are not equal
domains. Root maps compose the current sample at a cut, while later domains keep
their own meaningful-start anchors. Preparation-clock handles cannot enter this
unit-rate root API. Split and occurrence copies retain authored audio lineage
under allocation-revision/origin tokens. Keep live ownership separate from those
historical names. Transparent structural edits preserve tokens; changed raw
contributions and their ancestors detach without resetting unrelated copies.
Prune deleted owners, include lineage in guarded patches and changed IDs, and
reserve initial allocation revisions after all owners disappear. Frozen layouts
retain the relationships but never infer them from matching media or timing.
Lineage alone does not authorize PCM or a live sample binding. See
[audio lineage](docs/AUDIO_LINEAGE.md).

`RootSignalTransfer` consumes already mapped raw root PCM. Apply explicit silence
and retained-envelope exhaustion to input taps before interpolation, then reapply
their exact point-grid audibility. Numeric zero or missing input never creates a
suppression policy. Keep creative fades after time mapping. StageAudio transfer
reads retain full root support and share one deadline, preparation-work budget
and source-provenance set across every halo chunk. Returned point samples need
their exact transfer and owning revision. These are conversion APIs, not authored
Hold bindings. See [signal transfer](docs/AUDIO_SIGNAL_TRANSFER.md).

`Split` retains full contexts and inserts sibling Partitions; refining a Partition
reuses its child domain and keeps repeated cuts shallow. Keep logical mark IDs,
map owner and host independently, and relocate concrete events once using their
exact position in the old target clock and seam bias. Never use outer query
visibility to choose their side. Preserve compact Repeat orders, override trees,
accepted artifact identities and unresolved coordinates. Caller identity and
binding budgets fail atomically. Native `s`/`:split` captures an interior local
boundary and selects the committed right fragment. These zero-delta checks do
not qualify shifted-fragment sample resume or automatic nested range planning.
See [Split](docs/STRUCTURAL_SPLIT.md).

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

Use `AnchorIndex::locate_boundary` for project-to-content descent. Retain every
owner's full clock, exact local boundary, authored Sequence slot, stable Repeat
path and distinct play/gap entry. Implicit gaps have no authored node; report
their Repeat owner and separate gap-local boundary. Outward project endpoints
have no provider, while inward bias descends. Share scope and prefix-comparison
budgets across the path, never expand plays, and never use picture centers or
quantize fractional Retime coordinates. The query does not choose an insertion
parent or isolate an occurrence. See [splice design](docs/STRUCTURAL_SPLICE_DESIGN.md).

Logical marks retain one primary binding plus bounded `MarkFragment` bindings.
Owner lifetime is independent of coordinate visibility. Transform loss per
binding; preserve surviving bindings and never reattach unresolved ones. Actual
occurrence copying collects eligible owned Local/Source bindings under one fresh
logical ID; pure Split must retain its existing logical ID. Apply Partition seam
bias before exact-coordinate deduplication. Named results retain every matching
binding ordinal within the resolved revision; do not use the representative
target to discard other attachment owners. See [mark bindings](docs/MARK_FRAGMENTS.md).
Database-18 history uses frozen core 12; all earlier mark wires reject fragments,
including empty arrays and null. Database 19 uses frozen core 13, including its
multi-binding mark vocabulary but excluding Split. Database 20 uses frozen core 14
including closed direct/occurrence Split identity pools. Database 21 uses frozen
core 15. Database 22 uses frozen core 16, including closed binding vocabulary but excluding InsertTime. Database 23 uses frozen core 17, excluding framing. Database 24 uses frozen core 18, excluding captured Hold geometry and its setter. Database 25 uses frozen core 19, excluding selected audio placements. Database 26 uses frozen core 20, excluding chronological reanchor steps. Database 27 uses frozen core 21, excluding gap binding maps and nested gap clock/placement vocabulary. Database 28 uses frozen core 22, excluding sparse gap branches and detached gap-clock references. Database 29 uses frozen core 23. Database 30 uses frozen core 24, retaining composite root-seam InsertTime admission but rejecting interiors before composite suffixes. Earlier replays check the stricter physical-suffix admission before modern apply. Database 31 uses frozen core 25, retaining root physical interiors but refusing nested Sequence insertion. Database 32 uses frozen core 26, retaining nested Sequence pause admission while rejecting SpliceSource. Database 33 uses frozen core 27, retaining SpliceSource while rejecting new Retime edits. Database 34 uses frozen core 28, retaining Retime commands but rejecting sound state and commands. Database 35 uses frozen core 29, retaining sound recipes while rejecting routed state, ReplaceSound and formerly forbidden sound-bearing temporal commands. Database 36 uses frozen core 30, retaining sound routes and ReplaceSound with their original contextual admission while rejecting allowance state and commands. Database 37 uses frozen core 31, preserving exact allowances but rejecting direct and occurrence Hold audio setters. Database 38 uses frozen core 32, retaining Hold audio setters while rejecting node audio treatments and their commands. Database 39 stores core 33; migration validates its history without authored JSON or patch rewrites. Schema 40 adds operational render jobs. Schema 41 adds publication records. Current schema 42 adds immutable automatic encoder decisions, preserves every schema-41 operational cell and also stores core 33. Legacy initial snapshots gain empty audio lineage; replayed copies may
establish it. Compare every old projected patch and changed-ID summary exactly
while retaining complete modern transactions for historical undo/redo.

Owned audio bindings contain flat timing records and bounded phase expressions,
not historical raw media bodies. Keep root round-even and selected-origin point
clocks distinct. Resolve births against retained lexical default roots and stable
play IDs; an overridden old play is not a surviving default contribution. Phase
terms retain their own clock because a shared local cut may round differently in
different plays. Transparent copies remap live arguments in every term, never
historical aliases. Reserve all retained allocation names and prune unused tables
atomically with owners. Legacy histories gain no invented bindings. Normal
StageAudio rendering resolves bindings on their retained root/point grids and
evaluates current owned policy independently on every consuming grid. Preserve
input bindings do not move creative fades before DSP. Fade geometry keeps a
retained virtual origin through later rate changes and translations; raw endpoint
audibility remains separate. Endpoint masks gate a physical grid's raw input;
do not turn them into output mutes across a later nonunity Preserve. Explicit
SilentHold policy still crosses that stage, including zero-input-point intervals.
Carry current Edit support and exact coincident
Hard owners into retained evaluation. Share work and relative cached-depth
admission across every nested read. Context-schema-1 capture and source-only
SequenceAudio still reject nonempty bindings. Pure capture retains existing
bindings and compact birth scope, including configured positive Repeat gaps even when no play renders them. InsertTime
authors one atomic Source/Hold splice under ordinary Sequence groups, including
existing fragments, and accepts Sequence seams before composite suffixes. Keep the prior
reducer for previously admitted inputs. For composite suffixes, capture fresh
current placement templates and append one chronological windowed step per
physical/default-gap owner, preserving prior lattice and resume intent. Stop
movement at the first nonunity Preserve output, retaining its input preparation
clock. Root Source/ordinary Hold interiors before composite suffixes capture
sampling lattices before Split and current placements afterward, under two
consecutive checked timing ordinals. Never overwrite an inherited lattice with
the copied graph. Never expand plays. Reanchor every shifted physical entry on
its own pre-edit clock, not only a newly cut suffix.
Current raw recipe extent governs reads and fades independently of retained
clock anchors; Hold duration edits must not reset RoomTone phase or captured
Repeat origins. Keep new phase-only layouts private until referenced, then prune
unused tables before validation. Nested Sequence insertion retains the actual parent and every live ancestor. Reanchor later siblings at each Sequence level; capture only picture scopes below the insertion parent. Native completion carries its exact cursor and scope and selects the visible enclosing child for a hidden Hold. Repeat/Retime and fractional cursor insertion,
occurrence isolation, Visual replacement and the complete authoring lifecycle
remain required. See [pause insertion](docs/INSERT_TIME.md).
See [owned bindings](docs/OWNED_AUDIO_BINDINGS.md).

Build the pinned FFmpeg developer prefix and export `DEADPAN_FFMPEG_PREFIX` as
described in [Development](docs/DEVELOPMENT.md). During implementation, run
focused tests and strict Clippy for the changed crates and affected dependants.
Finish independent review before one full workspace gate for a coherent delivery
milestone. Do not run the full gate after each small increment:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
```

The workspace tests already build and exercise the normal app, CLI and media
worker executables, including doctor. Use a separate build for packaging or
native startup work, and doctor when diagnosing the environment. Preserve both
base-app and optional `ui-harness` coverage when the app changes; their runtime
branches differ. CI runs both. Pure backend changes do not require repeating
unchanged app-feature tests or painted replay.

Keep `--no-fail-fast` on full milestone runs so one failed target does not
prevent independent targets and doctests from producing their results. The
command still fails if any target fails; fix and recheck the affected scope.

Keep one owner for Cargo execution. Before starting another command after an
interruption or quiet output, inspect the existing process and log; a harness
yield is not a test timeout. Preserve completed results. After a small review
fix, rerun the affected target and lint; do not restart unrelated passing suites
or rerun focused tests already covered by the full gate. Record source changes
and the scope of each result rather than discarding the entire run. Reproduce a
failure once with useful state diagnostics, fix its cause, then verify; do not
loop retries until green. Recheck an environment failure only when its relevant
capability changes. Prefer a Git commit and concise validation record over
repeated full-tree archives or new copies of gate scripts.

For cross-crate milestones, keep Cargo's package, target and feature selection
stable and narrow runtime test-name filters, for example
`cargo test --workspace --locked gain`. This retains the final workspace gate's
build graph while selecting the relevant assertions. Switching individual `-p`
or `--test` selections can recompile the large core against different
Serde/proc-macro dependency builds even when its source is unchanged. Preserve
those in-progress compilations; batch remaining checks instead of restarting
them. A small isolated change may still justify a narrower initial build.

After a test-only correction to a failed full run, preserve passing
targets whose source and Cargo artifacts are unchanged. A bounded continuation
may use the exact `--no-run --message-format=json` workspace inventory, package
working directories and required environment; never glob stale target binaries.
Record complete target coverage, source differences and exits, and run doctests
through Cargo separately. Do not describe the original failed invocation as green.

Playback real-media tests acquire the shared PCM permit before fixture work and
retain it through the engine's shared callback until both workers exit. Keep
that reservation bounded and keep production deadlines and PCM assertions
intact. `Engine::drop` and `Stopped` alone do not establish worker teardown.

Use the [UI feedback loop](docs/UI_FEEDBACK.md) for every meaningful UI change.
Add or update a scenario that replays real keyboard, pointer, wheel or text events
through the application, then run visual mode and inspect its contact sheets and
affected full-size frames against the [design targets](docs/design/README.md).
Run the separate release performance mode when input, layout, rendering, decoding
or service work can affect responsiveness. Preserve the actual input router,
widgets, project service and shared GPU picture path, including real video;
direct model calls or painted substitute screens do not establish UI behavior.
Keep simulated replay time separate from monotonic latency measurements, and
keep screenshots/readback out of measured performance samples. Never bless a
new visual baseline automatically to make a failure pass. Record assertions,
observed defects, artifact paths and untested boundaries; a green suite is not a
usability score. Use native computer interaction for evidence beyond offscreen
replay, including OS focus, dialogs, physical key delivery, IME, VoiceOver and
physical display behavior. Pure command, geometry, lifecycle and persistence
tests remain the fast foundation. [Interaction review](docs/INTERACTION_REVIEW.md)
records concrete improvements and personal shortcut compatibility.

The optional harness owns a repaint callback only on its fresh kittest context;
never replace native eframe's callback from application construction. Keep wake
state bounded, honor repaint delays/pass identity, preserve notifications during
steps, and use finite condition-variable waits instead of timer polling. Worker
start/finish/publication timestamps belong to feature-gated diagnostics, not
authored state. Keep their tickets through held delivery and exclude failed,
stale, repeated or invalid timing from successful phase distributions. Record the
wait/timing version when comparing reports; a faster harness does not by itself
establish improved application or physical display latency.

Audio preparation owns the DSP call schedule; device/export consumer block sizes
must not change it. Use origin-based input boundaries and explicit context/crops
for short clips. A fresh suffix render is not a restored phase state. Seek from
a matching checkpoint, exact canonical replay, or prepared PCM, and measure the
work and cache lifecycle. DSP preparation and file reads stay off the device
callback. The isolated canonical prototype is evidence for that boundary, not
application playback or listening qualification.

`RenderPlan::audio` allocates structural intervals by rounding their absolute
project-frame endpoints once to 48 kHz samples. Keep exact source transforms
separate from that allocation. Search half-sample thresholds with the proper
ties-to-even bias; ordinary picture-center or sample-boundary descent selects
the wrong leaf near a rounded edit. Retain every Retime pitch stage and Hold
audio policy, clip placement to structural hosts, and enforce both output-span
and traversal budgets. Query partitioning must not change allocated boundaries
or DSP origins. See [audio planning](docs/AUDIO_PLAN.md).

Root audio boundary metadata retains every coincident exact constraint, including
each ancestor's own occurrence path and stable Repeat gap identity. Never infer
an edge owner from a rounded sample, the leaf alone, or a query crop. Capture
boundary paths against the query work budget before cloning them. This metadata
does not choose precedence between authored policies or apply a fade.

Keep the qualification harness and `deadpan-dsp` on one canonical DSP header.
Vendored upstream headers remain byte-identical to their pins with full notices.
The Rust wrapper retains boxed input until after native destruction, bounds each
read and replay step, and rejects nonfinite/extreme input without normalization.
Its 1,048,576-frame input bound is explicit; do not fake long-clip support by
resetting the stretcher at arbitrary chunk boundaries. See [DSP](docs/AUDIO_DSP.md).

Use `CanonicalRecipe::with_rate` when an authored speed must remain independent
of rounded input/output allocation. Keep the legacy constructor and engine ID
stable for count-derived recipes. Both Rust and native entrypoints normalize and
bound the rational rate; absolute signed boundaries use integer arithmetic.
This API does not provide fractional phase. Prepare that phase with the sampler,
and process whole Retime occurrences continuously through their child cuts.
Outer crops must not reset inner stage history. Preserve ordered mixed pitch
policies. See [stage preparation](docs/AUDIO_STAGE_PREPARATION.md).

Use borrowed `AudioStage` handles and distinct `SignalSample` grids for continuous
retimes. Virtual point-grid storage uses ceil; final root allocation uses exact
absolute round-even boundaries. Neither storage count changes the authored rate.
Validate complete intrinsic output policies before input preparation, including
Holds with no input-grid samples. Reapply explicit silence after DSP and retain
its suppression metadata. Cache entries retain transitive source fingerprints
including full qualified index and chosen matrix layout; verify them on hits.
Share preparation work, residency and cancellation/deadline budgets across all
nested stages. Never reset an inner history to satisfy an outer crop or budget.

A `RepeatGap` audio definition selects the configured positive gap
recipe even in a one-play Repeat, with no invented preceding-play identity.
Its explicit root/PointCeil clocks keep gap duration separate from Repeat duration.
Actual occurrence domains retain their stable `gap_after`. The frozen support
projection stays inside one physical clock and rejects crossing a nonunity
Preserve. These readers do not install authored gap bindings or authorize splice.

Owned audio reanchor steps are chronological and retain each step's own clock
and lexical birth scope. Visible allocation includes every Partition; meaningful
raw support does not. An empty intersection has no entry and leaves the previous
sampling map intact. A narrower definition birth drops an enclosing window while
keeping intrinsic cuts; selecting the same retained root keeps its own window.
Old phase terms remain the initial state. Later InsertTime edits append after
existing steps rather than rewriting that initial state. Visit every step during
Split/isolation, pruning, allocation reservation and changed-owner reporting.
Terms and steps share the bounded count and aggregate work limits. Core-16-through-20
history rejects even empty/null reanchor vocabulary. See
[compact audio reanchors](docs/AUDIO_REANCHORS.md).

Repeat-gap audio bindings use a separate owner map and own-gap argument; the
gap's preceding play is not an outer `InstancePath` step. Clock scope includes
`NodeOutput` versus `RepeatGap`, so an enclosing Repeat window cannot clip a
new gap's canonical definition. Capture positive configured gaps even before
they render. Preserve surviving stable-gap clocks through reorder and duration
edits; former-final and new gaps use definition birth. Prune removed gaps and
never revive their bindings on re-add. Both walkers must intercept dynamic and
seeded gaps, retain current duration/policy and bypass only the selected recipe.
Historical clocks supply timing, never stale media or policy. See
[authored gap bindings](docs/GAP_AUDIO_BINDINGS.md).

Repeat gap branches are sparse ordinary owned subtrees keyed by the preceding
stable play. Keep final-play branches dormant, never append trailing time; retire
them only with that play identity. Default gap changes do not rewrite explicit
branches. An empty Sequence suppresses a gap; ClearGapOverride exposes the current
default. IsolateGap copies the current recipe and preserves marks and exact audio
clocks. Its Node-owned Hold may retain a historical RepeatGap reference with a
closed own-gap argument. Keep current raw policy separate from that timing record;
drop an enclosing reanchor window when closing a canonical born-gap dispatch.
The target's unique owned ancestry disambiguates gap and play InstancePaths.
Audio contexts are schema 3; legacy context layouts stay closed. See
[editable Repeat gaps](docs/REPEAT_GAP_BRANCHES.md).

Room tone uses an explicitly authored source range, never an inferred replacement
for silence. Preserve its exact 48 kHz extent separately from ceil storage.
The overlap period and short linear crossfade remain rational; derive each phase
from the absolute Hold origin without accumulating loop rounding. Full intrinsic
Hold/gap duration and gap-after identity survive plan crops. Room-tone and
Preserve preparation share cache provenance and work/residency admission. See
[room-tone audio](docs/ROOM_TONE_AUDIO.md). Native ordinary-Hold selection and
source audition retain one exact inward-snapped source-sample span for display,
audition and Apply. Success and failure preparation replies both carry the
request ticket, session and revision. Preserve a newer draft when an older reply
arrives. Source preview has its own zero-based audio clock and no implicit loop
context; it cannot move editor cursors, select a beat or schedule a picture.
Saved room-tone ranges win on reopen; replacing from a copied Original is
explicit. Field edits revoke audition until prepared again. Only Apply authors
the Hold, and native text/IME and focused-button ownership stay intact.

Informational audio meters consume contiguous fixed 48 kHz stereo PCM on an
analysis worker. Preserve filter history and zero-origin window alignment across
blocks; a new origin requires a fresh analysis. Keep silence/short-programme
results undefined instead of inventing a finite LUFS value. Only the peak meter
flushes finite zero context; loudness discards incomplete final windows. Keep
algorithm identities and frame/storage admission explicit. These meters change
no gain and do not qualify a limiter or final master. See
[audio measurement](docs/AUDIO_METERING.md).

Mastering follows the final policy-resolved voice/group bus and precedes monitor
gain and output conversion. Keep exact digital silence, tiny fragments and
intentional dynamics; do not use blanket fades or program normalization to pass
peak tests. Qualify the actual final f32 output under declared finite true-peak
paths, and retain broader complete-sinc failures as explicit diagnostic evidence.
These are separate claims, not a universal DAC theorem. Signed reconstruction
is not monotone under independent gain reduction. Earlier global LP and
finite-context experiments remain unadopted. `LimitedAudio` now feeds audition
and headless inspection with a pinned finite limiter after the current edge-faded
bus; this does not complete the missing voice, send, group or export paths.

Limiter cache tiles are absolute 8192-sample intervals, independent of playback,
seek, beat and request boundaries. Retain complete transitive source/layout
fingerprints and re-admit every hit through the exact immutable plan. One
`PreparationBudget` spans all tiles, cache checks, bus halos and kernel work in
a request. Pass its original deadline through final verification/publication.
Real context is required inside the project; only actual project endpoints permit
zero extension. Numerical FFT tile dependence is part of the halo proof. Verify
actual final f32 at owned reconstruction anchors, never temporary halo edges.
Reject an unverified tile before publication. Keep exact-zero and all-unity
shortcuts mathematically identical to the pinned finite gain recipe. See
[mastering qualification](docs/AUDIO_MASTERING.md).

The separate limiter input cache holds twelve exact bus ranges of at most 8192
frames. Split from the required context start and reuse only matching ranges;
never round outward to a cache grid. Unrequested source/effect support could
introduce a false failure or dependency. Re-admit every input-cache dependency
under the same request budget, and union those dependencies into the final tile.

Source resampling evaluates each original coordinate from its exact affine
origin, splitting the integer floor before float conversion. Keep fixed filter
order and versioned kernel/matrix/trim-context policies. Never reset phase at a
read boundary or derive speed from rounded allocated counts. A block reads one
bounded halo intersected with the authored trim; zero extension outside the trim
must not conceal unavailable selected PCM. Unknown speaker layouts require an
explicit interpretation, never a channel-count guess. Preserve source dynamics
and retain the chosen layout in preparation provenance. See
[source preparation](docs/AUDIO_PREPARATION.md).

`SequenceAudio` reads an immutable plan through a revision-aware source provider.
Anchor recipes to the full allocated span, never the current query. Convert exact
source clocks using the actual sample rate, and clip discrete PCM context with
`[ceil(start), ceil(end))` at exact structural edges. Historical assets resolve by
project/revision/asset and retain full receipt and original-byte verification.
Preflight unsupported processing before source I/O; never omit a Preserve stage
because another stage cancels its aggregate speed. Source-stage blocks explicitly
identify `source_pcm_before_effects`; they cannot stand in for the final mix.
The CLI and playback hosts each retain up to 16 private decoded PCM sessions under a 1 GiB aggregate
physical-sample budget, including padding, with least-recently-used eviction.
Bound aggregate indexed audio frames to 1,000,000 and retain compact receipt
identity instead of duplicate full indexes. Reserve both budgets before decoding.
Recheck captured asset, receipt and original contracts on every hit; verify cold
original bytes before eviction and reserve before decoding. Playback retains
shared immutable receipt identity and rebuilds on revision changes. Full
prepared-cache scheduling remains open. See
[source-stage audio](docs/SOURCE_STAGE_AUDIO.md).

Store hard-edge exceptions on their exact node/placement/gap owner. Any explicit
policy object retains all six fields; omit all-automatic objects so default
migration does not grow snapshot, request or patch JSON beyond its byte limit.
Legacy wires still reject the field even when its value is automatic or null.
Any explicit Hard at an exactly coincident boundary suppresses its one fade; Automatic is the
default, not a veto of another owner. Capture policies in the immutable plan.
Apply shared fades once after all time mapping, on flattened root allocations,
never inside a Preserve cache or at query chunk boundaries. Use sample-centered
linear edges with F=min(96,N/2), independent of enabled sides; one sample retains
unity. Preserve silent-Hold suppression metadata. Ungroup must not discard an
explicit wrapper choice silently. Keep raw and edge-faded PCM stages distinct;
neither is the final mix. See [audio edges](docs/AUDIO_EDGES.md).

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

Update these living instructions when implementation establishes a durable convention. Keep detailed procedures in the relevant documentation. Preserve the imported 1.0 package in docs/spec/archive/1.0 byte-for-byte; integrate explicitly authorized product revisions into the current normative docs/spec/DEADPAN_SPEC.md and update docs/SPEC_PROVENANCE.md. Current user direction and the current specification take precedence over archived designs.

Authored framing uses optional per-node static/enveloped canvas poses, exact owner
progress and explicit Q32 numeric interpolation. Keep provider-to-root scope
identities and intermediate clips in the shared picture path. Camera modifies one
evaluated operation on a matching retained picture; its geometry revision is
separate from decode/request/display identity. Never decode source media on each
Camera key, let stale drafts cross sessions, flatten existing curves implicitly,
or freeze bare source pixels while dropping authored framing. Captured Hold
geometry is a bounded sequence of canvas fits and static clipped operations,
separate from both `HoldVideo` and the Hold's live `BeatNode.framing`. It introduces
no media references or audio-context vocabulary. Provider changes, acceptance,
fallback restoration and duration edits preserve it. Native insertion
samples descendant operations at the frozen frame center and excludes its actual
Sequence parent and all ancestor framing. Same-canvas recapture may compact redundant identity clips but must
retain every meaningful intermediate clip, including an empty stage's implicit
clip. Keep explicit identity poses. The renderer checks finite, nondegenerate
cumulative geometry, independently of core collection limits. See
[captured framing](docs/CAPTURED_FRAMING.md) and
[framing](docs/FRAMING.md) for current bounds, migration and remaining work.

Framing's integer evaluator delegates to `evaluate_exact` for bounded evaluation
of derived rational owner extents. Keep segment selection exact; do not first
form a potentially overflowing local/duration quotient or duration-times-endpoint
product. The fixed 320-bit helper covers at most 294-bit products and Q32's
one-bit remainder shift. This numerical API does not admit fractional authored
durations or splice structure. Preserve integer document and command contracts
until the separate effective-clock representation is implemented and qualified.

`AudioSignalTape` is a borrowed input projection, not an authored edit or final
output allocation. Keep every run on one intrinsic PointCeil grid. Run windows
choose current providers; they must not reset sample phase, crop filter support
at allocation seams, or replace a span's physical allocation/sampling anchor.
Use the existing `StageAudio` reader with shared work, deadline and dependency
admission. Unchanged nested stages retain their full canonical preparation.
Use checked `AudioStageProjection` for changed intrinsic operands, retaining
complete input and an independent output-policy tape. Parent inputs reference child
intrinsic output; schedule inserted pauses separately. Reclock exact policy
before allocation and preserve processed decay past physical Source endpoints.
Use request-local retained projection identity for memoized PCM, re-admit
dependencies/depth on reuse, and charge retained results to the shared residency
budget. Do not put projected PCM into the ordinary descriptor cache. These
borrowed views do not persist routes. `AudioProjectedRoot` separately allocates
one physical projection on the absolute RoundEven grid; preserve its original
policy/support clock, exact PCM map and independent later-domain anchors across
crop/resume. Regrid exact policy before rounding, never scale prepared masks.
Keep exhausted retained support explicitly silent. Reject an unsupported output
sampling recipe before source preparation. These handles do not install authored
routes or replace ordinary root-plan evaluation. See
[input tapes](docs/AUDIO_INPUT_TAPES.md) and
[projected root output](docs/AUDIO_PROJECTED_ROOT.md).

`AudioSignalMix` combines complete scoped voices on one plan-local PointCeil
grid before creative effects. Apply source policy and gates to their named
voices; bus silence is the intersection of explicit raw silence and suppression,
never a union across voices. Keep opaque processed decay intact. Admit every
voice and nested history before media I/O, including fully gated contributions.
Use ordered finite f64 accumulation without clipping or normalization. A declared
aggregate processor may feed that sum to one Preserve. Default authored sound
voices retain independent continuous time/pitch processing and scoped output
gates before the group bus, as required by specification Section 10.2. Do not
split the existing continuous Original voice into independent per-beat engines.
Reuse the shared dependency, work, depth,
deadline, cancellation and PCM residency limits. This borrowed preparation API
does not persist a sound event or provide a final master; see
[sound event integration](docs/SOUND_EVENTS.md).

Retain every chronological sound-route sample grid. `AudioSoundRoute` resolves
an old cut and a new anchor on their respective physical grids, then composes
the preceding sampled output. Never flatten frame offsets to reconstruct PCM
phase, infer speed from allocated counts, or restart recipe support or fades at
a fragment. Keep/Window retain the old selection's half-open audible sample mask;
extra samples from displaced rounding are silent, while full filter/DSP context
remains available. Unity routes preserve grid spacing and rule; processing is separate.
Current `audio_hold_policy` queries retain each Hold/Repeat-gap issuer and its
definition/occurrence namespace without replacing the Original scalar policy.
Those rules do not invent retained historical identities or grant allowances.
These preparation interfaces are not persisted sound-event implementation.

`AudioSourceVoice` derives an independent catalog operand from a checked structural
owner. Keep its explicit natural-rate source mapping, signed mix offset, complete
filter/DSP support and opaque identity. A qualification ID in the plan is not host
admission: the media provider must still verify the revision, receipt, layout and
original. Its input keeps sound through current silent Holds; its output applies
only current scoped Hold rules on the consuming grid. Never inherit the Original's
source absence, endpoints, retained sampling bindings or edges as sound policy.
Preserve uses the existing checked descendant scope and independent output policy;
do not loosen those checks to fit a catalog asset. These borrowed operands do not
persist sound events or fill the final bus.

Persisted root `SoundEvent` recipes use qualified natural-rate source mappings,
exact selected intervals, independent sample offsets, owned gain and edges, and
explicit overflow rejection. `SetSound`/`DeleteSound` share reversible command
transactions; storage rechecks revision-bound receipt/original metadata before
commit. Rendering admits actual source bytes, including fully gated dependencies
on cache hits. Evaluate sounds directly on root RoundEven, apply per-voice edges
and gain, sum with the existing continuous Original, then use one shared limiter.
Source exhaustion cannot mute another contribution. Source-only Original and
catalog audition exclude authored overlays. Temporal edits and frozen-context
capture reject sounds unless the complete interval/sample-history transform is
implemented. Core 30/database 36 introduced separate root `sound_routes` journals:
original extent/grid, ordered Insert/Delete operations and cut intent. Root-clock
InsertTime, SpliceSource and ordinary Sequence Delete transform the bus once
outside helper Split; non-root Split is neutral. Root Split, temporal occurrence
edits and general retained sound-bus captures remain guarded. Keep the complete
recipe, physical sample labels and semantic boundary coordinates independent;
never reround an old label after an odd sample shift. Current Hold gates intersect
retained envelope boundaries once, selecting effective sides by physical labels
while exact provenance governs Hard coincidence. Keep virtual labels widened
and clip physical allocations before narrowing. Event survival follows retained
integral support; initially sampleless intent uses the exact logical fallback.
SetSound parameters preserve a route; explicit
ReplaceSound clears it atomically. Recheck unchanged recipes when their route
changes. Freeze database-35/core-29 contextual admission before modern replay.
Nested ownership, send/tail allowances, treatments and remaining structural
sound transforms remain required.

Core 31/database 37 retain a separate `sound_allowances` relation for exact root
contributions. `SetSoundAllowance` addresses one sound and one concrete silent
Hold occurrence or default Repeat gap, including every enclosing play and the
gap's stable preceding play. Do not promote definition-relative addresses into
root permissions. Prepare complete sound input before current per-contribution
Hold gates and authored edges; keep the Original's silence and every other
contribution's policy independent. Allowances neither bypass source admission or
exhaustion nor create media in retained routing gaps. Recheck receipt/original
metadata when only an allowance changes. Split and occurrence isolation preserve
or remap exact issuers; prune removed sounds/issuers in the same reversible
transaction. Freeze database-36/core-30 documents, patches, commands and contextual
admission; older snapshots gain no allowances. The full voice graph remains open.

`SetHoldAudio` changes only the selected Hold's audio policy. Preserve its exact
duration, picture/provider, framing, marks and retained sample clocks; let raw
audio lineage reconciliation invalidate changed processing content. After
occurrence isolation remaps identities, remove only the resolved Hold's obsolete
silence allowances in the same reversible patch. Explicit Silence does not
recreate removed permissions. Admit RoomTone/Tail sources against the expected
revision's qualified asset, stored receipt, Original binding and measured
integer sample span. Stored admission is not fresh byte verification; playback
uses verified snapshots. Keep unrelated legacy recipes valid and freeze core31
history before modern replay. Native Repeat-gap/fragment controls and tail DSP
remain open. See [room-tone authoring](docs/ROOM_TONE_AUDIO.md).

Gain preparation uses exact fixed owner-output coordinates and independent trim,
envelopes and mute ranges. Interpolate in dB; never normalize dynamics or use a
finite attenuation as a mute sentinel. Preserve continuous time/pitch and edge
processing, then apply gain before mixing/limiting. Owner-clock queries retain
current outer scopes separately from bound recipe clocks. Strict inspection
rejects missing clock support; the authored query explicitly marks known exhausted
retained support inactive and requires zero Original PCM there. Root sounds keep
their own current root gain. Default Repeat gaps never duplicate Repeat gain.
Persist node recipes through SetAudioTreatments and occurrence isolation; preserve
raw lineage, sample clocks, curves and configured unity intent. Retain treated
Partitions through refinement; reject unsupported treated Ungroup. Frozen context
schema 4 authenticates every nonempty owner recipe separately from timing-only
layouts; legacy context schemas 1 through 3 reject its field even null or empty.
Muted cached dependencies remain subject to admission. Native gain drafts retain
a distinct content identity, captured owner/revision and same delivered-sample
window. Writer previews never become the authoritative workspace or history;
receipts remain anchored to the committed entry snapshot. Admit preparation and
playback updates by their complete draft/change/session/revision identity.
Before/Draft switches retain heard content samples, including loop wraps, and
paused comparisons remain paused. Invalid text cannot disable Pause. Apply
rechecks the captured target and commits once; failures retain the draft.
Keep Apply/Cancel outside the scrollable editor and native Tab within its
enabled controls. Newly focused fields reveal their complete label/input pair;
forward and reverse traversal must reveal populated controls at both window
sizes without a pointer scroll. Reselecting the active comparison preserves its
generation and heard position. Cancellation preserves the accepted picture and restores
entry context only in the same session and revision. See
[gain contracts](docs/AUDIO_GAIN.md).

Measured gain waveforms belong to the captured committed beat before effects,
independently of Before/Draft mix audition. Keep signed stereo extrema, exact
owner clocks, terminal PointCeil clipping and unknown coverage distinct from
silence. Use one cumulative canonical preparation budget and one shared peak
allocation ledger whose reservations survive retained results. Analysis shares
the playback preparation owner; do not duplicate its media/DSP caches. Playback
cancels analysis, and only controller-confirmed device quiescence, including a
scheduled terminal prefix, permits new analysis. Admit every request afresh;
reject stale success and failure by complete identity. Analysis errors cannot
disable valid gain Apply or Pause. See [waveform contracts](docs/WAVEFORMS.md).

Bind retained sample routes only to checked complete providers. `AudioRoutedSignal`
uses independent source input or an immutable Preserve projection on PointCeil;
`AudioRoutedRoot` retains a complete projected output or checked raw
`AudioRootSource` on RoundEven. Match the
original Recipe extent, grid origin/spacing/rule and allocation. Reject cropped
or resumed captures, then select output through the route. Root Recipe frames
are relative to its original extent start but preserve absolute sample labels.
Resolve integral old sample labels and reuse the old provider's exact PCM under
one preparation budget; never reconstruct phase from destination frame endpoints.
Retain full filter/DSP support and admit dependencies even for a wholly masked
query. Captured output policy follows its old samples; current consuming Hold
gates, allowances and creative edges remain separate. See [routed preparation](docs/SOUND_EVENTS.md#routed-pcm-preparation).
