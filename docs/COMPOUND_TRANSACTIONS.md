# Resolved compound transactions

This is the shared execution boundary for a sequence of resolved edits and
register operations. It supplies atomic history for the semantic macro engine
required by specification §§6 and 7.4. The separate [semantic planner and native
recorder](SEMANTIC_MACROS.md) now supply frame motions, cuts and bounded calls.
The first supported
[semantic dot-repeat](SEMANTIC_REPEAT.md) repeats an ordinary frame cut through
the existing atomic cut path.

## Meaning and resolved execution

`Command::Compound` contains a `ResolvedTransaction`: an expected register-bank
version, frozen input register values, and ordered `ResolvedStep` entries.
Inputs record absence explicitly. The request still names its project, expected
timeline revision and fresh outer revision.

Steps are ordinary edits, yanks, cuts or pastes. Each editing leaf has its own
fresh allocation revision and an `AtomicCommand`. A leaf cannot contain another
Compound. The historical `Delete` command is unavailable as a new compound leaf;
use `DeleteRipple` so current audio clocks remain intact.

The executor resolves no keyboard gestures and reads no mutable macro at replay
time. A future semantic program must resolve selectors against each preceding
staged state, expand calls within bounded instruction fuel, and supply the exact
commands and fresh identities. Persisting those resolved commands keeps history
independent of subsequent changes to macro or copy registers.

## One authored result

Each leaf uses the same pure command reduction as an ordinary edit, including
its sound, allowance, mark and timing transforms. There is no second sound
transform around the compound. A late failure leaves the input untouched.

The executor calculates one forward patch from the entry document to the final
document, stamped with the outer revision, and one inverse patch. SQLite saves
one revision and one history row. Undo restores that net change with a fresh
revision. Intermediate editing revisions are allocations, not timeline states.

A named write updates its named slot and the unnamed slot. Later paste steps
read the staged bank. Exact cut content must match its pre-deletion selection;
paste must match its chosen staged input. Store admission verifies Original
ordinal mappings against measured qualification rather than trusting payloads.

Register writes publish one final bank version in the same SQLite transaction
as an authored result. A bank-only compound uses the dedicated compound API and
returns no authored commit. It leaves timeline history, redo and generation
relevance intact. The generic edit API requires an authored leaf because its
result promises an edit transaction.

## Intermediate copy provenance

SQLite schema 54 adds `transaction_steps`. Every editing leaf reserves its
allocation identity there, including leaves whose nodes disappear later.
Only states needed by a capture retain their full document. Those snapshots
must match deterministic replay of the owning compound exactly.

For example, start with `[A, B, C]`:

1. Repeat A three times in step S1.
2. Capture that Repeat into register a and cut it in step S2.
3. Paste a after C in step S3.
4. Save outer revision M and one history entry.

S1 retains a capture snapshot. S2 and S3 retain allocation reservations. Undo
restores `[A, B, C]`; register a continues to refer to the exact S1 Repeat.
Reopening can still validate and paste it.

`capture_snapshot_at` accepts committed revisions and validated capture
snapshots. `snapshot_at`, expected live revisions, history navigation, playback
and export continue to accept committed timeline revisions only. Native
register restoration and historical copy validation use the capture resolver;
new interactive captures must still target the live workspace.

Checkpoint retention follows immutable history, not the current register bank.
Undo, redo abandonment or replacing a register cannot discard a snapshot needed
by an earlier committed command. Register contents distinguish ordinary revision
provenance from step provenance.

## Admission and recovery

Before saving, the store checks both the current head and bank version. It
validates each staged transition through existing source, Generated-media,
sound, Hold-audio, Slip, Trim, Roll and single-Original admission rules. A
compound cannot hide an unqualified operation inside an otherwise valid final
document. Captured payloads grant no media authority.

Reopen validation checks the resolved command, its aggregate patch, exact step
reservations and required capture documents. Missing, extra, duplicate,
reordered or altered step rows fail. Captures may use an already reached local
state or an earlier immutable historical state; future references and cycles
fail. Step IDs cannot reuse timeline revisions or retained allocations, or
become live-head authority.

Preparation completes before any durable result is published. The SQLite
transaction covers history, reservations, capture snapshots, register writes,
redo changes and final generation relevance. Failure rolls them all back.
Receipts and the returned bank are prepared before commit, so a subsequent view
refresh cannot erase durable success.

## Bounds

The resolved execution limits are 1,024 steps, a 64 MiB canonical command,
64 MiB of retained capture snapshots and 512 MiB of staged-document bytes
processed per validation pass. Existing document, patch and 64 MiB register
content limits still apply. These are implementation bounds, not measured
latency or memory claims. Construction and decoding enforce these semantic
limits. The core Serde reader bounds decoded content; hosts must separately
bound raw input before deserialization, including whitespace and escaped text.
CLI request files, live IPC frames and stored history already enforce raw-byte
limits on their complete envelopes before parsing. Before a store commit, the
exact canonical request must also round-trip through the history reader. This
includes its decoded-value and depth limits, so a large directly constructed
Rust command cannot save history that reopening would reject. Semantic call
depth and macro-body limits belong
to the forthcoming semantic program layer.

Schema 53 development packages require recreation. This uses the user's
authorization to break unused development formats; historical migration
support remains separately qualified. The complete DP-06 workflow and Gates
A through G remain open.
