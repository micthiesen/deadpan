# Retained conditioning qualification, 2026-09-21

The host now freezes the exact context manifest and both prepared input byte
streams before generation. Qualification requires these snapshots and rejects
worker provenance that substitutes input declarations or color assumptions.
Host provenance schema 2 records all three input objects alongside both canonical
masters. This establishes prepared-byte provenance, not source-frame or image
semantics, color accuracy, visual continuity, audition, or authored acceptance.

The implementation is [conditioning.rs](../../crates/deadpan-models/src/conditioning.rs).
The [bundle contract](../GENERATION_BUNDLES.md) describes authority and remaining
integration. [Recorded reports](../../tools/model-qualification/evidence/2026-09-21-retained-conditioning/)
include configurations, source hashes, timings, object identities, and gate logs.
No images, model weights or generated footage are committed.

## Actual local run

The existing pinned LTX-2.3 q4 development pipeline generated 25 native frames at
24 fps, sampled by the native host converter to 30 frames at 30000/1001 fps,
768 × 320, seed 1. Runtime/model pins and hardware are unchanged from
[native bundle qualification](model-bundle-2026-09-21.md): Apple M5 Max, 128 GiB,
macOS 26.5.2, private MLX runtime, and the compatible FFmpeg 8.0.3 converter.
This is one functional run with concurrent development and uncontrolled caches,
not a performance distribution or clean-machine packaging test.

The preparer captured inputs in `retained-conditioning/` before creating the
worker launch configuration. Capture and preparation took 0.366 seconds. The
supervised run took 121.733 seconds and exited cleanly without faults. Backend
time was 82.088 seconds; reported process peak RSS was 14,494,613,504 bytes and
the ending MLX counter was 17,767,443,772 bytes. These counters differ in meaning.

After worker teardown, its original input directory was moved out of the worker
workspace. Host qualification used only the pre-launch retained copies, verified
their identities again, and produced both masters and the linked provenance in
5.016 seconds. The initial pre-launch capture build preceded final control-check
and test refinements; the finalized implementation performed archive reload and
qualification and passed the full repository gate.

All six objects were then published through `ProjectStore`'s generated-object
API. The package was relocated and the entire original worker path moved away.
Separate store-only processes read every object and verified its expected BLAKE3
digest and byte length. This developer probe does not record Ready or accept a
Hold; the separate integration test exercises Ready without changing authored
state. The store's current Ready receipt checks only the three output objects,
so full conditioning dependency enforcement remains open.

| Retained object | Bytes | BLAKE3 |
| --- | ---: | --- |
| Context JSON | 1,350 | `fa905adba9f3b7b84cb1ade4b537ec3a2c7514c94bdcc3c4c16735f8d112fa92` |
| Left prepared PNG | 310,963 | `7074ad08d6a13fddcc615c7bf2fc4d7d3cd67320ae813e0f4af83e3c544ccf3e` |
| Right prepared PNG | 310,279 | `1546d7b105d6fcf262b93fabc5683642358930f897e0bd1c28617c1141482c3a` |
| Native master | 4,544,157 | `10eb7822ca74bd0e4ea3bdbceac3c359c0b7435df9992b849fc788dc7af96ee9` |
| Sampled master | 5,450,240 | `987ea014efe9f7740ab9c7cae128c8589d19c3bdeb4ecbb2ba37650bd7a3cadb` |
| Host provenance | 44,659 | `d6095abf910f02a373e0ae4e97c3942f2295dd67ae3305f572e974e08e227121` |

The native decoded RGB SHA-256 is
`be4ffce12b048457bcab1779eefe8683f41c242d013be7f488c25416491efe1a`;
sampled RGB is
`eda362044e57353435b56756be4143419a3bb6ed52bb2f849102b3eff962c71a`.
They match the earlier seed-1 run. Matroska container object hashes can differ
between conversions despite identical pixels and lengths.

## Automated checks and review

The exact repository gate passed: rustfmt, workspace Clippy with warnings denied,
326 Rust tests with zero failures or ignored tests, locked workspace build, and
CLI doctor. The audio and model Python suites passed 74 checks. No unrelated
native-adapter sanitizer or GUI/startup checks were repeated; C, native lifecycle
and interface code are unchanged.

Seven new conditioning unit tests cover immutable copies after source mutation,
exact hashes, typed receipts, request/plan mismatch, component-aware scope
separation, duplicate JSON keys, byte limits, cancellation, symlinks, hardlinks,
outside references, manifest aliases, and identical left/right inputs. Existing
real-codec integration tests now cover deleted worker inputs, substituted input
provenance, all six published objects and package relocation. The image inputs
in those tests are deliberately opaque byte fixtures, not PNG decoder evidence.

Independent general and boundary reviews are recorded with this change's
verification evidence. Source-clock binding, source and working color transforms,
image decode validation, installed-pack attestation, dependency reachability,
durable acceptance, GUI audition and natural keyboard navigation remain required.
No DP requirement or delivery gate is complete.
