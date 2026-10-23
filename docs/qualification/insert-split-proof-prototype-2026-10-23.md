# Insert-split proof reuse prototype (2026-10-23)

This private prototype reuses the already-proved entry duration map around the existing interior `Split` bridge for a narrow `InsertTime` case. Six matched diagnostic pairs show a small, consistent improvement on the tested workload. The result is not production qualification, a latency-target pass, or a percentile measurement. Main documentation baseline is 79a14b8b; qualified runtime code remains 5fdae748.

## Change and scope

The prototype changes only `crates/deadpan-core/src/insert_time.rs`. After the entry query has successfully validated the document and durations, it keeps that map alive through the tail and lends it to the existing binding-only bridge around `split_captured`. The path is eligible only for local command work with compact timing, no derived clocks, no retained duration map, and a legacy interior split. Other cases continue through the existing path. The borrowed proof scope ends before replacing the working document. Capture, identity and binding checks, complete result validation, inverse behavior, and input immutability remain covered by differential tests. This skips one redundant successful complete validation of the immutable captured working input; it does not rerun every validation. Byte/work charges, replay and Store authority remain unchanged.

The private source map contains 2,054 Rust files (SHA-256 `ddf4b90c48877c24206c09cf4cf89f9f51c233b8b6fd158b95d5e08c15f31bf1`) and differs from the matched C profile map (SHA-256 `c4e2c6a8592dd5b819f81b49ad6f196d63b27cb8c68b8c20ee61ecb64bebdae5`) in only `insert_time.rs`; six pinned inputs are unchanged. The qualified Main code revision is `5fdae748ac2fdc67e1a11793c5ab69f6b61da833`; the prototype did not change Main. The private candidate app SHA-256 is `ffbaf7273f10e1a67041b3645d027be525bce4cc82d04985707ba781c63ee655`. The qualified uninstrumented control app is `0cb2931562108a68b57915c6135f448bc74293ba20623c7484053152566d31ff`; the matched C diagnostic app is `274e8a04a0a41b3656886663cea46f85169fa463d8b0e2c8d57b5d8d5638302a`. These are separate comparison lanes.

## Focused verification

The private ten-stage build used Rust 1.97.1 and passed formatting, focused Core compilation and execution, the 10,000-Hold reference oracle, three Core integration binaries totaling 22 tests, twelve release Store tests, the release App build, and strict checks. Six new focused Core tests, the existing wide oracle, and the integration/Store checks passed. This was not a full Core test-suite run.

V1 stopped at a test-only `E0659` ambiguous helper; no measurement ran. V2 rejected its selected test target because it matched zero tests; its new tests had passed separately, but that run was not counted as the oracle pass. V3 used the exact oracle module and completed the focused build. Both earlier attempts and the V2 review-pin discrepancy are retained as history.

## Six rotated timing pairs

Each of six rotations ran a qualified control, matched C diagnostic baseline, and private candidate in a different order, using fresh processes and fixtures. Each workload used `perf make-large ... --beats 10000` to create Background/Silence Holds, with no source media; the app edited a private portable copy. The first pause followed the initial Background settling. Each invocation waited for load at most 3 and no conflicting processes; OS file-cache, power and thermal state were uncontrolled. The separate control medians were 194.684813 ms to commit and 197.150167 ms to offscreen composition. All 18 runs and 36 make/run commands passed 126 pause workflow checks and 18 Kestrel checks. The reports retain 36 layout warnings; these were warnings, not failed checks. Authored input maps remained unchanged and the SQLite WAL was empty in every run. Timing excludes physical display and GPU readback.

| Measure | Matched C baseline median | Private candidate median | Paired baseline-minus-candidate median (range) |
| --- | ---: | ---: | ---: |
| Input to commit | 196.279438 ms | 191.777167 ms | 4.502272 ms (2.670417–8.509125 ms) |
| Input to offscreen composition | 198.770292 ms | 194.317417 ms | 4.452875 ms (2.503499–8.579791 ms) |
| Legacy insertion, including input-duration work | 33.031813 ms | 29.785688 ms | 3.363292 ms |
| Pure leaf region | 44.312729 ms | 40.843084 ms | 3.660208 ms |

Groups are summed within each trial before taking medians. The paired column need not equal the difference of the two lane medians. Both diagnostic lanes recorded 35 exclusive phases per run, with every phase count checked and the phase sum exactly equal to the enclosing duration. The profile and helper overhead were not calibrated. The six pairs are diagnostic samples; they do not establish a p95, a 100 ms or 50 ms target pass, a physical-display result, or a general performance gain.

## Retained evidence

The [primary evidence package](../../tools/media-qualification/evidence/2026-10-23-insert-split-proof-prototype/README.md) contains 286 payload files /6,881,896 bytes, with inventory SHA-256 `3059af729aeb692cb5d230cf661674d61d4232e088b235dd1255efd3a9fa237d`. The [independent verification supplement](../../tools/media-qualification/evidence/2026-10-23-insert-split-proof-prototype-verification/root-package-verification.json) reread all 287 packaged files, decoded 44 gzip streams, compared 26 decoded source copies with live hashes, and verified 18 safe fixture archives containing 54 regular members against their authored maps. All fixture WALs were empty. The fixture archives retain their SQLite databases; executable bytes, complete source trees and Cargo targets are excluded. Packager and independent byte verifier pass on their first execution after static review corrections.

The implementation remains a private prototype. A separate uninstrumented production qualification is required before treating this result as a production measurement. It makes no source-decode, independent DSP, listening, canonical PCM, memory, or physical-display claim. No requirement or gate closes; coverage remains 89%, eight Complete and sixteen Partial. The latency targets remain open.
