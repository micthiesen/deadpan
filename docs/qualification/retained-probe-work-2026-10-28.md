# Charge retained lookup work even when no input is found, 2026-10-28 (UTC)

AudioOwnerInput::retained_probe_at now returns a typed optional input and actual
work from the same validated lookup. The retained_at compatibility method moves
that input out without a second traversal. None is no longer confused with zero
work by controlled preparation.

Context::owner_context charges the receipt before either branch.
PreparedOwnerEqualizer includes it in construction work. Both remove the former
Some-only charge, so an existing retained input is charged once. Input selection,
reference grids, guards, DSP, caps, weights, cache policy and error propagation
remain unchanged.

The exact root-tone metadata fixture now bills all 28 successful work units.
With 27 available, the same traversal refuses with structural work after 19
completed billed units. Failed-query partial work reporting is unchanged; no
budget is reset and no failed operation is retried.

## Verification

Rust 1.97.1, locked dependencies, qualified FFmpeg/ORT, opt-level 2, debug
assertions and overflow checks. Guards cover 1,428 main and 1,199 isolated
inputs. All seven staged source files equal the isolated final build. Six
also match main; plan.rs retains an inherited overlay outside the task hunk.

| Check | Result |
| --- | --- |
| Main / isolated Audio library | 284 / 265 pass, none ignored. |
| Main / isolated complete Plan | 553 / 540 pass across 29 executables each; none failed, ignored or filtered. |
| Main enabled / full isolated real-WAV stages | 203 / 203 pass, none ignored; main filters seven unfinished Split cases. |
| Main complete definition / isolated historical PCM | 171 / eight pass; the focused historical run filters 163 other cases. |
| Previous consumer charge bodies with the new Plan API | Isolated Audio: 263 pass / two expected failures at actual accounting assertions. |
| Required strict gate | cargo xtask check --no-test passes formatting and strict all-target Core/Plan/Audio/Store lint. |
| Independent review | No supported findings, including test corrections. |
| Seven unfinished Split cases | Four pass / three planning-work failures, none ignored; 383.90 seconds. |

The old-consumer variant fails before PCM comparison: PreparedOwnerEqualizer
records 13 instead of 22 units; node metadata records 19 instead of 28. It keeps
the new work-bearing Plan API and final legal Audio fixture. This is direct
underbilling evidence, not an old-PCM failure claim. Its unused Plan unit test
still has the initial guard expectation and is not run as Plan qualification.

Tests cover positive None work, exact/one-short limits and guard order, Some
PointCeil work/reference mapping, the controlled Root retained-input charge
once, prepared constructor work and actual tone PCM against a separate authored
EQ oracle. The provider refuses media reads; both actual tone paths use native
filters and restore the renderer's active residency.

Two initial new tests are corrected without production changes. The PCM fixture
passed a temporally authored EQ suffix to a raw-input API which correctly
refuses it. The final fixture has raw Tone input, an explicit nonunity recipe
and a separate authored-EQ oracle. The Plan zero-work expectation is corrected
to the existing InvalidAudioLimits result before range validation. Final
binaries rerun the relevant full libraries and all media qualification above.

Initial artifact pinning collided on the Plan/Audio audio_definition target
name. The no-overwrite assertion stopped; a queued Audio launch then failed
before invocation because its binary had not been copied. Partial files and
the failed script remain. Corrected pinning validates 32 package-qualified
destinations before copying exactly three Audio and 29 Plan executables from
successful Cargo JSON. No glob or stale binary is selected.

The Split failures are Source/Sequence refinement, cut-three, limited0+511:
plan work per read; nested selected without root sound, prefix-two, limited
0+4096: frozen projection work exhausted; and one selected play, prefix-two,
limited0+4096: source occurrence construction work. The first reported subphase
changes after honest charging. Processed-root, partition-independence,
each-depth and audible-witness cases pass. No new memory failure or cap change.

All 19 build/check/test receipts are terminal with source and binary guards
unchanged. Final main/isolated builds take 35.14/33.70 seconds; main stage and
definition runs 72.12/76.74 seconds; isolated stages 67.14 seconds. Definition
started 5.104342 seconds before the last Split witness finished after a yielded
wait was mistaken for completion. Exact overlap/output is retained. These
durations are verification, not performance acceptance. No GUI, ABI, packaging
or release checks are repeated.

## Remaining work and evidence

The accounted successful None result does not add failed-query work receipts,
metadata heap admission, cold resource closure or batching. Actual context
construction still needs prior allocation accounting. A fused, sealed owner
candidate query could remove a duplicate real membership query, but it must
preserve the actual parent seed, capture scope and all work. Read-only design
is not implementation evidence. Coverage stays about 85%, ten Complete
requirements and all seven gates open.

The [inventory](../../tools/media-qualification/evidence/2026-10-28-retained-probe-work/inventory.json)
retains 311 gzip payloads: exact before/final source, task/negative/test patches,
32-target binary pins, all commands/output and Plan per-target logs, retained
failures, source/index equality and independent reviews. Stored/expanded hashes
verify: 3,285,888 / 13,572,537 bytes. Inventory SHA-256:
b311089d45d4d637d33252d5a6d57cfae88a7096131370d21100add65228eac2.
Raw task work is in /tmp/deadpan-retained-probe-work-2026-10-28.
