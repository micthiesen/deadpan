# Compound wire-count reuse: production qualification

Main now reuses exact local counts around one eligible ordinary InsertTime
replay. The successful actual entry count also applies to its untouched
working clone. Core still fully counts the actual post-leaf value; after the
immutable visitor, checked replacement of its serialized revision token
supplies the final successful count. Uncertain arithmetic, token errors and
over-limit results fall back to the original full serializer and its error
mapping. All cumulative charges, identity/prefix boundaries and Store
document/edit/readback/inverse checks remain.

Counts never come from document equality, revision identity or the optional
leaf capability. Both Some and None paths can use this local proof. Debug
builds compare the complete full-serializer Results. Other shapes and
reference modes retain full counting. The change adds scalar counts and a
bounded header clone, with no new retained document, JSON buffer, persistent
cache, public API or schema change. Core 58 /SQLite 87 are unchanged.

## Sources and builds

The uninstrumented candidate has exactly 2,053 Rust files, source-map SHA-256
2b0997f1ad22ce7dd869beba699909d22972efba434279cee350478dc63cb728.
Only compound.rs and tests in one_leaf_replay.rs differ from qualified
control f973b941a3f71b70cd0826cf6ab3f5ceb68395e5407690bea79f8e5a49e53a53.
All 2,051 other Rust files and six build inputs match. After promotion,
Main's complete Rust path set, every source byte and all six inputs match
the qualified candidate. Main formatting also passes.

Executed release App SHA-256:
0c9c2e641dbe0ef242f1d5503edb4d795a32fef23f286335cdf575e52f814c66.
Control App SHA-256:
970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2.
The comparison uses this physical Mac17,7, Apple M5 Max, 128 GiB, macOS
26.5.2, actual Rust/Cargo 1.97.1 and the pinned qualified FFmpeg/helpers.
The three helper binaries are reused with verified hashes, not rebuilt.

All nine build stages pass: 729 unfiltered Core unit tests, 35 integration
tests across four binaries, 12 release Store tests, release App, strict
Core/Store/App all-target Clippy and formatting. Unit tests independently
force full counting for Some and None paths and compare complete results,
edits, inverse, visitors and charge amounts/order. They cover normal versus
equal provisional representation, reference modes, nested/unwinding scopes,
identity/visitor failures, short/equal/128-byte outer headers and the actual
64 MiB document and 512 MiB cumulative limits.

## Real-media verification

One actual release Nextest case passes from a fresh archive; 65 unrelated
cases are skipped. The test compares leaf-capability Some/None history,
registers, durable Undo/Redo/reopen, authored and limited PCM, limiter gain
bits and partitioned/cold reads. Both legs permit local count reuse; the
independent forced-full counter oracle is in the unit tests.

Both actual exported movies independently verify 45 pictures and 72,072
stereo samples at zero offset. CLI SHA-256 is
8eb7716d210b11b1f3a7434a3824c873ccc1d284fe9beaf31d87a95191cbe5ac;
test binary is 7cc889f5be0b0896fe7a370ff422fce973d623b3bebbf1d643c01d01f30c0452.
Archive SHA-256 is
55d358989010cae45c118634fe8bc4fe34f473c53cfc237a8bda7f33d08822aa.

## Uninstrumented timing

Five alternating control/candidate pairs use ten fresh 10,000-Hold fixtures
and twenty commands. All 70 pause and 10 Kestrel checks pass. Twenty known
layout warnings remain in the raw reports. Every fixture starts with the
same bytes, authored bytes remain unchanged and WALs are empty. Exact
source/input/binary pins, quiet load/process checks and profile-free logs
pass. No memory observer or private profiling hook runs.

| Metric | Paired median reduction | All-pair range |
| --- | --- | --- |
| Input to commit | 5.737334 ms | 3.295332 to 6.570333 ms |
| Input to offscreen composition | 5.692792 ms | 3.111458 to 6.439916 ms |

All five pairs improve. Control/candidate commit medians are
185.961791/180.792875 ms; composition medians are 188.426250/183.389625 ms.
The paired differences are computed within each rotation, independently of
the difference between lane medians. All raw values remain retained.

These five pairs establish no p95, physical-display latency, memory saving
or ceiling, or 100 ms/50 ms target pass. The earlier
[private prototype](compound-wire-reuse-prototype-2026-10-24.md) remains a
separate diagnostic batch, including its slower third pair. Its values are
not pooled with this production batch.

## Evidence and preparation history

All producers in /tmp/deadpan-compound-wire-reuse-production-2026-10-24
are terminal: build 76289, media 85726 and timing 26178 exited zero. Main
formatting session 42762 also exited zero. Independent raw-result review
reports no findings. Analysis SHA-256 is
8ea0744c155cd8226ddf4f8b4dc95dcd2b0e65c1f8d2285546bbcccf798af50d;
independent review is
7b15f06575b84a2eaf584f00468132396165f553c4dc3d5d8d9166fb32d277ab.

The initial copy filter mistook two legitimate source directories named
target for build outputs. A complete-path guard refused that copy before
either source change was applied. The corrected copy protects all qualified
source paths; the initial script, partial map and failure receipt remain.
Before execution, review also corrected analysis-schema assumptions and
collector retention, identity, path and byte-bound checks. Those draft
scripts and correction histories remain. No native trial was discarded.

The [primary inventory](../../tools/media-qualification/evidence/2026-10-24-compound-wire-reuse/inventory.json)
has 253 payload files /10,713,173 bytes, excluding the inventory itself.
Inventory SHA-256 is
eb326525ca16652869ae889695c155bb5848e611bc2ca445ed5bbedfd100684c.
Both the first copier and independent post-copy verifier pass. Every one of
237 approved decoded payloads, 31 gzip streams, ten safe native archives
with thirty authored files /44,892,780 bytes, 35 media files and both emitted
movies verifies. Gzip decoding follows the explicit encoding ledger, which
also permits logical JSON/HTML filenames. Both runtime SQLite sidecars have
exact live/after pins and remain outside authored-fixture archives.

The [verification supplement](../../tools/media-qualification/evidence/2026-10-24-compound-wire-reuse-verification/files.json)
retains the verifier, receipt and raw copy/verification outputs. Its six
payloads plus inventory have inventory SHA-256
036e8a53e5cc49b60cd4d1813bb2abf05a3cc84e705d08204bdcd6f9ba8c428d;
receipt SHA-256 is
afdd59e05b2c044682da9dba786fbcad2d3bebf267e64df4f90bf16187893b1d.
The primary tree remains unchanged across verification. Original raw EOF
whitespace is retained; authored diff checks pass.

Coverage remains 89%, eight Complete and sixteen Partial. No requirement or
gate closes; the full development goal remains active.
