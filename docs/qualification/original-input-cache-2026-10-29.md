# Bound request-local Original PCM reuse

The published candidate gains a request-local cache for exact Original input blocks before authored owner treatments. This ports only the cache and its integration from inherited Main work; independent sound families, General Split and other unpublished structures are excluded.

## Identity, admission and cleanup

A Graph owns 64 fixed identity slots, bucket links and a separate entry-capacity LRU. StageAudio owns evictable PCM payloads. Keys retain the actual context allocation, signed half-open sample range, depth and controller identity. Root and Domain use their existing faded read mode; Retained keeps its existing authored=false mode. Every hit still checks cancellation, deadline, nested depth and current source fingerprint/admission. Already-built query products remain real billed operations and are not retained here.

The Graph reserves its fixed cache shell before construction, including the scope Arc's separate two-counter allocation. Completed optional payload admission checked-adds cached PCM, active reservations, projected residency, live owner-query metadata and candidate storage. Optional storage pressure evicts only unpinned Original payloads, then returns the successful fresh block if no room remains. Required allocations may evict these optional payloads before required stage outputs. Both caches share the existing entry ceiling. Controller pressure uses first eligible insertion key; the Graph LRU governs its bounded identity capacity.

Cleanup removes every Graph payload before dropping its contexts and releases the possibly allocated empty controller map root. Pinned Rust 1.97.1 source establishes the conservative per-entry BTree node bounds. The ledger counts requested Rust layouts, not allocator size-class rounding, allocator-private bookkeeping or process RSS. Context handles share already-admitted Graph context storage.

## Reproduced metadata limit failure

A real WAV-backed read measured candidate residency P=336 frame units. An actual admitted owner query held a lease M=84. The original inherited code cached at cap419 and returned aggregate420. Its exact-fit420 control completed first. The unchanged test passes after adding the missing checked live-metadata term: exact-fit caches; one-short returns identical Fresh PCM, masks, dependencies and depth, re-admits the source, publishes no key/payload and releases the actual lease.

The pre-fix failure is evidence against inherited Main cache work, not against the preceding published Domain policy API, which had no cache integration.

## Work profile and remaining scope

The post-policy diagnostic reaches the unchanged 16,777,216-unit canonical cap, with cache allowance 0, full conservation and no trace overflow. Its one-selected-play prefix-two limited0+4096 error matches the current uninstrumented reference. It observes 5,851 actual prepared PCM hits after 2,012,414 query units have already been billed. Those products still feed policy prediction; this is a retention candidate, not a verified saving. All 3,591,443 observed transfer units are nonidentity. The profile's source/executable/raw-stream identities are retained separately from production evidence.

General sound-bearing Split remains disabled. Query retention, remaining variable containers/mix copies, full recursive admission and product acceptance remain open. No cap, previous work weight, refund, AI acceptance rule, timing, speed, peak-memory or requirement/gate completion claim changes.

## Verification

A second regression uses the existing CLI CountingAllocator around a real shared-bus request with WAV input, independent root sound and nonunity trim. Warmup precedes the measured controller's baseline. The old-root-only variant fails with 101,941 live requested bytes versus its 101,661-byte baseline, exactly 280 retained bytes. Restoring only the empty-root release makes the same test pass in both Main and the isolated candidate. It checks two requests, exact PCM, real source readmission, positive actual allocation peak and no retained prepared-stage cache. This observes requested Rust allocations on the calling thread, not native allocator memory or total RSS.

Main/isolated Audio libraries pass 378/372, with three ignored capture tools each. Fourteen cache/prepared tests cover exact reuse, fingerprints and revoked sources, cancellation, actual context allocation identity, controller/range/depth separation, pins, shared pressure, stale entries, collisions, LRU identity removal and index precharge/refusal. The actual held-owner-query test establishes exact finite residency and cleanup. The old frozen query/native matrix remains unchanged within the passing libraries.

Ordinary native Stage suites pass 203 in each variant; full definition/history suites pass 171 in each. All native suites run serially. Strict `cargo xtask check --no-test` passes formatting and all-target lint. The ordinary isolated copy has no Git metadata, so its wrapper fails during changed-package discovery; direct full-workspace formatting and Audio/CLI all-target Clippy with the same synthetic-worker feature pass there. The raw wrapper failure is retained. Plan/Core source is unchanged by this slice, and all eleven frozen oracles retain their hashes. No UI, encoded Render or release bundle change is made; those checks were not rerun for this backend cache checkpoint.

Three fresh reviews cover general correctness, resources and tests. The allocator regression closes the accepted cleanup-test gap. The resource reviewer withdrew an initial work-limit objection after checking that actual cache bookkeeping shares the finite canonical allowance; a Fresh fallback for unavailable storage does not promise free work. The controller's bounded first-unpinned eviction order remains explicit rather than being described as full LRU. The new allocator test received a separate read-only review with no material finding.

The fresh Split matrix remains three pass/four fail in 256.91 seconds, with the same failed revisions/windows as the prior checkpoint but different refusal phases:

| Window | Actual refusal |
| --- | --- |
| Source/Sequence cut-three, limited 0+511 | Plan AudioQueryLimit: structural work |
| Nested selected play, root_sound=false, prefix-two, limited 0+4096 | Document LimitExceeded: frozen projection work exhausted |
| One selected play, prefix-two, limited 0+4096 | Plan AudioQueryLimit: structural work |
| Processed root sound, root-sound-refinement, limited 0+511 | Document LimitExceeded: sound placement work exhausted |

These are resource failures, not successful Split witnesses. No error-phase difference is attributed to a particular collision or accounting row without a diagnostic reconciliation. General sound-bearing Split stays disabled.

The shipping index matches all eleven isolated candidate source files. Five also match Main exactly; the other six preserve Main's inherited family-only differences in its working tree. All 1,505 other initially captured Main inputs and 1,279 other isolated inputs are unchanged. The preexisting seven edited-file baselines match HEAD blobs. The source guard was expanded before the authoritative CLI red build/test from the earlier Core/Plan/Audio inventory to 2,750 Main and 2,422 isolated Rust/Cargo/native/config inputs. The preliminary CLI build is retained with its narrower coverage clearly identified.

The post-policy profile predates this fix and is archived as diagnostic evidence. Its 1,511 other captured Main baseline inputs remain unchanged; its two later differences are the assigned cache and cache-test files. It is not represented as a current production performance result.

Host: Apple M5 Max, 18 CPU cores, 128 GiB RAM, macOS 26.5.2 (25F84). Rust is pinned to 1.97.1. Guarded receipts retain the qualified FFmpeg/Opus/dav1d and ONNX Runtime prefixes, optimized build settings, exact commands, elapsed durations, source/executable identities and raw streams. No timing distribution or memory-performance claim is inferred from correctness runs.

Scoped patch SHA-256: `b4e7c42ddd5a80f3e6ced49744e2bda5ebdb32e3dbd399600e5fa82be2d1d416`.

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-29-original-input-cache/inventory.json) contains 214 compressed records: 7,047,426 stored bytes and 28,787,832 expanded bytes. Its SHA-256 is `d7346f8f1dcc8a5af850f133624276ac7362959e7a7a28adc719d1cd16b193a5`. Stored and expanded hashes were verified before publication. It retains all terminal task/profile receipts, both red regressions, exact candidate source, scoped patch, independent reviews, raw failures/passes and pinned-layout research. The prior policy inventory is linked by its exact digest; binaries, diagnostic filesystem copy and downloaded standard-library source copies are excluded.

Base revision: d0d850207079c389d6820c6288eb1cbf92df4fd7.
Scratch: /tmp/deadpan-original-input-cache-metadata-budget-2026-10-29.
