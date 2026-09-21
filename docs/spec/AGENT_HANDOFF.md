# Deadpan — implementation-agent handoff

Read `DEADPAN_SPEC.md` as the normative full-product specification. This package is a design deliverable, not an implemented application. Do not treat examples, pseudocode, or proposed performance targets as tested code or measured capability.

## Product in one paragraph

Build a native macOS, Rust-first, Vim-style video editor specialized for awkward timing, pauses, repetitions, reactions, selective audio emphasis, and reframing. Its document is a sequence of editable beats, not a flat pile of rendered clips. It includes actual local AI-generated silent holds, YouTube URL import, projects/recovery, and one-action source-derived YouTube-oriented output. End users install no external runtimes or tools.

## Read first

Read Sections 1–8 for product/primitive/keyboard semantics, 12–14 for AI contracts and qualification, 17–22 for rendering/runtime/storage/export, and 23–30 for dependencies, tests, requirements, and build gates. Section 31 resolves command targeting and source-browser behavior. Source references are in Section 33.

## Decisions already made

- Working name: Deadpan; `.deadpan` project directory packages.
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
- Full scope includes every DP-01 through DP-24 requirement. Ordered gates are not an MVP scope reduction.

## First concrete work

Create a dependency/architecture decision log and the workspace boundaries in Section 24. Run Gate A technical harnesses for actual macOS media decode/seek/encode, GPU preview, audio DSP/output, model inference, and private-runtime packaging. Record exact revisions, licenses, true output behavior, and measured hardware results. Do not spend the first implementation pass decorating a timeline while leaving timing, generation, and export unqualified.

In parallel, implement the pure core with generated fixture documents and property tests. Establish command resolution, exact duration math, reversible transactions, serialization, and render-plan inspection before connecting widgets to mutable state.

## Completion discipline

Maintain a requirement tracker mapping DP IDs to implementation, tests, and evidence. Every operation must be editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A UI button, mock worker, successful model download, or ignored test does not count as implementation.

If a dependency fails qualification, preserve the product contract and replace the implementation behind its interface. In particular, slow or unreliable video generation is a measured engineering issue: do not silently redefine AI holds as freeze frames or require a manually installed external application.

## Non-negotiable correctness points

`3riw` means three total plays. Repeat gaps occur only between plays. A Hold inserts exactly N project frames and resumes untouched original speech. Do not accumulate fractional-rate duration rounding. Jobs cannot overwrite newer edits. Accepted generated media remains usable without the model. Export snapshots cannot mix revisions. Cache cleanup cannot remove referenced originals or accepted artifacts.

## Delivery

Deliver the complete source, signed/notarized application distribution, approved model-pack manifests, clean-machine online/offline test evidence, benchmark report, keyboard guide, fixture/verification reports, migration policy, and third-party notices/SBOM. Any unfulfilled required behavior remains explicitly open rather than being described as finished.
