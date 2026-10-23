# Reusing the validated leaf result during compound pause preparation

**Result:** the one-site Core change passed the completed qualification checks. In five alternating control/candidate pairs (ten large-project trials), candidate median input-to-commit was 265.164417 ms versus 272.579417 ms for control. Median input-to-offscreen-background-composition was 267.862584 ms versus 275.099042 ms. These are diagnostic samples, not a p95 or a performance-target pass.

## Change and exact build

`deadpan-core`'s compound pause leaf path now uses the validated document returned by `apply_with_result` rather than applying the generated forward patch a second time. The charge, current mutation, step accounting and child refresh order are unchanged. The differential test compares against the legacy leaf patches and checks exact marks/bindings, compound replay, inverse restoration and unchanged input. Core schema remains 58; SQLite schema remains 87.

The qualification used source base `1e67fdb27bbb3a59d282e5240c65b860b4090b76` plus the frozen overlays recorded in the evidence package. Its all-Core Rust identity manifest SHA-256 is `9568cd84ccae53fb71248ac2e653816b39c68bea49620b4dffc67aa17db7b45f`. The retained production source `crates/deadpan-core/src/semantic/planner/pause.rs` hashes to `8b6e2f282b5d704afe6528e5dbdad5118028bb99d637fe0644410bb514fde15f`; the tested pause source hashes to `febd9d852ee043b496960a9442cdc8ed34cae1e497442ce7a46e400867dcc276`. The executed candidate release app hashes to `1864c3acd798d7ce68a8d3a62ee144ec6956cdc4750c568bf0001d7c2a67be4c`. Control app: `69978abd09ffc52dcd281066bc615d7b3e1d5cb11653f5d888d0738015ba1496`.

## Qualification

All eight main qualification stages and all 26 post steps exited zero. The candidate source, inputs and executables passed their recorded guards.

| Gate | Result |
|---|---|
| Focused differential test | 1 selected test passed; final focused preparation/test/guard receipt passed |
| Core archive | 1,514 passed, 0 skipped, 69 matched test binaries |
| Broad archive | 3,505 passed, 740 skipped, 110 matched test binaries; one nextest leak status |
| UI archive | 1,296 passed, 11 skipped, 2 matched test binaries; one nextest leak status |
| Strict check and formatting | `cargo xtask check --since 1e67fdb27bbb3a59d282e5240c65b860b4090b76 --no-test` and `cargo fmt --all -- --check` passed |
| UI Clippy and formatting | `cargo clippy --locked -p deadpan-app --features ui-harness --all-targets -- -D warnings` and formatting check passed |
| Media | Debug preflight and release neutral/aligned sound-scope cases passed |
| Native | Release `sound-group-endpoints`, macros, named-registers, dot-repeat, range-set-selectors and transcript-search-selectors reports had no failed checks |
| Portable copies | Neutral and aligned copies each contained 9 regular files; `project validate` and `doctor` passed with package paths/bytes unchanged |

The broad and UI nextest leak statuses were independently rerun: `prepared_relink::serialized_record_does_not_grant_authority_over_current_inventory` and `transport::tests::ntsc_picture_changes_at_allocated_sample_boundary_once` each passed alone with source and binary guards intact. The first isolated-rerun wrapper failed before starting a test because it looked for a nonexistent `candidate-broad-lock.json`; the corrected wrapper used the phase input and source maps. This was a harness setup error, not a product test failure.

The first focused differential attempt failed at raw primary mark lookup with `AnchorError::OutsideMapping`. A logical mark can retain multiple physical bindings, so its primary coordinate can lie outside a clipped interval. The test fixture was corrected to resolve through a revision-aware `SelectionRequest`, `BoundarySelector::Mark` and `NamedMarkTarget`. The corrected test still asserts frame 12, exact mark and binding equality, legacy leaf equivalence, replay, inverse restoration and input immutability. The production helper did not change for this correction. Three read-only reviews recorded no findings; the evidence keeps the first test failure, correction, and reviews separate.

The six native workflow scenarios ran 1,340 workflow checks plus 6 Kestrel routing checks with no failed checks. Their reports retained layout warnings: second-retry frame counts were 51 for sound-group-endpoints, 115 for macros, 30 for named-registers, 61 for dot-repeat, 22 for range-set-selectors, and 31 for transcript-search-selectors. Distinct-cause retry-run warning counts were respectively 7, 32, 2, 8, 2 and 5. Each scenario also reported an egui discard warning. These are retained warnings, not failed checks.

The debug neutral and release neutral media cases verified 15 pictures and 24,024 canonical samples. The release aligned case verified 19 pictures and 30,430 final canonical samples. These reports explicitly set independent DSP and listening claims false; the aligned report also sets the move-PCM-translation claim false. This qualification makes no ordinary Move PCM or listening claim.

## Large-project timing

Each trial used a fresh private copy of a `perf make-large --beats 10000` project with Background/Silence Holds and no source media. The seven `large-first-pause` checks passed in each of five alternating control/candidate pairs. Authored input bytes remained unchanged apart from SQLite coordination sidecars; WAL length was zero. Source and binary hashes remained guarded. Pre-run load stayed within the recorded limit.

| Boundary | Control median | Candidate median | Difference |
|---|---:|---:|---:|
| Input to commit | 272.579417 ms | 265.164417 ms | 7.415000 ms (2.72%) |
| Input to offscreen Background composition | 275.099042 ms | 267.862584 ms | 7.236458 ms (2.63%) |

These five samples per build are diagnostic; they do not establish p95, a target pass, or calibrated overhead. The 100 ms fallback and 50 ms ordinary-edit performance targets remain open. The offscreen composition boundary is not a physical display measurement. Do not pool these runs with the earlier private prototype timing or the direct-performance scenario, which has different boundaries. No source-decoding or PCM-performance claim follows from this comparison.

## Evidence

The bounded package is [the retained evidence directory](../../tools/media-qualification/evidence/2026-10-23-reused-leaf-result/). It contains 548 files totaling 13,363,276 bytes. Its inventory SHA-256 is `d1805e2c757039581298c368938afdc8ef2a1d05da3ae76f3bdbc88444dbd9f4`; every payload was reread and verified against the inventory. It retains the exact relevant Rust source copies, source/input/binary manifests, phase receipts and list summaries, compressed logs and reports, focused failure/correction/review records, media and portable summaries, six native reports, and all ten guarded timing inputs and reports. It excludes executables, nextest archives, targets, generated project packages and source video. The earlier Core checkpoint and single-site prototype remain separately identifiable historical evidence.
