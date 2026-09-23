# Logical mark fragments, 2026-09-23

This implements the [mark binding lifecycle](../MARK_FRAGMENTS.md) needed by
structural Split, following `6420ecda`. One logical mark may retain multiple
physical owner/coordinate/state bindings. The Split builder, inserted-time
sample-resume contract and native mark workflow remain open. All DP requirements
and delivery gates remain open or partial.

## Behavior and review

The core transforms every binding atomically, preserves remaining bindings when
one is lost, and keeps unresolved coordinates from reattaching. Actual occurrence
isolation gathers eligible owned bindings into one fresh logical mark per copy.
Source clocks and external hosts preserve their meaning. Named resolution
requires explicit occurrence scope, retains all exactly coincident bindings,
and reports ambiguity before frame rounding. Left/right bias determines visibility
at internal Partition seams. Owner lifetime is independent of visibility.

The main agent reviewed the lifecycle and legacy adapter implementation and
tests. Two independent worker reviews covered anchor/basis behavior and migration
routing/persistence. No confirmed correctness finding remained. Review removed
an unsupported design assumption that owners needed visible same-play
correspondence with their coordinate hosts. Existing ownership is authored node
lifetime; the updated splice design and tests preserve that behavior.

Focused tests cover partial loss and primary promotion, mixed bound/unresolved
state, external-host ownership, copy subsets, shared bias, precise fractions,
seam endpoints, explicit actual-Source scope, billion-play bounded lookup,
exact-versus-rounded ambiguity, replacement, inverse patches and limits.
Exactly 100,000 old single-binding marks still upgrade without a new serialized
field. Modern documents admit up to 1,024 bindings per mark and 100,000 total;
growth beyond either bound fails atomically. Timed secondary bindings cannot
hide behind a source-clock primary to retain provisional project timing.

During development, compilation exposed an omitted error-code arm and Clippy
flagged the enlarged result enum. The error arm was added and optional named
metadata was boxed. A new Repeat fixture initially used the obsolete `plays`
wire field; it now uses only the current compact iteration order. These were
corrected before the complete repository gate. Its first test run then caught an
older limit test expecting the previous `exceeds mark limit` wording. Isolation
still rejected growth atomically; the assertion now checks the specific physical
binding-limit error. The full gate was rerun after that correction.

## Authentic migration evidence

Database schema 19 stores core schema 13. The strict legacy mark grammar is
shared by core schemas 3 through 12; old wires reject fragments even when the
value is `[]` or `null`. Core 12 preserves Partition purpose, while earlier
adapters continue to reject that vocabulary. All 18 old database schemas retain
their complete chronological meaning and operational state.

The [schema-18 fixture](../../crates/deadpan-store/tests/fixtures/v18-history.sql)
contains 10 revisions and five edits produced by the existing `6420ecda` CLI.
It includes a transparent partition, Local and concrete Occurrence marks,
an abandoned rename, deletion, undo and pending redo. Capture used SQLite's
backup API. No media bytes, local paths or personal data are included.

- Old CLI SHA-256: `7cb3a0793b791d421b3df325b61f90ec78254e2d0c6d8710f73572c216ebc685`.
- Fixture SHA-256: `78903284f3ac20d0c7022577fafbe4bb9b9a024a61240cb95e2ca7928a9a59be`.

Migration tests compare every snapshot and both patch directions, preserve the
old JSON after changing only its schema number, exercise pending redo after
reopen, and reject fragment vocabulary without changing the original database
or retained backup. Modern persistence tests preserve fragment loss through
commit, reopen, undo and redo and reject invalid replacement before any revision
is written. Earlier profile, qualification and operational-history tests remain
part of the full gate.

## Verification

The complete repository gate passed on Apple M5 Max: formatting, workspace
Clippy with warnings denied, all **927 tests** with zero failed or ignored,
workspace build, doctor and the native Metal smoke check. Source and fixture
SHA-256 hashes were unchanged throughout the final gate. [Command logs, source
hashes and counts](../../tools/media-qualification/evidence/2026-09-23-mark-fragments/summary.json)
are retained with this report. Doctor reports core 13/database 19 and continues
to identify incomplete application capabilities explicitly.

All eight design images/prompts verified against their manifest, all five
archived specification files remained byte-identical, and changed Markdown
links passed verification. This layer changes no native controls; it uses
headless behavioral tests. The prior [single-Original aesthetic and keyboard
review](single-original-2026-09-23.md) remains the applicable native UI evidence.
