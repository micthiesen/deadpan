# Native PreserveTape source policy admission

The routed projected Signal reader now constructs the actual after-Preserve Tape
policy through native admission. The query, content inventory and guards survive
content preflight, the control check, complete range validation, PCM zeroing and
native preparation/commit. The existing PreparedBlock Arc remains live. The exact
final Signal Vec transfers into native storage; no merged source replacement or
G destination copy runs at this seam. The later public route still owns its
separate mapped destination.

Canonical plus additional query work forwards once. Other SignalInput callers,
ordinary preparation preflights, graph/PCM/dependency construction and whole
preparation remain outside this unit. No requirement, coverage score or product
Gate changes; general sound-bearing Split remains disabled in shipping Scope.

## Actual ordinary baseline

[Baseline38](../../crates/deadpan-audio/tests/fixtures/native-tape-source-baseline-v1/corpus.json)
was frozen before kernel integration after two-copy capture, cross-regression
and independent review: 3,679,168 bytes, SHA
58c0993114d8775c6c306c1d1799a319f95af7d4eaa65db766a47de3bf91f179.

It retains seven cold public requests, nine warm boundaries and all three
unqualified Source setup rejections. A separate real managed WAV family uses
ProjectStore retention, the same AudioSession for qualification/registration and
first native preparation, and actual persisted receipt/asset data. The WAV's
unspecified stereo layout uses the explicit StereoLeftRight interpretation.
Natural Source input and output each complete three reads. FitBeat retains its
actual explicit-natural-rate-mapping setup rejection. Adding that family preserves
every earlier observation exactly. All 38 immutable payloads remain fixed.

## Actual native results

Four complete paired bodies, builds2/4 in Main/Scope, are byte-identical:
7,278,340 bytes, SHA
ea26580e6b4c027e726672dce54669353120e0314fe18e43446fcfe3e73a3bd8.
Of 55 result snapshots, 26 successful and 23 failed pairs are entirely equal,
including successful capacities/layouts. Six snapshots represent two actual
earlier resident-memory refusals, warm_exact_positive and warm_exact_empty,
plus their held copies. Their limits and errors remain literal.

Eleven actual Query events satisfy total = canonical + additional = view work;
39 Use events retain accepted work without another debit. Eleven PrepareMask
events record actual capacity/lease at entry, separately from commit. In-process
tests compare the exact source/final pointer and capacity. Source addresses never
enter serialized evidence. Inventory remains live through every observed use.

The three warm lanes retain these actual query values:

| Start/frames | Canonical | Additional | Total | Source ledger frames at Query | Final capacity |
| --- | ---: | ---: | ---: | ---: | ---: |
| 512/256 | 13 | 41 | 54 | 74 | 1 |
| 0/256 | 8 | 16 | 24 | 41 | 0 |
| 640/64 | 8 | 27 | 35 | 40 | 1 |

Positive output retains the typed native charge of 10 frames after controller,
ReadWork, provider, PreparedBlock, projection and plan destruction, then releases
to the three-frame account shell on output drop. Empty final masks create no
native account. Real Query/pre-handoff cancellation and foreign native issuer
refusal release all query guards. No accepted counters are filled or reset.

Both final runtime bodies match: 5,153,832 bytes, SHA
0c8717917452c91682725f2876cacaf94298c0dae9a487b65017bdd18b5e011e.
They add only the measured boundary family to the earlier equal runtime bodies.
Twelve fresh-request binary-search steps establish a full warm-request minimum
of 1,065 resident frames; 1,064 returns the actual resident PCM/stage-cache Limit.
This is a request minimum, not an allocation-callback peak. At one unchanged
1,068-frame quota, the first output charges 10 frames, a second read refuses while
10 remain held, dropping the first reduces that to three, and retry succeeds
with 10. Every search and fixed-quota result/work/account snapshot is retained.

## Verification and limits

Final4 covers all 21 Audio test targets per copy: 14 fresh executions and seven
physically byte-identical passing build2 executables. Main passes 1,147 tests;
Scope passes 1,141. Each has 31 ignored manual entrypoints. Only Main filters its
seven inherited unfinished Split tests. Libraries pass 566/560. Builds use opt2,
incremental0, debug assertions and overflow checks with qualified FFmpeg/ORT.
Main strict check, Scope all-target Audio Clippy, both formatting, rustdoc
(zero examples) and hygiene checks pass.

The [evidence inventory](../../tools/media-qualification/evidence/2026-11-01-native-tape-source-admission/inventory.json)
retains actual source/artifact/stream guards, raw/canonical captures, independent
acceptance, candidates and failures. Candidates are not execution claims.
Before1's missing Ordering import, the /private/tmp output-guard failure, the
qualified-family parent-traversal setup error, two pre-execution runner inventory
assertions, and strict3's explicit-counter-loop rejection remain recorded.
Final4 uses enumerate with unchanged accepted-preflight ordering and work.

Scope's 2,640 named source inputs own shipping. Main's 2,951 and Scope's 2,623
other initial inputs remain exact; Main WIP is excluded. Graph construction,
other preparation policies, PCM/dependency ownership and whole preparation remain
owed. Natural native work-ceiling exhaustion and routed post-Box cancellation
are not new claims; existing native custody tests remain separate evidence.
