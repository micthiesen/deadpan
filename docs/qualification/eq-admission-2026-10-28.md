# Admit the complete native EQ state, 2026-10-28 (UTC)

Both authored and shared EQ node builders now admit the complete published
native construction bound before creating a filter. They retain that same
bound for its graph lifetime, including when recursive tile preparation
temporarily takes the processor out of the node.

## Behavior

The published bound is 2,129,928 bytes, or 266,241 existing 8-byte residency
units. No smaller lifetime estimate is introduced. Checked future active and
graph totals are calculated before construction. Context, boundary, work,
budget, native-construction and final-control checks finish before publication;
failure drops any local filter and leaves no new node residency committed.

The synchronous native constructor cannot re-enter StageAudio. Existing graph
cleanup releases the full charge on success and error. Existing nodes reuse
their filter without a second native charge. Event-local EQ already admits
this bound and is unchanged; wet nodes receive no EQ charge.

No DSP arithmetic, PCM policy, cap, native bound, work weight, failed-batch
retry or cache policy changes. The fixed 4,097-tap work charge and the performed
6,144-frame tile work charge remain spent when subsequent admission fails.

## Verification

Four real graph tests use non-unity EQ over a 512-sample Tone Hold and a provider
which refuses source reads. Each real builder is tested one residency unit
short and at the exact bound, then for node reuse, residency while its filter
is taken, tile workspace refusal/restoration, repeated low-budget read failure
and successful exact PCM/metadata comparison with the normal-cap renderer.
All four fail at the admission assertions with the previous published builders.

Rust 1.97.1, locked dependencies, qualified FFmpeg/ORT, opt-level 2,
debug assertions and overflow checks on the same macOS host. Guards cover
1,426 main and 1,197 isolated inputs. The six staged source files equal the
isolated build. Main preserves its inherited shared_eq.rs overlay and three
matching Graph fixture fields; the other four files match exactly.

| Check | Result |
| --- | --- |
| Final main / isolated Audio library | 277 / 258 pass, zero ignored. |
| Main enabled real-WAV stages | 203 pass; seven unfinished Split cases filtered. |
| Full isolated real-WAV stages | 203 pass, none ignored or filtered. |
| Main complete definition suite | 171 pass, zero ignored. |
| Isolated historical birth/processing PCM | Eight pass; 163 other cases filtered. |
| Previous published builders with the new checks | All four fail at the intended admission assertions. |
| Required strict gate | cargo xtask check --no-test passes formatting and strict all-target Core/Plan/Audio/Store lint. |
| Seven unfinished Split cases | Four pass / three planning-work failures; zero ignored, 411.43 seconds. |

The Split failures are Source/Sequence refinement, cut-three, limited 0+511:
sound placement work exhausted; nested selected play without root sound,
prefix-two, limited 0+4096: frozen projection work exhausted; and one selected
play, prefix-two, limited 0+4096: source occurrence construction work.
These reported subphases differ from the previous checkpoint; preserve the
actual failures. The processed-root, partition-independence, each-depth and
audible-witness cases pass. No Split case fails native memory admission.

Final main/isolated builds take 34.41/33.73 seconds. Main stage/definition runs
take 258.80/75.87 seconds with one/four test threads; isolated stages take
66.81 seconds with four threads. These are verification durations, not
performance acceptance results. Main media runs use the first pinned patched
binary. Its complete production and existing integration-test inputs equal the
final build; only four test-only helper/adapter files change for lint. Final
libraries use rebuilt binaries and all four checks still run.

All 18 build/check/test receipts are terminal with source and binary guards
unchanged. No GUI, native ABI, packaging or release acceptance check is repeated.

Strict lint initially reported duplicate inclusion of the common test module.
It is now compiled once, with generic common assertions and thin adapters
calling both actual graph paths. Independent review found no supported defect
in admission/lifetime or the test refactor. No lint suppression was added.

Initial isolated harness construction failures are retained separately: a
script accidentally copied main-only module imports, then the new fixture
included three inherited Graph fields absent in published source. Those are
setup errors, not runtime results. The published adapter removes exactly those
three fields; assertion logic and numerical source match.

## Remaining work

This fixes existing native-state accounting. It does not connect the completed
cold demand visitor to exact context ancestry, recursive metadata/native
admission or batching. General sound-bearing Split remains disabled and the
full product goal remains active. Requirement/gate completion is unchanged.

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-28-eq-admission/inventory.json)
retains 146 gzip payloads: exact snapshots, patches, source/binary pins,
commands/output, all failures, test-only lineage and independent reviews.
Stored/expanded hashes verify: 3,071,128 / 12,618,432 bytes. Inventory SHA-256:
dc4c62e8dc990aa87275c72de89513a3e3fe5038becda11b541861e131e6eb54.
Raw task work is in /tmp/deadpan-eq-admission-2026-10-28.
