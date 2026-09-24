# Sampled-root transfer qualification, 2026-09-23

This increment builds on `284653fce629c5f307fd8c1cdfda78c643537fc3`, whose
[CI run](https://github.com/micthiesen/deadpan/actions/runs/35940513032) passed.
It adds bounded root-PCM transfer onto a typed preparation grid, with input-tap
audibility and exact output suppression. `StageAudio` supplies qualified source
PCM and shares one preparation budget, provenance set and deadline across the
complete halo. [The contract](../AUDIO_SIGNAL_TRANSFER.md) records the boundary.
Core 14/database 20 are unchanged.

## Verification

The final full repository gate passed with **1,021 tests**, zero failed and zero
ignored. All 335 source/fixture hashes remained unchanged through that run.
[Raw evidence](../../tools/audio-qualification/evidence/2026-09-23-signal-transfer/summary.json)
retains the commands, logs, source hashes, expected mutant failures and earlier
cleanup-test failure.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo test --workspace --locked` | 1,021 passed |
| `cargo build --workspace --locked` | Pass |
| `cargo run -p deadpan-cli -- doctor` | Pass; development foundation, core 14/database 20 |
| `cargo run -p deadpan-app --locked -- --smoke-test` | Pass; Apple M5 Max Metal and shutdown |

The saved eight ImageGen boards/prompts, five unchanged original spec files and
changed Markdown links also verified. The gate was repeated after the reviewed
fixture refinement so its hashes cover the final source and tests.

## PCM evidence

Six standalone integration tests compare transfer against a completely
materialized, explicitly masked signal rendered with the existing resampler.
They cover fractional and nonzero origins, signed out-of-crop anchors, exact
rates from 1/64 through 64, suffix-first reads, irregular block partitions,
maximum halo demand, input support exhaustion and exact point-grid silence.
Malformed callback starts/counts/policies, missing or invalid PCM, cancellation
and invalid recipes fail without returning a partial block. Numeric zeros do
not invent suppression metadata; invalid samples cannot hide behind a mask.

An actual canonical Preserve regression retains the NTSC distinction between
root sample 3203 and prepared point 3204. The test applies the retained root
resume from 1602 to 3203, includes current explicit silence, transfers the result
into a nonzero point grid and feeds it through a second canonical Preserve.
It compares exact output against the fully materialized masked carrier. Two
additional cases preserve real DSP decay through absent-source and
outside-placement regions.

Four StageAudio integration tests use the retained synthetic WAV fixture through
the native decoder and qualified source provider. They establish exact PCM
transfer, omission of creative fade gains, root silence across fractional
boundaries, full Preserve/RoomTone cache reuse, one shared work allowance and
rejection of a source-layout change between input blocks. Returned inspection
retains project/revision identity and the exact destination map.

The envelope test additionally checks explicit exhaustion intervals at both
retained endpoints, including extreme signed progress. Existing root and faded
read behavior remains covered by the complete workspace suite.

## Deliberately broken variants

Two variants were compiled and tested in an isolated scratch copy. The shared
checkout was never reverted or replaced:

- Removing input-tap suppression caused the PCM oracle test to fail with an
  assertion. Destination-only masking changed audible samples beside a silent
  interval.
- Resetting `ReadWork` for each input chunk caused both the preparation-limit
  and changing-source-provenance tests to fail with assertions. The integrated
  reader must retain those controls across the whole halo.

The raw logs retain those expected failures and their exact source substitutions.
They are test-sensitivity evidence, not failures of the final implementation.

## Worker-cleanup fixture correction

The first full gate stopped at the pre-existing
`successful_leader_exit_cleans_up_descendants_with_inherited_pipes` assertion:
its marker existed after cleanup. Audio tests had passed. The old fixture ran
`sleep 1; printf alive`, which permits the shell to write when group termination
kills its sleep before the shell itself. Production requires confirmed group
cleanup and control-pipe EOF before success. The marker alone could not establish
survival after return.

Twenty instrumented runs in a scratch copy did not reproduce that scheduling
race. A separate controlled shell witness proved the faulty marker behavior:
a killed sleep returned 137 and the unconditional continuation wrote the marker.
The guarded continuation did not. Successful delay before host return also
produced no guarded marker, while successful delay after return did.

The same pattern in three media/job fixtures now requires a successful delay
and a sentinel created only after the host returns. Existing deadline, event,
pipe and marker assertions remain. The nine media host tests and focused
successful-job cleanup test passed after this change. Independent follow-up
review identified a shared return sentinel across the cancellation and deadline
attempts. Each now owns an isolated fixture and writes its sentinel immediately
after its own call returns; a slow second attempt cannot hide the first's
survivor. Re-review found no remaining issue. Production process code is unchanged.
The original full-gate failure and all scratch observations are retained.

## Review and limits

Independent general and clock/filter reviews found no outstanding behavioral
issue. The clock review initially raised missing future frozen-binding
resolution, then withdrew that finding after checking the declared API scope:
StageAudio converts its own immutable root, while the generic bounded callback
accepts explicit retained policy. Authored live-to-frozen resolution remains
required work and has not been asserted complete.

Initial focused invocations without `DEADPAN_FFMPEG_PREFIX` stopped at the
expected build-time requirement; reruns used the qualified prefix. The first
format check identified module ordering, which was corrected before the final
gate. The scratch-copy witness script initially expected an optional rustfmt
configuration file; it was corrected before either mutant compiled.

The environment is macOS 26.5.2 (25F84), Apple M5 Max, Rust 1.97.1 and the pinned
FFmpeg 8.0.3 compatible prefix. Tests use generated stereo signals and the
synthetic PCM fixture. They do not establish recorded-speech continuity,
listening quality, authored Hold insertion, playback, export or performance
targets. No GUI behavior changed, so no new aesthetic, focus/IME or keyboard
review was performed. The saved single-Original ImageGen targets and earlier
native review remain current. No product requirement or gate is complete.
