# Copied sound clocks, 2026-10-03

Whole-beat and group copies now retain their attached sounds' processing history
and sample phase through paste and recopy. This advances DP-04 and DP-09. No
product requirement or release gate is complete.

## Behavior and supported scope

Core schema 46/database 58 stores one live processing scope and chronological
historical scope/owner references for each attached sound. A bounded paired
traversal proves equal clocks, structure, Repeat identities and processing
controls while permitting fresh node IDs. Both scopes have only ordinary
Sequence ancestors. Current policy, gain and edges remain separate from the
independent sound's raw processing clock.

Whole-owner capture appends the source's final placement, which was implicit in
its saved journal. That placement can contain clipped samples and must survive
when a pasted copy receives a new final placement. Selecting an inner ordinary
Sequence child narrows an enclosing scope to the matching historical subtree;
copying an outer group preserves an already smaller scope.

Paste allocates fresh live nodes, historical aliases and timing IDs. Proven
corresponding Repeat families share fresh play identities across their clocks.
Repeated pastes remain independent. Recopies append the new capture placement.
Supported later Sequence moves preserve earlier aliases and append their own
pre-edit references. Genuine whole-owner deletion removes live attachments and
clocks without modifying saved registers.

The planner binds each retained plan to the complete frozen layout, qualified
asset and saved recipe. An opaque token binds concrete current and historical
occurrences to those exact plan objects. Raw PCM keeps its complete nested
Preserve history and chronological clipping; current Hold gates and gain apply
after transport. All reads retain the existing work, deadline and residency
limits.

Register restoration recaptures the named immutable revision. Preview and commit
separately recheck source receipts and retained originals. The store tests cover
reopen, source-owner deletion, two independent pastes, recopy, and fresh-revision
Undo/Redo. Missing or changed registry evidence rejects without changing the
document, history counts or retained register contents. Restoring the evidence
permits the same paste.

## Review and corrections

Independent review found repeated complete scope comparisons for sounds sharing
one scope. Capture and Repeat-family inventory now cache proofs by the full
timing/historical-scope/live-scope tuple, validate every event's owner on cache
hits, and install each historical layout once. A regression copies all 64 sounds
in a scope containing 800 additional leaves, preserving their events and clocks
through validation, paste, serialization and inverse restoration. Re-review found
no further issues. A suspected failure when recopying an outer wrapper was
withdrawn after confirming that selection membership includes the complete
selected subtree.

The first two focused runs exposed test helper compilation errors: direct
deserialization of a validating frozen-layout type, an absent revision helper,
and indexing an iterator. The third run recorded 34 passes and six failures.
Four core fixtures used an incorrect absolute range, an outdated root-journal
shape, a removed local sound ID, or a pre-paste timing identity. The registry
corruption fixture hit SQLite's foreign-key constraint before exercising source
admission. Its separate injection connection now permits the deliberate damage
and checks restored referential integrity afterward. Production foreign-key
enforcement stays enabled.

The nested audio fixture exceeded the prepared-stage limit with one whole-window
read. Its correction uses bounded reads and distinguishes per-play PCM clipping
from current Hold gates. At fractional rates, copied raw samples and a current
Hold boundary can move by different rounded sample counts. Gain references must
also preserve per-contribution f64 mixing instead of scaling an already rounded
f32 bus.

Follow-up review confirmed the revised oracle. Frame 32 is exactly 30 frames,
or 48,048 samples, after the source at frame 2. The original allocation length
and boundary phase therefore return. Direct terminal-zero assertions verify
that clipped labels remain absent, independently of the comparison mask around
current Hold edges. The reviewer found no further issues.

Strict lint required grouping the new gate-query API's start/end policies,
removing a redundant clone of a Copy register name, and naming a test PCM type.

## Verification

- `cargo test --locked --workspace --no-fail-fast`: 3,600 passed, none failed
  or ignored, including two documentation tests.
- The focused sound-clock run: 40 passed, none failed or ignored.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.

All four final checks used the same unchanged source inventory:
`d4b70a5edab1e3dc2453483540f473c33f53c8b5571139bbac7471eb2013548c`.
The [retained reports](../../tools/media-qualification/evidence/2026-10-03-sound-slice-clocks/metadata.json)
include exact commands, durations, full compressed logs, source inventories,
review corrections and earlier failures. [SHA-256 checksums](../../tools/media-qualification/evidence/2026-10-03-sound-slice-clocks/SHA256SUMS)
cover every retained file.

## Environment and remaining work

Base: `16c650ecea0e234479a2e8cf7d3361c5cdfc12a5`.
Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust/Cargo 1.97.1,
locked dependencies, FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

Partial sound owners, edits inside surviving processing branches, root-owned
temporal transport, occurrence isolation, allowances, tails and native beat-sound
placement remain open. Complete `ib`/`ab` semantics still require that lifecycle.
Register structural restoration does not establish current media availability;
preview and commit perform the qualified source checks.

No native UI changed or interactive app was launched. Optional app configurations,
acoustic delivery and performance measurements were not checked in this increment.
Copied attached sounds have no emitted-movie equivalence check yet. Preserve
reference PCM uses the canonical DSP engine and does not independently qualify
its algorithm.
