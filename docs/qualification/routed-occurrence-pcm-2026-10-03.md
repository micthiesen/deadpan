# Retained occurrence PCM, 2026-10-03

This increment adds borrowed sample routing for one complete independent sound
occurrence. It advances DP-04 and DP-09 preparation; no product requirement or
release gate is complete.

## Behavior and limits

`AudioRoutedRoot::occurrence` retains the checked occurrence's plan, complete
sample allocation and nested Preserve projections. The recipe extent is relative
to the occurrence's start; its RoundEven grid origin is the negative of that
start. Admission compares the complete extent, grid and allocation.

The canonical reader transports old integral sample labels through chronological
route operations. It does not resample from new frame endpoints. A shorter
intermediate allocation can discard a sample permanently even when a later move
returns the sound to its original position. Current consuming Hold gates,
creative edges and gain remain separate from this raw retained input.

All requested spans share one preparation budget. A wholly silent route still
preflights its complete nested history before opening media and admits its source
on every read, including warm caches. The added tests cover depth, stage count,
input size, residency limits and source revocation/recovery.

This is a borrowed preparation boundary. No schema or temporal-command guard
changes. Independent persisted clocks, stable historical occurrence resolution,
current gate/gain projection, partial-copy timing and native placement remain
required.

## Review and fixture corrections

Independent read-only review found no runtime defects. Its requested half-sample
tie coverage was added: at 96,000/3,203 fps, the occurrence boundaries at frames
1 and 7 land on 1,601.5 and 11,210.5 samples. The routes retain the RoundEven
labels 1,602 and 11,210, exercising both tie directions.

Initial test compilation required cloning borrowed plan Arcs. The first executed
tests correctly rejected two reads larger than the reader's 256-frame limit;
the fixtures now compare complete passages through bounded reads. The NTSC
terminal-sample witness initially fell outside its natural-rate source allocation.
Extending it beyond the owner and then trying FitBeat each hit existing validation.
The final fixture retains a longer natural-rate recipe with an exact four-frame
SelectedPlacement. Its last sample is nonzero before the route clips it, and
stays absent after the return move. These corrections changed tests only.

The first broad plan/audio run encountered that same invalid fixture. All other
tests passed. Complete failed-attempt logs and source inventories are retained
with the final results below.

## Verification

- `cargo test --locked -p deadpan-audio --test audio_definition
  routed_occurrence`: four passed, zero failed or ignored.
- `cargo test --locked -p deadpan-plan -p deadpan-audio --lib --tests
  --no-fail-fast`: 756 passed, zero failed or ignored.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.

The focused tests are included in the broader total. Every final check used
the same source inventory, unchanged throughout execution. SHA-256:
`dac6e3c6478d86500322c6e8780e48b70b26bcf285cae3a73f0ae7f6950714d2`.

[Evidence metadata](../../tools/media-qualification/evidence/2026-10-03-routed-occurrence-pcm/metadata.json)
records each command, source inventory, exit status, environment and review.
Compressed complete logs retain the failed attempts and passing reruns.
[SHA256SUMS](../../tools/media-qualification/evidence/2026-10-03-routed-occurrence-pcm/SHA256SUMS)
covers all retained evidence files.

## Environment and untested scope

Base: `025c2a9433b345a3fc2b9d345ed3dfa1e1b35db6`.
Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust/Cargo 1.97.1,
locked dependencies, FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

Real decoded WAV PCM is compared with independently assembled sample routes,
including cold shuffled reads and two nested Preserve processors. Preserve
references reuse the canonical stretch engine; they do not independently qualify
that algorithm.

No native UI changed or app was launched. Full workspace tests, optional app
tests, acoustic delivery, performance measurements and emitted-movie equivalence
were not run in this increment. Persisted attachment timing and release
qualification remain open.
