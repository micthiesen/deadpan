# First-pause binding diagnostics, 2026-10-23

The combined disposable prototype measures first-pause store-and-refresh median
111.576 → 91.629 ms. This is a five-pair diagnostic, before picture display,
with an incomplete standalone codec. It does not qualify the 100 ms visible-Hold
or p95 target. DP-24 and all gates remain unchanged.

## Method and identity

The control is the qualified compact-edge implementation
`6916ee0d578a11c37e6e66a470eb430b02773764`, executed perf SHA-256
`d8c0cd0fb54c22f8b697ed7e4c9e4f7944f6ea59b0e55f103c58d2ab61e2e7fd`.
Each experiment uses an isolated source copy and Cargo target directory. The
prepared base differs from shipping only in three uncompiled test files,
listed in each build receipt. Production source and six build/input identities
are checked before and after compilation; executed binaries are checked around
each trial. The host and qualified toolchain are the same as the
[compact-edge qualification](compact-frozen-edges-2026-10-23.md).

Each batch uses five fresh package pairs, 10,000 silent Background Holds,
seed 7, two pause insertions, alternating execution order. There were no
load-above-three flags. Compilation finished before measurements. The values
are short-run observations, without percentile inference or picture latency.

| Experiment | First control median | First prototype median | Second control median | Second prototype median |
| --- | ---: | ---: | ---: | ---: |
| Capture instrumentation overhead | 113.405 ms | 113.544 ms | 32.699 ms | 32.611 ms |
| Early full-layout fallback | 114.630 ms | 109.481 ms | 33.178 ms | 33.560 ms |
| Six binding defaults, v2 | 113.594 ms | 96.923 ms | 33.686 ms | 28.551 ms |
| Combined fallback and defaults | 111.576 ms | 91.629 ms | 32.645 ms | 29.280 ms |

The combined first ranges are 109.706–112.886 ms control and
90.955–92.435 ms prototype. Controls vary between batches, so compare paired
batches rather than adding their apparent gains.

## What the profile establishes

The all-but-root alias set first builds a scoped projection containing the whole
tree, discards it, then builds the full layout. The first-pause profile measures
the discarded scoped pass at median 4.718 ms, and the full fallback at 5.274 ms.
The complete per-node scan is 0.682 ms and ancestor closure 1.652 ms. Nested
admission/index timers occur three times, with a sum of 8.511 ms; these overlap
outer timers and must not be added to them.

The early guard follows precharge, derived-clock fallback, duration acquisition
and the duration-count check. With N−1 distinct retained aliases, ancestor
closure or a keyed spacer prevents a smaller projection. Returning None leaves
the existing complete-capture checks authoritative. Unknown aliases also fall
back to that existing path. The retained audit describes the proof boundary.

A separate read-only wire audit of one compact-edge first-pause history row
finds 10,001 bindings and 1,561,546 bytes of exact defaults: empty arguments
150,015; empty births 120,012; project-root clock 420,042; absent resume 20,930;
zero resume boundary 450,765; zero phase constant 399,782. Package hashes are
unchanged by inspection. This byte audit alone makes no runtime claim.

## Failures and production obligations

The initial binding v1 preparation omitted four fields because two Serde skip
attributes were missing. It was preserved and not measured. v2 corrects all six
attributes. Their zero/default values restore exactly, while nondefaults remain
explicit and bounded collection decoders stay in use.

The prototype still has an independent raw preflight requiring arguments and
births. A scratch round-trip test exposed `missing field arguments`: five
controls passed and the captured-state round trip failed. Its first direct
rustc link also failed because Apple LLVM could not read Rust LLVM bitcode;
Rust thin LTO linked the test. Receipts retain the original test-source hash,
but the exact original draft was overwritten by later refinements before
retention. The perf runs do pass their writable/read-only reopening and head
equality checks. Their receipts verify five revisions with zero replayed in the
retained example, so they do not exercise the defective bound-state keyframe
reader. Complete reopening/history correctness is not established, and no
production qualification is inferred from these diagnostics.

The working production candidate uses Core 58 / SQLite 87, accepts absent paths
as zero preflight entries while bounding present paths, keeps old-header fixture
serialization explicit, and adds nested malformed-input and capture fallback
tests. Standalone audio-context schemas 1–12 and sound-clock formats are
unaffected. The standalone binding JSON API has no independent schema header;
its current vocabulary changes under the owner's authorized development-format
break. Production review, full tests, real media, durable reopening and
current-format timing remain separate obligations.

## Retained evidence

[Evidence](../../tools/media-qualification/evidence/2026-10-23-first-pause-binding-prototypes/README.md)
contains 329 payload files / 5,003,929 bytes, with independently verified paths,
sizes and SHA-256 values. Inventory SHA-256:
`e8e298219c05f78515186da91dcb58b312952c81114432f86ec010a600ab3ec8`.
It includes procedures, exact source overlays and identities, build receipts,
all four batches, load reports, audits and the failed test/link probes. Generated
large packages, binaries and Cargo targets are excluded.
