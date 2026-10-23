# Semantic preflight leaf profile, 2026-10-23

This is a private, diagnostic-only profile of the qualified implementation at 5fdae748, using a temporary V3 instrumentation overlay. It does not change production code or qualify a performance improvement.

## Method and checks

The profile adds nine exclusive measurements to the prior 26-phase route trace: semantic head validation, context admission, planner setup, pause target/provider work, child refresh, known-leaf insertion, input-duration calculation and net construction. Three rotated control/off/on triplets use a fresh 10,000-Hold fixture for each trial. All nine runs pass seven pause-workflow checks plus one Kestrel check, exact source/input/binary guards and empty-WAL checks. The three enabled reports each contain one valid 35-phase trace; the six control/off reports contain none. The 35 rows conserve exactly to the enclosing interval, including the zero-count, zero-time checkpoint row. The strict parser passes 21 synthetic tests.

The private release App is SHA-256 274e8a04a0a41b3656886663cea46f85169fa463d8b0e2c8d57b5d8d5638302a. Qualified production control remains 0cb2931562108a68b57915c6135f448bc74293ba20623c7484053152566d31ff. The fresh private source/target starts from the fine overlay, changing seven paths and retaining the other 2,047 Rust files plus six pinned inputs. Its 2,054-file source-map SHA-256 is c4e2c6a8592dd5b819f81b49ad6f196d63b27cb8c68b8c20ee61ecb64bebdae5. The separate qualified reference map retains all 2,053 original Rust files; main contains none of the hooks.

Eight exact std-only helper tests pass after standalone compilation with optimization level 2, debug assertions off, overflow checks on and unsafe forbidden. This is not a new full Core suite run. Twelve release Store preparation tests, the actual release App, strict release Core/Store/App all-target Clippy and formatting pass. All eight build stages terminate successfully; qualified native helpers were reused. Independent source, parser, driver and packager reviews have no outstanding findings.

The same Mac and native Background/Silence replay boundaries as the [fine study](current-pause-detail-profile-2026-10-23.md) apply. No source decoding or device audio runs in the timed replay. The 18 make/app commands pass 63 workflow and nine Kestrel checks; 18 layout/retry warnings remain. The helper retains bounded numeric state only. V3 emits 37 lines, including BEGIN and END; the nine new counts are one each, and the old counts remain unchanged. Its conservative 3,699-byte trace bound fits the 4,096-byte buffer.

## Measurements

Values are medians of three enabled trials, derived from same-trial sums of exclusive rows:

| Region | Median |
| --- | ---: |
| Semantic planning and application region | 92.230291 ms |
| Pure leaf region | 43.943916 ms |
| Legacy insertion region (leaf_insert_known + leaf_input_durations) | 32.736583 ms |
| Preflight/provider/refresh region | 34.420458 ms |
| Two named head-validation and input-duration calls | 2.757250 ms |
| Store preparation region | 61.709875 ms |
| Named document-size sites | 38.253499 ms |

leaf_insert_known alone is 31.453791 ms and excludes the separately measured 1.284166 ms leaf_input_durations call. It therefore does not represent all input validation. The 1.471250 ms head-validation measurement and 1.284166 ms input-duration measurement do not justify bypassing validation. These children are exclusive accounting within the profile, not individually removable costs. The diagnostic traces include instrumentation/helper overhead, which is not calibrated. Control/off/on commit medians are 195.141583 /195.833459 /195.879042 ms; offscreen composition medians are 197.625042 /198.464459 /198.506125 ms. These n=3 observations are not a percentile or a target pass.

| New child or remaining parent residual | Median ms |
| --- | ---: |
| semantic_head_validate | 1.471250 |
| semantic_context_admission | 7.468584 |
| semantic_planner_setup | 2.220958 |
| semantic_pause_target | 4.340625 |
| semantic_pause_provider | 9.884667 |
| semantic_children_refresh | 8.828542 |
| leaf_insert_known | 31.453791 |
| leaf_input_durations | 1.284166 |
| leaf_net_construction | 7.509334 |
| semantic_plan residual | 0.223750 |
| semantic_leaf_apply residual | 3.678916 |

The [full 35-row summary](../../tools/media-qualification/evidence/2026-10-23-semantic-preflight-leaf-profile/run/summary.json) retains minima, medians, maxima and lane observations. Worker totals are 189.389125, 188.328916 and 190.708834 ms, median 189.389125 ms. Control/off/on commit ranges overlap: 193.269542 to 196.920500, 194.352708 to 199.270208 and 194.513042 to 197.017875 ms. No overhead bound follows. Residual rows exclude their new children; separate medians must not be added or compared with earlier inclusive parents. The 4+4+3 size-site counts do not cover all serialization, and the two named validation calls do not cover all validation.

## Interpretation and next experiment

The profile separates several calls but does not prove a safe production optimization. Preserve complete validation, earliest refusal order, exact clocks, byte charges, bindings and transaction/history authority. The next bounded step is a private prototype that borrows retained entry durations only around the existing legacy interior split_captured call. No gain is claimed or inferred.

The profile makes no claim about the 100 ms or 50 ms targets, p95, memory ceiling, source decoding, physical display, PCM/DSP latency or ordinary user-visible improvement. No requirement or gate closes from this study. Coverage remains 89%, eight Complete and sixteen Partial; the full goal remains active.

## Evidence

The [primary evidence package](../../tools/media-qualification/evidence/2026-10-23-semantic-preflight-leaf-profile/) contains 186 payload files /4,356,015 bytes, 188 files including inventory and summary; inventory SHA-256 dbe4ad6e69f4f76ff6163dc36955c6c134a82998c0e33a20c7263c0171941932. Its separate [verification supplement](../../tools/media-qualification/evidence/2026-10-23-semantic-preflight-leaf-profile-verification/) records independent rereading of all payload hashes and sizes, 31 gzip streams, 22 decoded source-hash comparisons and nine safe fixture archives with 27 regular members. Only the two root SQLite sidecars, shared memory and the empty WAL, are excluded; authored bytes match before and after. The [phase-group derivation](../../tools/media-qualification/evidence/2026-10-23-semantic-preflight-leaf-profile-verification/derived-disjoint-totals.json) documents the exact same-trial sums. Earlier coarse and fine profiles remain separate batches and are not pooled.

Static packager review found that the newly supplied qualified-reference map needed explicit GO bindings. The final script binds its exact hash and provenance, checks them against the retained baseline, and retains both script drafts. Packager and independent byte verifier then pass on their first execution. This was an evidence gate corrected before execution, not a product failure. Raw private host metadata is excluded; public metadata is whitelisted and independently checked against its pinned derivative.
