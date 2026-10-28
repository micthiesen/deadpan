# Shared audio policy visits, 2026-10-27

The numerical reader now shares its final raw policy traversal and temporal
callback phases with metadata consumers. Contributions keep permission in a
separate, admitted bit carrier instead of inside PCM. This supplies production
building blocks for the cold demand proof; runtime population still comes from
actual reads, and graph reads still request one output.

## Boundary and accounting

The borrowed raw visitor dispatches silent Holds, retained envelopes, endpoint
exhaustion and explicit suppression in their original order. Numerical envelope
validation stays before exhaustion. Every explicit suppression range validates
before any such zeroing. Creative fades run afterward and never create a
permission mask. No child-mask union or source authority is inferred. Malformed
private flattened offsets now return Range where unchecked slicing/subtraction
could previously panic. The visitor allocates no metadata; production output
mask vectors remain. Two fixed exhaustion slots replace the per-span temporary
vector, and borrowed validation replaces temporary explicit offsets.

Components allocates one contiguous permission payload after residency admission
and before PCM. The complete carrier header/capacity is counted, including old
plus replacement payload during growth. Normal 256-frame routes use four u64
words; existing one- and 1,024-frame windows remain supported. Sample now holds
only its four f64 numerical values. Marks survive PCM allocation, and add marks
only after both channel finite checks. This is PCM-equivalent ownership
accounting, not an RSS or complete cold resource proof.

Both numerical engines consume the same EQ member and wet effect visitors at
their existing callback points. Outer constant/dynamic orders, gain flushes,
finite checks, masks, driver-only members, Tail exclusions, output aliases and
clock clone counts remain. Requests borrow the one selected phase clock.
Production inspection hooks replace existing walks with no added canonical
charge. Extra cold visits must bill each inspected member/effect/policy record,
including inactive probes; errors preserve partial charges and callers discard
provisional observations. No work weight, limit or source admission is weakened.

Fourteen new tests cover literal policy/PCM bits and ordering, invalid suppression
atomicity, error precedence, exact/partial work, cancellation, carrier allocation
and growth, one/1,024-frame masks, post-finite marking, metadata visits with empty
PCM, clock pointer identity, effect order and inactive probes. Existing scalar
PCM/callback, alias, mute, skip/stop and work/error oracles remain unchanged.

## Verification

Same qualified macOS host, Rust 1.97.1, pinned FFmpeg/ORT, locked dependencies,
opt-level 2, debug assertions and overflow checks as the preceding milestone.
Main guards 1,408 inputs; isolated guards 1,180. Exactly four existing files
change and six are added. All other frozen inputs match, and all ten
published files match the isolated source byte for byte, including stages.rs
without inherited main WIP.

| Check | Result |
| --- | --- |
| Final main / isolated builds | Pass, 85.18 / 82.87 seconds. |
| Final main Audio library | 231 pass, zero ignored. |
| Complete main Audio definition | 171 pass, zero ignored, 259.72 seconds. |
| Enabled real-WAV stages | 203 pass, zero ignored, seven unfinished Split cases filtered, 276.90 seconds. |
| Final isolated Audio library | 214 pass, zero ignored. |
| Isolated historical birth/processing PCM | Eight pass, zero ignored; 163 other cases filtered. |
| Required strict check | `cargo xtask check --no-test` passes formatting and strict all-target Core/Plan/Audio/Store lint. |

These are verification durations, not a performance acceptance comparison.
The seven unfinished Split tests are not rerun. The last full matrix remains
four passes / three work-limit failures from the wet storage milestone, not a
fresh result for this source. No coverage credit changes.

A test-only Clippy `manual_bits` failure is retained. Its correction expresses the
same eight-word byte expectation as `size_of::<[u64; 8]>()`; no assertion or
production code is relaxed. Both final source/binary sets are rebuilt afterward.
Independent review finds no supported runtime regression.

## Remaining work

This is a root final-policy adapter and production per-phase visitor, not a full
Components demand walker. Cold population across Original, signal, occurrence,
bound and routed policy transfers remains. Empty-carrier convenience reads are
not completion proof; EQ checks allocation shape, while a full walk must prove
complete population. Standalone empty raw visits need caller-owned cancellation
checks; partial observations must be discarded on any error.

Then reconstruct exact retained raw contexts, fold future outputs/tiles,
constructor peaks and recursive scratch, and establish eligibility before extra
histories allocate. The Plan intrinsic query is still unused by Audio and does
not classify arbitrary projected inputs. Batching remains disabled. No product
gate or requirement closes. Native ABI, GUI and packaging checks are not rerun.

Raw receipts remain in /tmp/deadpan-policy-visitor-2026-10-27. The
[inventory](../../tools/media-qualification/evidence/2026-10-27-audio-policy-visits/inventory.json)
retains exact live/published patches, changed-file before snapshots, frozen
worker candidates, source/binary identities, commands/output, independent review,
main/isolated lineage, the original lint failure/correction and next-wiring notes.
The evidence recorder's relative/absolute-key mismatch is corrected by prefixing
the final manifest's keys with the same fixed repository root; all hashes/sizes
then compare exactly, with no source, binary or result change.

All 101 gzip payloads verify stored and expanded hashes:
2,956,335 stored bytes and 11,912,630 expanded bytes.
Inventory SHA-256: `a1b8f5e4ff5c94cb0af6de2edfcaad81c45f9bce12a9ef0cf1a9330bc0006922`.

The archived raw-policy/next-wiring.md is read-only design. It identifies the
prepared sound queries to reuse, Original preparation/cache-miss factoring,
honest extra work on hits, exact routed-mask transfer and the need for an
independent planned-versus-observed permission oracle. Premarking and later OR
marks cannot be their own oracle. Nothing in that note is implemented evidence.
