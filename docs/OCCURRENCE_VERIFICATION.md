# Nested occurrence editing verification

This report covers `Edit one nested occurrence atomically`, based on
`83ba5ef19533edb729b4eb6d10ca5f0c5af08fb5`. It extends schema-4 sparse overrides
with node operations addressed through a complete nested `InstancePath`.
It also includes the gap-mark ownership correction based on
`40956523b68dd01e8fcadec0325390e671070694`.
DP-04, DP-21, and Gate B remain partial.

## Implemented behavior

[`EditOccurrence`](../crates/deadpan-core/src/occurrence_edit.rs) resolves a path
at the expected revision and isolates its repeated ancestors from outside
inward. Each selected default branch becomes an owned override subtree. An
already overridden branch is reused. Copies contain ordinary editable nodes and
any existing nested overrides; immutable source media stays shared. Compact play
orders retain their identities under fresh authored node IDs. There is no
expanded loop over rendered plays or frames.

The caller supplies fresh node and mark identity pools. The reducer consumes
nodes in structural preorder and marks in their current ID order, per copy.
Unused identities have no effect. Missing or colliding identities, stale/invalid
paths, invalid inner operations, and node/mark limit growth fail atomically.
The isolated tree and actual node operation form one patch and one history edit.
Undo restores original structure and marks; durable undo/redo still receive fresh
revision IDs. The document schema remains 4; this adds a command to its vocabulary.

Isolation itself preserves local durations, exact coordinates, source mappings,
and gaps. The normal mark transform then runs against the isolated tree for the
actual operation. This preserves ancestor-local marks inside a selected play
without rebinding them to neighboring content. Concrete occurrence marks follow
the copied path with their original mark IDs. Local and Source marks owned by a
copied node gain independent copied records. Original source coordinates remain
unchanged. Sequence-pinned marks remain single events at fixed project frames.
Unresolved records remain unresolved and retain their last coordinates.

Ownership controls inheritance even when owner and coordinate host differ. A
copied mark's external Local host stays external. An externally owned Local mark
referencing a copied host stays on its original authored host and does not gain
an implicit duplicate. This policy keeps ownership separate from coordinates;
the [headless API](HEADLESS.md#edits-to-one-nested-occurrence) documents it.

## Evidence

The [core integration tests](../crates/deadpan-core/tests/occurrence_edits.rs)
cover selected nested duration changes, unchanged default definitions, exact
neighbor/gap offsets, opposite boundary bias, explicit occurrence relocation,
Local/Source ownership, cross-host references, pinned and unresolved records,
existing-override reuse, reordered allocation runs, new play allocation, stale
requests, identity exhaustion after partial isolation, and invalid operations.
A generated small-reference test varies both Repeat counts, selected plays,
durations, and gaps. Successful test edits check inverse equality and JSON
round-trip. A unit test rejects copy growth from valid documents already at
100,000 marks or nodes before a partial result can escape.

Three independent [picture-plan tests](../crates/deadpan-plan/tests/occurrence_edits.rs)
compare original source coordinates before and after a selected nested wrap,
including copied existing overrides, exact fractional Retime composition, and
gaps. Old plans remain immutable and inverse plans restore their samples. A
`u32::MAX`-play outer Repeat with one edited nested play uses eight authored
nodes, four identity runs, eight duration segments, and two sparse overrides.
Selected, neighboring, and endpoint queries retain expected original source
coordinates. This is structural mapping evidence, not decoded playback or a
product latency benchmark.

Two independent [store integration tests](../crates/deadpan-store/tests/occurrence_edits.rs)
check cloned marks, reader/writer dry-run parity, exactly one edit/revision on
commit, reopened undo/redo, stale requests, failed identity pools, unchanged SQL
history after failure, and successful retry with the previously rejected
revision. The [CLI process test](../crates/deadpan-cli/tests/project_commands.rs)
uses the JSON command through the actual binary, compares dry-run and commit
patches, inspects the selected and unaffected picture instances, and restores
the edit through undo/redo.

## Local verification

On 2026-09-20, the complete repository gate passed on Apple M5 Max with 128 GiB
RAM, macOS 26.5.2 (25F84), Rust 1.97.1, locked dependencies, and bundled SQLite
3.53.2. Formatting, workspace Clippy with warnings denied, workspace tests,
workspace build, and `deadpan-cli doctor` passed. After the gap-mark correction,
there were **157 passing Rust tests**, none ignored: 82 core, 36 store, 24 plan,
13 CLI, and two native headless
tests. All six Python audio-measurement regression tests passed as well.
Diagnostics lists nested occurrence edits as partial and continues to identify
the unimplemented media, interactive editor, AI, export, and distribution paths.

General independent review found no actionable issues in occurrence isolation, command
dispatch and patching, mark relocation/copying, owned subtree traversal, and the
plan/store/CLI evidence against specification Sections 5.2–5.4 and 6.1–6.3.
The reviewer also reran the targeted core occurrence tests successfully.

A separate focused review found that an occurrence mark inside a Repeat gap
retained its original owner during isolation. The gap's stable play identity
was absent from the content point's ancestor map. Isolation now checks that
identity explicitly. The new regression failed before the fix with owner
`gap-n3` instead of `gap-n7`, then passed. Its eight cases cover inner/outer
gaps, both biases at gap edges, unaffected plays, and owner-loss policy when
either override is cleared. All edits check inverse equality and JSON round-trip.
The complete repository gate and six audio analyzer tests passed again after
the correction. The focused follow-up review found no remaining issues and
reran the regression successfully.

## Remaining work

This entrypoint applies the existing supported node operations to one complete
occurrence path. Their structural preconditions still apply. Partial-range
splitting/deletion/wrapping, multi-target moves, text selectors, temporal
attachments, role-only operations, explode, registers, and semantic macros remain
open. Render-plan rebuilding is not incremental.

No native UI or startup behavior changes. This verification stays headless;
native focus/IME, GUI aesthetics, accessibility, and natural keyboard navigation
remain required for the actual editor. Media decoding, audio output, inference,
export, packaging, and the remaining full-spec acceptance requirements also
remain open.
