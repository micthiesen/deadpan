# Deadpan — implementation-agent handoff

Read version 1.1 of `DEADPAN_SPEC.md` as the current normative full-product specification. The imported 1.0 package is preserved in `archive/1.0/` and does not override the revised single-original V1 policy. Designs and examples are not implementation evidence; keep actual progress and measured capability in the requirement tracker.

## Product in one paragraph

Build a native macOS, Rust-first, Vim-style instrument for massaging one original video into a weird YTP. A project chooses one local or YouTube video and starts with its full unedited timeline automatically. Cuts, repeats, pauses, reframing and effects remain reversible structures on that original. Reuse moments from the same video, add external audio-only effects, and explicitly accept local AI Hold extensions. Do not offer additional video imports. New native projects live in Documents/Deadpan regardless of launch or source location. The full product still includes analysis, recovery, actual local generation and one-action source-derived YouTube output without external end-user runtimes.

## Read first

[Sound catalog audition](../PLAYBACK.md#sound-catalog-audition) uses the shared
canonical playback service with its own sample clock and leaves the retained
picture and edit selection untouched. Follow the
[sound design board](../design/boards/sound-audition-board-v1.png) for focused
selection, visible Space/Shift+Space keys and explicit state. It does not place
sound events. The [sound integration contract](../SOUND_EVENTS.md) distinguishes
node-owned clocks, continuous per-voice processing and scoped Hold allowances.
Its exact route kernel and CLI LRU PCM cache support the persisted root subset;
nested ownership, the remaining edit transforms, voice effects and full final
mixing remain required.
The [retained sample-route evaluator](../SOUND_EVENTS.md#retained-sample-routes)
now composes each edit's physical-grid cut and new anchor. Keep the old selected
audible mask separate from complete recipe/filter/DSP support. Current Hold
queries retain issuer identity; they grant no allowance and invent no historical
policy. Playback also uses bounded LRU source eviction. Complete per-voice
processing remains required. See
[sound-clock qualification](../qualification/sound-clocks-2026-09-27.md).
The [independent source operand](../SOUND_EVENTS.md#independent-catalog-source-operands)
now feeds qualified catalog audio through the existing tape and PCM engine without
adding a Source node. Its input retains complete DSP context; its output separately
applies current scoped silent-Hold rules. Preserve still requires the checked
owner/descendant relationship. Do not inherit Original bindings or use metadata
as permission to bypass host admission.
The [routed PCM readers](../SOUND_EVENTS.md#routed-pcm-preparation) connect retained
sample routes to complete source or projected providers on checked PointCeil and
RoundEven clocks. They read the old sample labels with full filter/DSP support,
including cold suffixes, and admit dependencies even for entirely masked output.
Captured provider policy stays separate from current consuming Hold gates. See
[routed-voice qualification](../qualification/routed-voices-2026-09-27.md).
Core 29/database 35 added qualified persisted root events and the shared
pre-master bus. Core 30/database 36 introduced chronological root sound routes
through InsertTime, SpliceSource and ordinary Sequence Delete. Non-root Split
is neutral; root Split and temporal occurrence edits remain guarded. Preserve
the complete recipe and old physical sample labels while current Hold gates
stay live. Parameter changes keep routing; explicit ReplaceSound discards it.
Frozen core 29 checks historical contextual admission before modern replay.
See [persisted root ripple edits](../SOUND_EVENTS.md#persisted-root-ripple-edits).
The [native root placement subset](../SOUND_EVENTS.md#native-root-placement) now
places the complete measured catalog sound with `,s` or `:sound-place` at the
retained Edit cursor. A separate Placed sounds pane owns event selection and
`j/k`, durable exact-frame `h/l` nudges, Enter for a whole 48 kHz sample onset,
3 dB `+` / `-` steps, soft/hard endpoints and `dd`. Edits use normal durable
history; picture duration never grows and overflow is rejected. Parameter entry
captures event/session/revision, including rejection when no event was captured.
Routed gain and edge edits retain their journal; native move/nudge rejects it.
Follow the [placement board](../design/boards/sound-placement-board-v2.png) and
the [native placement qualification record](../qualification/native-sound-placement-2026-09-27.md)
for the implemented subset and its verification limits. Nested ownership,
Repeat/Retime sound transforms, send/tail allowances, effects, the remaining
structural transforms and export remain required. No requirement or gate is
complete because these controls exist.

Core 31/database 37 add explicit per-sound, per-concrete-Hold allowances.
`:sound-allow` and `:sound-silence` capture the selected event, Edit frame,
issuer, session and revision. The writer re-resolves that scope and rechecks
source admission. Granting one sound leaves Original audio, other sounds and
other pauses suppressed. Raw preparation precedes current contribution gates;
an allowance cannot create sound in a retained route gap. Split and occurrence
isolation remap exact identities; database-36 histories replay through frozen
core 30 and gain no permission. See [sound allowances](../SOUND_EVENTS.md#persisted-root-sound-allowances).

Core 32/database 38 add [atomic Hold audio authoring](../ROOM_TONE_AUDIO.md).
`SetHoldAudio` and its occurrence form retain picture, duration and sample clocks,
reconcile changed audio lineage, and remove only that Hold's obsolete silence
permissions. Undo restores both policy and permissions. The store requires
revision-bound qualification and measured sample endpoints for new source
choices; unrelated legacy Hold recipes remain unchanged. Database-37 history
uses frozen core 31 with exact allowance comparison. The
[room-tone design board](../design/boards/room-tone-board-v2.png) now guides
`:room-tone` and `:hold-silence` for ordinary selected Holds. Copy Original time,
inspect inward-snapped source samples, audition on a separate audio-only clock,
then explicitly Apply. Native fields preserve IME and button/key ownership.
Reopen the saved range; replacing it from the captured copy is explicit.
Preparation successes and failures carry request/session/revision identities.
Waveforms, occurrence controls and acoustic qualification remain open.

[Gain contracts](../AUDIO_GAIN.md) add persisted node treatments, direct and
occurrence setters, exact owner clocks and canonical post-mapping PCM gain.
Core 33/database 39 replay database 38 through the closed core-32 adapter.
Context schema 4 retains a sparse treatment map separately from timing-only
`FrozenAudioLayout`. CLI `inspect-audio --authored-bus` exposes the pre-limiter
result. Native gain controls and Before/Draft audition remain open. Use the
[gain board](../design/boards/clip-gain-board-v2.png) before implementing its UI.
Keep gain after complete time/pitch mapping and edges, with exact independent
owner clocks and unchanged continuous Preserve history. Root-owned sounds receive
only their own gain and root treatments, never an unrelated Source's gain.
Temporary drafts need explicit content identity and the same delivered sample
window; a matching base revision alone cannot authorize cache/resume reuse.
Follow the [native gain integration design](../GAIN_EDITOR_DESIGN.md) for the
proposal/admission boundary and production-router verification.
The backend does not complete DP-09 or its native editing workflow.

[Structural speed editing](../RETIME_EDITING.md) exposes `:retime` and
`:wrap-retime` through exact speed resolution and the native inspector. Preserve
the input range when adjusting an ordinary Retime; wrap split Partitions instead.
Only a changed stage's own retained output binding resets. Descendant source and
DSP input bindings remain intact. Core 28/database 34 close the old command
vocabulary through `legacy_v27`; old histories cannot gain new speed operations.

The current [Original/edit audition contract](../PLAYBACK.md) describes the native
Space Play/Pause and Shift+Space selection-loop increment, exact paused sample
retention and the limited edge-faded bus. Whole Original playback includes its
full A/V union; selected moments use measured picture endpoints before adding
audition context. It does not qualify the full mastered preview/export pipeline or
reduce the requirements below.

[Authored framing and Camera](../FRAMING.md) records the current implementation
contract and remaining work, including saved targets and tracking.
[Captured framing](../CAPTURED_FRAMING.md) preserves a pause's input composition
separately from its provider and live Camera operations. Use the dedicated [Camera design board](../design/boards/camera-framing-board-v1.png)
alongside the primary workspace target. A temporary preview must remain distinct
from a committed edit, and opening Camera must preserve existing curves.

[Exact Original moments](../SOURCE_MOMENTS.md) records measured range candidates
and selected audio placements, including physical-grid endpoint behavior and the
core-20/database-26 migration boundary. Core-27/database-33 add atomic
`SpliceSource` at an explicit ordinary Sequence slot. Native `v`/`y` selects and
copies an Original range; `p`/`P` pastes after/before the selected beat. Keep the
copied session/asset/receipt identity and captured revision/scope through
preparation. Admit the prepared existing receipt and Original freshness in the
same history/relevance transaction. Do not use separate Split/Insert commits.
The new Source begins unbound on the canonical project grid while each old
suffix owner retains its sample entry. Frozen core 26 preserves DB32 nested
pause history and rejects the new command. Persistent registers, arbitrary
occurrence/cursor splice and Visual replacement remain required. The
`original-moment` harness exercises the keyboard path when Metal is available.

[Compact audio reanchors](../AUDIO_REANCHORS.md) adds core-21/database-27 ordered
per-occurrence resume steps and retained allocation queries. Distinguish hidden
allocation from meaningful raw support and preserve each step's lexical scope.
Core-22/database-28 [gap bindings](../GAP_AUDIO_BINDINGS.md) extend this ownership
to configured Repeat gaps, including gaps with no current occurrence. General
atomic moment splice remains required.

Core-23/database-29 [editable gap branches](../REPEAT_GAP_BRANCHES.md) retain
independent subtrees after stable plays and materialize current default gaps
without changing their audio clocks. Final-play branches stay dormant until a
following play exists. Use these owned structures in the general splice author;
the primitive commands do not yet resolve an arbitrary cursor insertion.

Core-24/database-30 [pause insertion](../INSERT_TIME.md) admits existing root
Sequence seams before composite suffixes. Capture current placements without
replacing retained lattices; append windowed steps and stop at nonunity Preserve
outputs. Every older replay checks the frozen contextual command boundary before
modern apply. Core-25/database-31 add root Source/ordinary Hold fragment interiors
before composite suffixes. Capture sampling before Split and placements afterward
under separate timing identities; preserve each occurrence's own rounded entry.
Frozen core 24 admits its old seams but refuses these new interiors. Arbitrary
nested splice and general Visual replacement remain required. The native editing
replay includes both pause-before-Repeat and interior-pause/undo paths.

Core-26/database-32 extend that command into unretimed Sequence groups. Use
`insert_time_target` for the exact native capture parent and required Split IDs.
Capture only scopes below that parent; keep every ancestor live. Reanchor later
siblings at each Sequence level, stopping at physical Preserve outputs. Frozen
core 25 refuses this broader context when replaying database 31. Native edit
completion carries the captured cursor. Native [Sequence navigation](../GROUP_NAVIGATION.md)
adds Enter/Backspace, breadcrumbs and direct-child editing at each ordinary
Sequence depth. A deeper Hold selects its visible enclosing group until entered.
The `nested-pause` harness exercises insertion, navigation, duration, history and
Camera when Metal is available. This does
not implement fractional clocks or insertion under Repeat/Retime ancestors.

Use `AnchorIndex::locate_boundary` for exact project-to-content descent. The
[headless query](../HEADLESS.md) exposes all owner clocks, Sequence slots, stable
Repeat identities, distinct play/gap entries and implicit-gap terminals under
shared work bounds. This query performs no mutation.
The [splice design](../STRUCTURAL_SPLICE_DESIGN.md#exact-boundary-descent) records
the required ownership and resume work that remains. Do not use picture-center
sampling or flatten a Repeat out of its live group to choose a splice target.

The preferred [derived-clock representation](../STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks)
keeps authored integer durations separate from exact effective extents, including
nested Retime output. It is not yet a document capability. The shared framing
evaluator now accepts exact derived extents without rounding or overflowing
intermediate quotients; existing integer callers use the same implementation.
Do not implement subtree duration dilation as an audio-preserving shortcut:
Preserve processing depends on its physical input grid and retained history.

The borrowed [audio input tape](../AUDIO_INPUT_TAPES.md) projects current scoped
signals onto one intrinsic PointCeil grid and reads actual PCM through the shared
`StageAudio` path. Allocation seams never restart phase or crop filter support.
Checked `AudioStageProjection` views retain full intrinsic input/output history;
parent tapes consume child intrinsic output, while a separate PointCeil schedule
can place pauses around it. A request-local identity memo prevents duplicate
preparation and descriptor aliasing.
[Projected root placement](../AUDIO_PROJECTED_ROOT.md) separately allocates one
physical projection on the absolute RoundEven grid, preserving phase, exact
policy and support exhaustion through crops and repeated resumes. These are
borrowed evaluation views, not authored splice routes: exact effective owner
clocks, persistent lifecycle, aggregate scheduling and normal root-plan
integration remain required.

Use the [UI feedback loop](../UI_FEEDBACK.md) for every meaningful interaction
change: replay production keyboard, pointer, wheel and text paths, inspect actual
Metal captures, and run the separate release latency checks. The
[interaction review](../INTERACTION_REVIEW.md) records measured friction and
priorities. Reserve Kestrel's global shortcuts; whole-Original reuse is `,i`,
while Cmd+Return belongs to Kestrel. Update routing, visible hints, help and replay
together. A passing replay does not establish native IME, VoiceOver or physical
display behavior.

Read Sections 1–8 for product/primitive/keyboard semantics, 12–14 for AI contracts and qualification, 17–22 for rendering/runtime/storage/export, and 23–30 for dependencies, tests, requirements, and build gates. Section 31 resolves command targeting and source-browser behavior. Source references are in Section 33.

## Decisions already made

- Working name: Deadpan; `.deadpan` project directory packages.
- One pinned Original per new native project, with a full-source starting timeline and protected undo baseline. SQLite retains this workflow identity independently of reversible presentation state.
- A global system Documents/Deadpan project library; initial source-picker cancellation creates nothing and failed preparation remains explicitly recoverable.
- Original/moments plus a separate sound-effects collection. Existing generic backend and legacy multi-video projects remain valid in an explicit compatibility workspace; never discard their data to fit the new UI.
- Visible keycaps, pending-prefix guidance and a distinct focused-pane cue teach ordinary actions. Searchable contextual help supplements the interface.
- Native Rust UI with egui/eframe/wgpu on Metal; no browser shell.
- New domain core, not a whole-app fork of a general editor.
- Pinned FFmpeg/native media adapter; qualify Cutlass components only if extraction reduces complexity.
- Exact rational frame/source timing and 48 kHz sample coordinates.
- Source/Sequence/Hold/Repeat/Retime nodes with stable IDs, anchors, occurrences, and attachments.
- Same rendering and DSP semantics in preview and export.
- SQLite authoritative storage; no competing mutable JSON document.
- AI worker language/backend chosen for measured usable-output latency, not MLX loyalty or Rust purity.
- Immediate committed freeze fallback; actual generated candidates require explicit acceptance.
- Bundle yt-dlp, its JavaScript/EJS support, and all executable runtimes; model weights install in-app or from an approved offline pack.
- Full V1 scope includes every revised DP-01 through DP-24 requirement. The single-original policy is intentional; ordered gates do not excuse missing required creative operations.

## First concrete work

Create a dependency/architecture decision log and the workspace boundaries in Section 24. Run Gate A technical harnesses for actual macOS media decode/seek/encode, GPU preview, audio DSP/output, model inference, and private-runtime packaging. Record exact revisions, licenses, true output behavior, and measured hardware results. Do not spend the first implementation pass decorating a timeline while leaving timing, generation, and export unqualified.

In parallel, implement the pure core with generated fixture documents and property tests. Establish command resolution, exact duration math, reversible transactions, serialization, and render-plan inspection before connecting widgets to mutable state.

## Completion discipline

Playback tests reserve real-media work before fixture preparation and retain
the permit through the shared engine callback until both workers exit. Use the
existing test helper for new PCM scenarios; `Engine::drop` and `Stopped` alone
do not establish teardown. The
[scheduling record](../qualification/playback-waits-2026-09-27.md) retains the
reproduced preparation timeouts and the scope of this test-only correction.

Maintain a requirement tracker mapping DP IDs to implementation, tests, and evidence. Every operation must be editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A UI button, mock worker, successful model download, or ignored test does not count as implementation.

If a dependency fails qualification, preserve the product contract and replace the implementation behind its interface. In particular, slow or unreliable video generation is a measured engineering issue: do not silently redefine AI holds as freeze frames or require a manually installed external application.

## Non-negotiable correctness points

`3riw` means three total plays. Repeat gaps occur only between plays. A Hold inserts exactly N project frames and resumes untouched original speech. Do not accumulate fractional-rate duration rounding. Jobs cannot overwrite newer edits. Accepted generated media remains usable without the model. Export snapshots cannot mix revisions. Cache cleanup cannot remove referenced originals or accepted artifacts.

Undo cannot erase the original identity or cross its initialization baseline.
Deleting all current beats does not make another video eligible. Importing sound
does not imply placing it, lengthening the edit or replacing original speech;
sound-event overlay requires its actual authored and mixing path. Do not relabel
a generic blank-picture audio beat as a placed effect. Maintain core structural
capability and strict legacy migration while enforcing V1 through the optional
profile and native workflow.

## Delivery

Deliver the complete source, signed/notarized application distribution, approved model-pack manifests, clean-machine online/offline test evidence, benchmark report, keyboard guide, fixture/verification reports, migration policy, and third-party notices/SBOM. Any unfulfilled required behavior remains explicitly open rather than being described as finished.
