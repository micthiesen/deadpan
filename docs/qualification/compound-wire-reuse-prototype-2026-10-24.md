# Compound wire-count reuse: private qualification

This is a private diagnostic experiment against qualified Main d6a6e23f.
It does not promote runtime code or close a requirement or latency target.

## Change and proof boundary

For one ordinary local, compact InsertTime transaction without isolation or
inputs, Core retains the actual successful initial document count. Its first
working document is a literal clone, so release can reuse that count. Core
still fully counts the actual post-leaf result. After the immutable visitor,
only its top-level revision header changes; checked subtraction/addition of
the actual serialized revision tokens supplies the final successful count.
Token errors, arithmetic uncertainty and an over-limit result fall back to
the original full serializer at the original admission site.

Every cumulative charge, prefix check, identity reservation/observation,
visitor and final work limit remains. Counts never come from a semantic
candidate, revision identity or document equality. This matters because equal
normal and provisional audio layouts can have different serialization
admission. Debug builds compare complete full-serializer Results with both
reused counts. Store's actual document/edit codec, readback, inverse and
authority checks remain. There is no public API, persistent cache or schema
change, and no new document or JSON buffer is retained.

## Exact sources and executables

The frozen candidate has 2,054 Rust files, source-map SHA-256
59ab40b15f9a705953320d45a9ffcef87a85f876a15455c4fb4180ce12ebdff9.
Exactly compound.rs and test code in one_leaf_replay.rs differ from the
matched instrumented baseline 0bf07513646ffdfee5aec03bb1b64b6fff757196512aeea883226c675b15ed02.
The candidate's complete private overlay differs from qualified Main's
2,053-file f973b941 map at 14 paths, including one added profiler module.
All six build inputs match. Profiling hooks remain outside Main.

Executed release Apps:

| Role | SHA-256 |
| --- | --- |
| Qualified uninstrumented control | 970df9f9c760435968082dc0ede48c6d1832008bb63b989d1ab7cf2dc23d4ac2 |
| Matched instrumented baseline | 1a12c2e9f866de1cef3ec64ea3997b26a1baafa194a95e3eef9b4587607d288a |
| Private candidate | ea9ff53ba7928aac7d2da11fabb7daad3c7b71ba39338f69227a9828878348e0 |

The run used this physical Mac17,7, Apple M5 Max, 128 GiB, macOS 26.5.2,
Rust/Cargo 1.97.1 and the pinned qualified FFmpeg/helper baseline. Helpers
were reused with verified hashes rather than claimed as fresh builds.

## Correctness and media

All nine build stages pass: 737 unfiltered Core unit tests, 35 integration
tests across four binaries, 12 release Store tests, release App, strict
Core/Store/App all-target Clippy and formatting. Tests independently force
full counting for both Some and None capability cases. They compare exact
results, edits, inverse, visitors and charges, and exercise actual 64 MiB
document and 512 MiB cumulative limits, reference modes, provisional values,
header lengths, identity/visitor failures and nested/unwinding scopes.

One actual release media test passes from a fresh Nextest archive, with
65 unrelated tests skipped. It verifies Some/None history, Undo/Redo/reopen,
register banks, authored and limited PCM, limiter gain bits and cold reads.
Both emitted files independently verify 45 pictures and 72,072 stereo samples
at zero offset. CLI SHA-256 is
6e876dcf1ae39582e5849b38054fa9ebb489e131187c41a1f9d40c2c05158f81;
test binary is 7805a2d62143e4c3bcee784c0f05c4bd3d92d8b77b18f46467d99d6de28b1c50.

## Timing observations

All six permutations of control/baseline/candidate produce 18 fresh native
trials and 36 commands. All 126 pause and 18 Kestrel checks pass. The 36
layout warnings remain in the raw reports. Exact source/input/binary,
authored-byte and empty-WAL checks pass, as do all 35 profile counts and
exclusive-time conservation checks. No memory observer runs in this batch.

Six within-rotation baseline-minus-candidate differences:

| Metric | Median reduction | Range |
| --- | --- | --- |
| Input to commit | 6.030062 ms | -3.646876 to 7.508584 ms |
| Input to offscreen composition | 6.203416 ms | -3.810250 to 7.277167 ms |

Baseline/candidate commit medians are 186.1159375/180.6210205 ms;
composition medians are 188.645875/183.169021 ms. The separate uninstrumented
control commit median is 186.0918125 ms. Replay document-wire phase medians
are 13.2755215/6.7645835 ms.

The slower third pair is retained. Its replay-wire phase is 6.360250 ms
lower while its whole worker is 3.434625 ms higher, with increases in other
measured regions. These observations establish no cause and justify no
exclusion. This batch proves neither a production gain, p95, physical-display
latency, memory saving/ceiling nor the 100 ms/50 ms targets. Uninstrumented
qualification remains required before promotion.

## Retained evidence

Scratch producers are terminal in
/tmp/deadpan-compound-wire-reuse-prototype-2026-10-24. Build session 6874,
media session 88440 and timing session 67860 all exited zero. Independent
source and raw-result reviews report no findings. Analysis SHA-256 is
5022f95b0dc73072995621dd8010b2f753b45ddf5e0abb6f942a53fab814fed0.

Before execution, review corrected an analysis PASS-regex draft, the
collector's treatment of newly created SQLite SHM metadata, six missing
input-file payloads and an unpinned packaging-plan attachment. Original
drafts and correction histories are retained. Both runtime SQLite sidecars
have exact after/live hashes and size checks; only authored files enter
native-fixture archives. No successful native trial was discarded.

The [primary inventory](../../tools/media-qualification/evidence/2026-10-24-compound-wire-reuse-prototype/inventory.json)
contains 359 payload files /15,332,794 bytes, excluding the inventory itself.
Inventory SHA-256 is
338756b4d89157f1e72270c6ca0bd3269240c7ef204a2db2453eb2fcce68e2e4.
The publisher and independent verifier pass on their first executions.
All 335 approved decoded payloads, 46 gzip streams, 18 safe native archives
with 54 authored members /80,807,004 bytes, 35 media files and both actual
movies verify. The complete primary tree remains unchanged across readback.

The [verification supplement](../../tools/media-qualification/evidence/2026-10-24-compound-wire-reuse-prototype-verification/files.json)
retains the independent verifier, receipt and raw copier/verifier outputs.
Its six payloads plus inventory have inventory SHA-256
1e9ee9651661bbee352c63191b970f0d92b715ce33ffc272147fd11e1afd9fb5;
receipt SHA-256 is
69fb68f3d1275ca9106484ca20309483d8fbcf0a834955345b2a605f9c0766fd.
Original EOF whitespace in raw evidence is preserved. Coverage remains 89%,
eight Complete and sixteen Partial.
