# Split without changing output

`Command::Split { node, at, identities }` cuts strictly inside a beat at an
integer local project-frame boundary. `OccurrenceEdit::Split` first isolates a
concrete repeated occurrence using the existing outer-to-inner transaction.
The cut has zero duration delta. All identities come from the caller; exhausted,
duplicate or reused pools and document/mark limits fail atomically.

The primitive keeps each side's full processing context beneath a transparent
Partition. Original media, source clocks, A/V linkage, signed placement, filter
support, original edge fades, Preserve history and room-tone phase remain intact.
The two output selections partition the old beat's range without creating a DSP
stage or another fade. Compact Repeat orders and sparse overrides are copied
structurally. An accepted generated artifact remains the same immutable artifact;
copying its beat neither generates nor accepts new media.

## Shape and mark identity

In a Sequence, the old child slot becomes two sibling Partitions. The old child
stays as the left full context and the right context is an owned subtree copy.
Splitting an existing Partition refines its range in place and inserts a sibling
over another full child copy. Repeated cuts therefore do not add wrapper depth.
A non-Sequence parent gets one enclosing Sequence. An explicit root cut retains
the root ID and moves its original content and edge policy into full contexts.

Logical marks keep their IDs. Owned Local/Source bindings gain the necessary
physical copies without clipping hidden retained coordinates. Owner and coordinate
host are mapped independently. Concrete occurrence events relocate once according
to their exact old position in the cut's local output clock; seam equality uses
the mark's insertion bias. Existing Partition visibility does not decide which
side owns that event. Unresolved coordinates retain their last authored address.
Root coordinates keep their full-root meaning. Every change is in the same
forward/inverse patch as the structure.

Core 15 also retains [audio lineage](AUDIO_LINEAGE.md) across the complete copied
contexts. Originals and copies share caller-revision-scoped tokens; refinement
reuses them. Picture and transparent grouping changes preserve the relationship,
while changed raw audio contexts detach without resetting unrelated copies.
This metadata is separate from the remaining authored sample-resume bindings.

## Native and headless entry

In Your edit, move inside the selected root beat with `h/l`, then press `s` or
submit `:split`. The inspector provides the same action and shows its key. The
service captures session, revision, target and local boundary; it resolves the
right fragment from the committed parent Sequence. The cursor stays at the cut,
the right fragment becomes selected and the stopped-frame picture refreshes.
Existing boundaries require no split. Counts on `s`, extra command arguments,
text entry, IME and held-key repetition do not dispatch another cut.

The versioned headless command envelope uses the same core/store path. Its command
payload is `{"command":"split","node":"beat","at":37,"identities":{"nodes":[...]}}`.
Provide fresh IDs for both output nodes and the copied context, with an additional
container where needed. Unused supplied IDs are not persisted. Use preview before
commit when a caller needs to inspect the exact patch.

Core 14/database 20 introduced the command grammar. Database 19 replays through
frozen core 13, including its complete multi-binding mark wire; schemas 1 through
19 reject new Split commands and preserve the pre-migration backup. Valid legacy
unit commands retain their JSON form but reject unexpected payload fields.
Current database 21 stores core 15. Its frozen core-14 adapter checks database-20
Split history and retains only relationships established by actual replayed
copies; initial legacy snapshots gain no inferred lineage.

## Evidence and remaining work

[Qualification](qualification/structural-split-2026-09-23.md) records core, native,
headless, migration, picture and actual PCM checks, independent review and native
interaction evidence. All requirements remain partial or open.

This command targets one explicit node or concrete occurrence. Automatic minimal
boundary-node selection for nested range operators, arbitrary cursor Hold
insertion and shifted-fragment sample resume remain required. A later insertion
must resume original audio at `B(f + N)` from the sample at `B(f)`; unchanged
output from pure Split does not establish that distinct contract. Range reuse,
sound placement, playback, effects, app generation and export also remain open.
