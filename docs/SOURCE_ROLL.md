# Atomic adjacent Source Roll

Roll moves the shared boundary between two selected Source slices while keeping
their combined output duration and the project duration fixed. The shared
command, exact limits and persistence path are covered by
[backend qualification](qualification/source-roll-2026-10-01.md).
Native Trim mode remains required.

## Command and scope

`RollSources { parent, left, right, delta_frames, left_wrapper, right_wrapper,
timing }` captures an ordinary Sequence and two literally adjacent children in
their authored order. A positive delta moves the shared boundary later. Each
child must satisfy the qualified Source or single unity Partition admission used
by [Slip](SOURCE_SLIP.md) and [ripple Trim](SOURCE_TRIM.md). Source-owned framing,
audio treatments, exact windows, dormant audio and independent audio offsets
remain valid. Treated/nested Partitions, authored retimes, repeated occurrences
and audio-only picture lead/tail remain unsupported.

Each side retains its own asset and receipt. A generic project may contain two
different Originals; the single-Original profile still enforces its one asset.
An empty sibling between the selected children invalidates literal adjacency.
The command does not infer a neighbor from the current cursor.

## Exact shared movement

For adjacent output allocations `[T,U)` and `[U,V)`, the result is
`[T,U+d)` and `[U+d,V)`. The left Out edge and right In edge use the same `d`.
Intersect both exact Source-edge limit intervals before rounding inward to whole
frames or clamping the request. At least one output frame and positive exact
selected time remain on each side. An exclusive tied bound wins over an inclusive
bound; otherwise equal bounds report Left deterministically.

The resolution reports requested/applied movement, rational and integer bounds,
the controlling side and reason, the fixed pair interval, both seam positions,
and both full Source/allocation/window transitions. Positive movement can require
a crop wrapper on the right; negative movement can require one on the left.
An existing Partition retains its identity. At most one fresh wrapper is needed.
A zero result preserves both Sources exactly and returns no candidate transaction.

Physical Source ownership never shrinks. Extension beyond retained physical
context grows the left tail or right prefix as needed. Prefix growth translates
media mappings, gain/mute coordinates, binding references and physical-local
content marks together. Source framing retains its original owner clock and
endpoint poses; enclosing clocks keep their unchanged duration. Fractional
selection padding and hidden filtering context follow the shared Trim geometry.

## Audio, sounds and marks

Capture previously unbound audio clocks once from the unchanged document before
installing either candidate. The pair's surviving material keeps its absolute
placement, so Roll adds no ripple reanchor. Existing sample lattices, symbolic
resumes and chronological reanchors remain. Right-prefix translation rebases its
owned binding once. Independent offsets are subtracted only at their declared
mapping boundary. Exposed sample counts use absolute `B(end)-B(start)` intervals.

Record editorial audio edges on the two sides of the changed seam. Preserve the
pair's outside edges and the existing exactly coincident Hard policy precedence.
New fades use delivered incident samples independently of retained raw filtering
support. Existing hidden-context markers must not leak into later allocations.

The independent root sound bus is detached during structural capture and restored
exactly. Roll authors no insertion, deletion or replacement map for it. Existing
sound routes and silent-Hold allowances retain their identities.

Transform marks once against the final pair. Stored Source PTS and physical
content anchors retain their coordinate semantics, including behind a crop;
concrete occurrence queries can become unavailable. Ancestor-local marks follow
their existing loss and boundary-bias policies. Extension does not revive an
already unresolved mark. Undo restores the exact authored state.

## Store and headless path

`ProjectStore::preview_source_roll` resolves one captured database snapshot and
rechecks both stored receipts, asset records and Original ownership. It returns
descriptive `resolution` and nullable `edit`; preview data grants no media bypass.
Even zero checks revision freshness, an unused new revision, timing allocation,
wrapper absence and both receipts. A raw zero commit fails without saving.

The shared `command --dry-run` path reports `source_roll` alongside `edit` for
both cold and live writers. Commit re-resolves the original request through
the common preparation path and saves one revision/history/cursor transaction.
Reopen and durable Undo/Redo retain fresh revision identities.

Core schema 41 and database 50 identify this command. Unused development
databases 39 through 49 are refused without writes or migration under the
session's format policy. Existing frozen adapters remain closed to Roll.

## Remaining scope

Full Trim still requires native In/Out/Slip/Roll cycling,
outgoing/incoming boundary pictures, waveform, audition, explicit overwrite
policy, one Enter commit and Escape restoration. Mixed draft edits need one
combined timing and sound transformation; sequential saved scalar edits do not
satisfy that contract.
