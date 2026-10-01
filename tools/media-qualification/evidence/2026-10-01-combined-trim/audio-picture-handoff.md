# Combined Trim PCM and indexed-picture test stage

Frozen 2026-10-01 against HEAD
`8ddf5863f53ebb471fe3dced2d442647f05cd228` and the corrected, integrated
combined-Trim core sources identified in `dependency-manifest-v1.json`.

## Artifacts

- `combined-audio-picture-v1.patch`: the original frozen patch, six files and
  988 added lines. SHA-256:
  `b04555e2dfec39baccb10e3cca7eb8e3766ec48ade95786cf842b485a6edea55`.
- `frozen-v1.json`: patch, input-manifest and all six output-file hashes.
- `base-manifest.json`: HEAD and exact hashes of the three parent module files.
  SHA-256: `772384b3a592d0a0632ed9f5611088f1954e230542a4ad32f2cd9c6f550e8809`.
- `dependency-manifest-v1.json`: corrected core changes, qualified test helpers,
  WAV fixtures, lockfile, toolchain and external design/review hashes.
  SHA-256: `3fe36c66679955d2b3c4ee57396b2d4d9afd27375674fc9d055f0b697a90cccf`.
- `original-partial/` and `original-partial-manifest.json`: all six files as
  received, before the final audit and added filter-support negative control.
- `base/` and `work/`: the exact before/after files for frozen v1. Leave these
  unchanged when recording later corrections.
- `post-freeze-index-lifetime-v2.patch` and its `.json`: the root agent's later
  test-only lifetime correction, applied after v1. The JSON records both file
  hashes and the correction patch hash. This preserves the original evidence.

The correction binds `vfr_index(origin)` to a named local before borrowing its
selected frame. Frozen v1 alone produces E0716 in the second picture test.
The correction changes no expected coordinate, sample value or production code.

## Coverage and independent oracles

There are **five test functions and ten parameter cases**:

1. One 48 kHz mixed-intent case: `I=-3`, `O=2`, `S=1`, `R=-2`. A retains a
   symbolic resume and two historical reanchors; A/B prepend one/two physical
   frames. Handwritten phases are A prefix `8288/5`, A retained body `32313/5`,
   B prefix `13951/5`, and B retained body `29966/5`. Both new clocks, prior
   bindings, explicit Hard policy and exact inverse restoration are checked.
2. Two 44.1 kHz overwrite cases: partial opaque Preserve and partial Repeat,
   including its room-tone gap and second play. The 128-sample Source operand
   keeps filter support `[100,218)`. A deliberately cropped `[159,218)` operand
   differs. Preserve uses the complete canonical 384-sample output before
   retaining its tail. Fixed absolute labels, raw PCM, literal entry/opposite
   fade widths and inverse restoration are checked.
3. One 44.1 kHz root-sound case: a prior Insert route receives one final Trim
   map for equal In/Out. Offset 7 and an audible -6 dB root gain apply once.
   New sample 3204 maps through old 4805 and recipe 3203 to phase `117453/40`.
   The final sample 9609 retains phase 7350; a separate sequential Delete/Insert
   control reaches `1175853/160` instead. The old/new gaps remain silent.
4. Four indexed-picture cases: signed origins `-10010` and `13013`, each with
   direct Sources and existing neutral Partitions. Both physical prefixes retain
   literal irregular source PTS/ordinals and unchanged suffix pictures.
5. Two indexed-picture cases: those signed origins with an overwrite leaving
   only B's terminal padding. The retained selection is half-open; source PTS
   7007 is excluded, so the chosen frame is ordinal 4 at PTS 6200 plus origin.

Expected phases, supports, frame ordinals and fade widths are literal test
arithmetic. The command resolver supplies resource allocation and is also checked
against explicit assertions. It does not supply expected PCM or picture values.
The existing qualified decoder, reconstruction kernel and canonical stretch
helpers supply media/DSP primitives. Uneven chunk sizes 193/239 and 31/73 cover
raw and faded reads; the composite and root readers start with cold tail blocks
and seek backward. The tests assert audible witnesses for PCM and gain evidence.

## Verification

This scratch agent performed source review, scoped rustfmt on the six staged
Rust files and `git apply --check` of v1 against the checkout. Both commands
completed with exit 0. Base files and preserved original partial files matched
their manifests before freezing. No Cargo, native media, GPU, browser or other
runtime execution was performed by this agent. `tests_executed: 0` in v1 records
that original freeze state.

After integration, the root agent reported:

- The mixed 48 kHz test passed.
- Both 44.1 kHz composite/root tests passed.
- The first indexed-plan build failed with E0716. Root retained that failure
  evidence and applied the separately versioned named-index correction.
- Both indexed-picture tests then passed. All five functions passed in total.

Root's independent source review is at
`../audio-picture-root-review.md`; runtime invocation logs and source inventories
remain root-owned evidence. The scratch agent has not independently rerun those
tests. The broader workspace run was still underway at this handoff.

Focused test entry points, using the project's qualified native environment:

```sh
cargo test --locked -p deadpan-audio --lib combined_mixed_intent_preserves_old_phase_through_distinct_source_prefixes
cargo test --locked -p deadpan-audio --test composite_insert trim::combined
cargo test --locked -p deadpan-plan --test selected_video_window combined
```

## Limits and remaining work

These tests qualify the named headless command cases. Indexed pictures use a
synthetic measured index, without native image decoding or GPU measurement.
Shared DSP helpers are not independently requalified by these comparisons.
Native Trim interaction, store receipt admission and CLI behavior belong to
separate stages; this patch adds no production capability or interface control.
The root agent owns broader regression checks and final evidence updates.

The earlier core-review full-capacity success witness was invalid: a bound
100,000-node tree exceeds the separate audio validation work budget. Root's
corrected core witness uses unbound geometry/preflight and retains the later
capture refusal. This stage makes no full-capacity success claim.

Future fixes must be new numbered correction artifacts. Do not rewrite frozen
v1, its `work/` files, or the preserved incoming partial files.
