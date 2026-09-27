# Continue the full Deadpan goal

This increment adds actual nested Sequence pause insertion, not a fractional
clock representation. Core 26/database 32 use the existing integer clocks for
unretimed Sequence ancestors. `insert_time_target` resolves the actual parent,
slot and precise Split ID count. Root transaction output stays unchanged;
frozen core 25/database 31 reject the newly admitted contexts during replay.

The command captures lattices before Split, fresh placements afterward and
reanchors later siblings at every Sequence ancestor. The new Hold stays under
its actual group. Native capture retains only lower picture scopes and leaves
all ancestors live. Completion retains the exact cursor; the root-only timeline
selects the enclosing visible group for a hidden Hold. Nested inspector editing
is still unavailable and must not be claimed from the typed-store framing tests.

Read the final gate, qualification and GUI report before relying on results.
The new `nested-pause` scenario is real keyboard/service/media/GPU code, with
typed fixture setup. A Metal startup failure is not a passing scenario.
The older GUI aesthetics, latency and accessibility findings remain open.

All full-product requirements and the active unbounded goal remain. Preserve
the concurrent harness and all ten ImageGen boards/prompts. Git metadata is
read-only; the verified checkpoint includes every pending tracked/untracked file.

Next authoring work should build another complete operation through the core,
store, plan, native host and relevant headless harness. Options with a direct
product benefit are nested Sequence navigation/inspection and integer-clock
Repeat occurrence insertion. Repeat insertion must isolate the selected play
outside-in, keep the Hold beneath the live Repeat, capture audio before copying,
retain stable play/gap identities and avoid expanding plays. It cannot be
implemented as root-level fragments that capture ancestors as static pictures.

Retime interiors still need the reviewed exact effective-clock design. Authored
integer Source/Hold/Retime values cannot simply be scaled to manufacture an
integer grid: this changes Preserve processing. Required consumers include
exact duration reducers, anchor/mark transforms, compact Repeat prefixes,
frozen audio timing vocabulary, owned live DSP input/output routes, ordinary
plan compilation, copy/edit lifecycle, guarded reversible patches and strict
legacy replay. The borrowed AudioStageProjection/AudioProjectedRoot APIs remain
useful; they do not persist routes or admit fractional authored clocks.

Any next schema boundary starts from core 26/database 32. Freeze current
Sequence-context admission before broadening it, preserve the authentic DB31
fixture and produce genuine current-version histories with a preserved binary.
Do not reuse the older core25/database31 proposal numbers from the preceding
checkpoint as the new current versions.

## Read-only native navigation map

The native implementer mapped this after source freeze. No implementation or
verification claim follows from the map.

Current rows are root-only in `preview.rs::rebuild_rows` and `root_children`.
Cursor/selection helpers and inspector admission assume that list. Service edit
validation, Split result selection and Delete neighbors search root children.
`insert()` hardcodes root even though the request already carries parent/index.
InsertTime is the exception: core resolves from the root cursor, then native
completion selects the visible root row. Camera finds the exact selected node's
InstancePath layer, but pending Camera state and commit lack a navigation scope.

Implement ephemeral `SequenceScopePath`, bounded by `MAX_DOCUMENT_DEPTH`, with
direct Sequence-owner edges beginning at root. Enter pushes only a selected
direct Sequence child; Backspace pops. Do not descend through Repeat or Retime.
Keep `sequence_cursor` absolute. Derive each scope's absolute interval and its
immediate child rows with absolute starts. Clamp h/l and gg/G to active bounds;
j/k choose immediate siblings. Enter preserves the cursor and chooses its child;
Backspace preserves the cursor and selects the exited group. Empty groups must
remain enterable and exitable. Show breadcrumbs, local/project clock distinction
and Enter/Backspace key hints. Plain Return is available in Normal mode;
Cmd+Return remains reserved and text/IME/command-field ownership takes priority.

Files: `navigation.rs` actions/bindings; `preview.rs` scope, input invalidation,
rows, timeline and inspector; `project.rs` request/commit context;
`project/service.rs` scope validation and sibling selection;
`preview/camera.rs` captured/revalidated scope. Service must check every scope
edge and direct-child target at the captured revision. Never admit Repeat plays
as an ordinary shared Sequence: those later need stable IterationId scope.

Use the validated current Sequence parent/index for `,i`; the current root
lookup would silently append a nested selection at root. InsertTime can retain
core target resolution, with completion preferring its exact visible target or
the current-scope row containing its captured cursor. Async completion must
retain captured scope/cursor, not reinterpret later navigation.

Open/new sessions reset to root. Refresh and Undo/Redo retain valid direct edges,
truncate missing edges to the nearest surviving parent, and resolve selection at
the unchanged absolute cursor. Preserve committed-revision deduplication.

Meaningful tests: Enter/Backspace at boundaries and empty groups; nested Hold
duration and Camera; stale scope/revision rejection; valid/invalid paths through
history; Repeat/Retime descendant refusal; `,i` parent placement. Extend the
`nested-pause` production-input scenario to enter Outer/Inner, inspect the Hold,
change duration, undo/redo and capture. The existing keyboard docs and board
prompt already call for Enter/Backspace. Update contextual help with the code.

Boundary refinement: `InsertTime` owns seams independently of UI navigation.
The resolver descends only at a strict Sequence interior. At an entered group's
start/end, it can resolve above the active scope. Preserve this core/wire rule.
Preflight the same target and admit it only when its parent is at or below the
active Sequence scope. If it resolves outside, explain that this is a group edge
and Backspace returns to the parent for insertion. Do not silently pop scope or
claim that the Hold will be inserted inside the entered group. Carry/recheck the
captured scope at service admission alongside revision validation. Test both
endpoints with no submitted edit, then Backspace and the same absolute boundary
inserting at the outer seam; test strict interior admission too. Empty groups
have no strict interior. This follows the existing structural design and avoids
inventing an authored insertion-scope wire contract in a navigation-only change.
