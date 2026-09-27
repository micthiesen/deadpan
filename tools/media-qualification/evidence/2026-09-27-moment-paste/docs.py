from pathlib import Path

def edit(name, replacements):
    path=Path(name); s=path.read_text()
    for old,new in replacements:
        if old not in s: raise ValueError((name,old))
        s=s.replace(old,new)
    path.write_text(s)

edit('README.md', [
('Native range reuse remains open.', 'Native `v`/`y` selection and `p`/`P` paste use one reversible Source splice in the selected Sequence group.'),
('Arbitrary cuts/range reuse, sound placement', 'Arbitrary range cuts/replacement, persistent registers, sound placement'),
('original after the selected beat. In Your edit,', 'original after the selected beat. For a moment, use `:source`, `v`, move with\n`h/l`, and `y`; return with `:sequence`, then `p` after or `P` before a beat.\nThe Out boundary is excluded; copying leaves the Original intact. In Your edit,'),
('schema-1-through-30 migration to schema 31', 'schema-1-through-32 migration to schema 33')])
edit('AGENTS.md', [
('reuses the whole Original, navigates ordinary', 'reuses the whole Original, selects/copies half-open Original moments with v/y and atomically pastes with p/P at explicit Sequence slots, navigates ordinary'),
('Core schema 26 retains', 'Core schema 27 retains'),
('Database schemas 1 through 31', 'Database schemas 1 through 32'),
('Database schema 32 stores core schema 26', 'Database schema 33 stores core schema 27'),
('Database schema 32 retains', 'Database schema 33 retains'),
('Current schema 32 stores core 26.', 'Database 32 uses frozen core 26, retaining nested Sequence pause admission while rejecting SpliceSource. Current schema 33 stores core 27.'),
('occurrence isolation, selected-moment payloads and the complete authoring lifecycle', 'occurrence isolation, Visual replacement and the complete authoring lifecycle'),
('See [Original moments](docs/SOURCE_MOMENTS.md).','''Native moment selection binds half-open original ordinals to session, asset and
receipt. Keep In/Out labels explicit and map the temporal bar through measured
PTS, not ordinal percentages. Copy never edits. Paste uses an explicit ordinary
Sequence owner/slot, including nested group edges, with one SpliceSource command.
The new Source starts unbound on the project-origin sample grid; independently
reanchor each old suffix owner. Admit the existing prepared receipt and final
Original freshness in the same history/relevance transaction. Never replace this
with generic child-index Insert or a separate receipt-registration preflight.
Preserve captured revision/scope through async preparation and return explicit
committed cursor/selection. Session-local copy is not persistent register support.
See [Original moments](docs/SOURCE_MOMENTS.md).''')])
edit('docs/ARCHITECTURE.md', [
('schemas 1 through 31 directly to schema 32 and core document schema 26', 'schemas 1 through 32 directly to schema 33 and core document schema 27'),
('SQLite schema 32, schema-1-through-31 migration', 'SQLite schema 33, schema-1-through-32 migration'),
('[Original moment candidates](SOURCE_MOMENTS.md) retain', '[Original moments](SOURCE_MOMENTS.md) use native Visual selection and atomic explicit-Sequence paste, and retain')])
edit('docs/HEADLESS.md', [
('Documents use schema 26.', 'Documents use schema 27.'),
('`ungroup`, `wrap_repeat`', '`ungroup`, `splice_source`, `wrap_repeat`'),
('[Original moments](SOURCE_MOMENTS.md). This does not yet add native range reuse.', '[Original moments](SOURCE_MOMENTS.md). Native range reuse derives this mapping from the prepared Original receipt.'),
('`Snapshots/before-schema-32-*.sqlite`', '`Snapshots/before-schema-33-*.sqlite`'),
('schema 32 and core document schema 26. Database-31', 'schema 33 and core document schema 27. Database-32 replays frozen core 26,\nretaining nested Sequence pause admission but rejecting `splice_source`. Database-31'),
('Optional per-node [framing]', '''`splice_source` accepts `parent`, `index`, `source`, `id`, `label` and `timing`
(`allocation` equal to the new revision and an ordinal). It inserts the supplied
Source at an explicit ordinary Sequence child slot and preserves each shifted
physical audio entry. The newly inserted Source starts on the canonical unbound
project grid. Repeat/Retime ancestors are rejected. This generic command can
reuse an existing asset offline; the native host instead derives the Source
through prepared-receipt admission. See [Original moments](SOURCE_MOMENTS.md).

Optional per-node [framing]''')])
edit('docs/design/README.md', [
('Sequence root-beat actions show their\nscope explicitly. Camera currently edits root-beat framing. Full selector,\noccurrence, saved-target, Trim, Visual and macro', 'Sequence actions show their current group\nscope explicitly. Camera edits selected beats at that depth. Original Visual\nselection and copied-moment paste follow the companion board. Full selector,\noccurrence, saved-target, Trim, Visual replacement and macro'),
('- The moment-reuse board is the target for the still-open native range workflow.', '- The moment-reuse board is the target for native Original selection and paste.'),
('`y` copies a ready Visual range; outside Visual it begins the normative\n  yank operator.', '`y` currently copies a ready Original range. The general Normal-mode yank\n  operator remains required; do not advertise it as implemented.')])
edit('docs/GROUP_NAVIGATION.md', [('selected-moment reuse and the full editing workflow', 'general cursor splicing and the full editing workflow')])
edit('docs/DEVELOPMENT.md', [('selected beat, or at sequence end. `⌘Z`', 'selected beat, or at sequence end. In Original, `v` plus h/l selects a\nhalf-open moment and `y` copies it. In Your edit, `p`/`P` paste after/before\nthe selected beat in the current ordinary Sequence group. `⌘Z`')])
edit('docs/UI_FEEDBACK.md', [
('| `nested-pause` |', '| `original-moment` | Selects Original [10,24) through v and counted h/l, copies with y, cancels selection with Escape, returns to Your edit, pastes after with p, undoes once, then pastes before with P. Checks exact range, unchanged copy revision, destination, selected Source and exact restored structure. |\n| `nested-pause` |')])
# Prior paragraphs describe distinct historical increments. State the newer boundary
# here and change their outdated current-status clauses without rewriting evidence.
edit('docs/REQUIREMENTS.md', [
('Native Visual selection, registers, named moments and atomic\nselected-moment reuse remain required.', 'Native selection and Sequence-slot paste now build on this layer; persistent\nregisters, named moments and the complete general splice remain required.'),
('General atomic moment\nsplice and the native Visual/register workflow remain open.', 'General cursor splice and the full register workflow remain open.'),
('Complete\nrange splice and native moment reuse remain required.', 'Complete\nrange splice and persistent register support remain required.'),
('splice, native range reuse and full media acceptance remain open.', 'splice, persistent registers and full media acceptance remain open.'),
('Arbitrary interior splice and selected-moment reuse remain open;', 'Arbitrary interior splice and Visual replacement remain open;'),
('selected-moment reuse remain required; no requirement or gate changes status.', 'Visual replacement remain required; no requirement or gate changes status.'),
('selected-moment reuse remain required. See', 'Visual replacement remain required. See'),
('[Exact boundary descent](STRUCTURAL_SPLICE_DESIGN.md#exact-boundary-descent)', '''[Original moment selection and paste](SOURCE_MOMENTS.md) add native v/y and p/P,
an identity-bound session copy, measured temporal range bar, and exact explicit
Sequence-slot insertion. Core 27/database 33 retain shifted audio entries and
admit prepared receipts atomically with history/relevance. Persistent/named
registers, Visual replacement, arbitrary occurrence/cursor splice and native
visual/performance acceptance remain open. No requirement or gate changes status.

[Exact boundary descent](STRUCTURAL_SPLICE_DESIGN.md#exact-boundary-descent)''')])
print('updated moment contract, current status, design and schema references')
