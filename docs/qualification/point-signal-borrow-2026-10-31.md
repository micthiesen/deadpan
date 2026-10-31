# Borrowed Point signal in native reads, 2026-10-31

The synchronous native Point reader now borrows AudioPointDomain's stored
Structural signal through signal_ref(). It no longer creates an independent
signal copy for read_signal. The existing owning signal() method is unchanged.
The admitted raw-domain carrier remains alive throughout the borrow, nested
reads and final control check. No new lease or work charge is introduced.
Production caps, prior work weights, DSP and persisted formats stay unchanged.

This is a narrow copy removal after
[raw-domain admission](raw-domain-query-admission-2026-10-31.md), published as
c2ef0dba6f187271498737f3c9660e461becdb81. It does not admit the subsequent fresh
content or policy queries, or close a requirement or Gate.

## Complete before/after evidence

The existing immutable 77-operand corpus includes 18 complete private Point/
Structural signal payloads. The new test uses those actual operands and compares
every borrowed signal field with both its containing Point snapshot and the
frozen payload. All original Bound/input/caller values and setup outcomes remain
exact. It verifies the helper callback prefix and actual observed callback count,
caller state and released leases. No oracle or expected body is regenerated.

Both current optimized native captures replay all 744 requests and equal the
published raw-domain canonical body after removing only capture variant and
source digest. All 264 complete successful payloads and 480 complete typed
errors are identical, as are work counters, provider events/order, independent
constructor receipts and cleanup. The shared canonical body is 27,092,643 bytes,
SHA-256 164f9543d7db901b20a8d1a4d200a9853cd3a89e6fc0615aaf61f4ee2dee38c3.

The wrapper reuses the unchanged raw capture driver, retaining its historical
schema, earlier retained-checkpoint field and pre-raw oracle reference without
rewriting them. Separate capture proofs name this unit's actual preceding raw
checkpoint and complete current source/binary identities. Its before reference
is decoded and hash-checked from the published raw-domain inventory
93f5dc491cd9367bb5321ae0f042b709658f02146b50ea97a35fb21b18b30a44.

## Verification

Host: Apple M5 Max, Mac17,7, 128 GiB RAM, macOS 26.5.2, arm64,
Rust/Cargo 1.97.1. Plan uses the ordinary unoptimized incremental profile.
Native Audio uses optimization level 2, debug assertions and overflow checks,
qualified FFmpeg 8.0.3 and ORT 1.31.0. All 14 recorded native invocations run
serially. Every qualified run retains complete named source and binary
before/after guards.

Plan libraries pass 278 tests in Main and 265 in the isolated filesystem copy,
with 24 ignored each. The focused borrowed-payload test passes separately in
both. Audio libraries pass 437/431, with 10 ignored each, including the real
Root/Point carrier, source-error/cancellation, cache-hit and eviction tests.
Per root, all 29 targeted integration tests pass: physical Domain 12,
gap bindings 10, gap definitions 6 and the public Point selection/range case 1.
The complete 744-request native capture also passes in each root.

Main cargo xtask check --no-test, isolated explicit all-target Plan/Audio
Clippy with -D warnings, formatting, both crates' doctests and hygiene pass.
The isolated copy has no .git, so it uses the explicit lint selection.
Independent review found no issue in the four-file patch. No UI or release
bundle is rebuilt for this backend change.

The preceding raw-domain checkpoint's full 29-Plan/21-Audio executable matrix
and seven-case Split research are retained as earlier evidence, not rerun or
relabeled here. Its Split result was three pass/four work-budget failures in
239.25 seconds. General sound-bearing Split remains disabled.

All four staged source blobs equal both current roots. Main's 2,848 and the
isolated copy's 2,520 initial unowned named inputs remain exact; all 23 oracle
hashes are unchanged. Main's unrelated research remains unpublished.

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-31-point-signal-borrow/inventory.json)
contains 137 verified compressed records (36,939,413 stored bytes;
294,063,548 expanded), SHA-256
082247fafe2a7e833a9e3d6c083f7a98f032c736212d7bd0cfbc4fb57cc61671.
It pins the preceding raw inventory and retains complete new captures,
before/current source identities, binary pins, commands, streams, the review,
source preservation audit and scoped shipping patch. Executables are excluded;
every record is re-expanded and hash-checked.

Next admit the actual fresh Structural Point content query using its existing
internal traversal, after freezing its full ordinary query outputs, work, lookup
and errors. The 18 stored-signal snapshots do not establish generated-query
results. SignalSample policy, other Signal providers, Tape/Mix aggregation,
general Graph containers and complete recursive preparation remain open.
This change makes no allocation-peak, speed, RSS or listening claim.
