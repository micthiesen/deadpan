# Deadpan — implementation-agent handoff

Read version 1.1 of `DEADPAN_SPEC.md` as the current normative full-product specification. The imported 1.0 package is preserved in `archive/1.0/` and does not override the revised single-original V1 policy. Designs and examples are not implementation evidence; keep actual progress and measured capability in the requirement tracker.

## Product in one paragraph

Build a native macOS, Rust-first, Vim-style instrument for massaging one original video into a weird YTP. A project chooses one local or YouTube video and starts with its full unedited timeline automatically. Cuts, repeats, pauses, reframing and effects remain reversible structures on that original. Reuse moments from the same video, add external audio-only effects, and explicitly accept local AI Hold extensions. Do not offer additional video imports. New native projects live in Documents/Deadpan regardless of launch or source location. The full product still includes analysis, recovery, actual local generation and one-action source-derived YouTube output without external end-user runtimes.

## Read first

The current [sequence audition contract](../PLAYBACK.md) describes the native
Space Play/Pause increment, exact paused sample retention and the limited
edge-faded bus. It does not qualify the full mastered preview/export pipeline or
reduce the requirements below.

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
