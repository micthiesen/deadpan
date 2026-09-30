# Native encoder qualification review

Read-only review of `native/deadpan-encode/examples/qualify.rs`, `tools/media-qualification/compatible/qualify_native_encode.py`, `test_native_encode.py`, and the new `export_probe.c` GOP mode. No commands were compiled or executed as tests. Source reviewed on 2026-09-29; other agents may edit it after this report.

## Findings

### P1: Unqualified AVFoundation timing is admitted as a passing case

`qualify_native_encode.py:138` uses only `case["avfoundation"]["checks"]["passed"]`. The imported `inspect_native_audio` deliberately returns `passed=True`, `outcome="unqualified"`, and `event_timing_qualified=False` when exact native timing cannot be interpreted. For example, nonzero decoded trim metadata raises `UnqualifiedTiming`; `native_audio_oracle.py:240-242` records that failure as diagnostic only. It does not inspect event timing afterward. The new harness can therefore report an entire production-adapter case passed without measuring the independent decoder's absolute PCM timing.

Require both `outcome == "passed"` and `event_timing_qualified is True` for cases claiming native timing qualification. Preserve the raw diagnostic and an explicit unqualified result for other cases. Add a runner test using a native result with passed true and unqualified outcome; it must never contribute to an overall qualification pass.

### P1: Both short cases have no effective audible-content assertion

`qualify_native_encode.py:188-191` adds one-frame 60 fps and nonzero-origin NTSC cases, then treats the generic audio oracle's `passed` value as sufficient. Their marker positions are `[100,400,600]` and `[100,800,1401]`. All diagnostic windows use radius 4096, so these windows overlap. `encoder_oracle.py:441-479` intentionally marks every event-amplitude, offset, and distinct-peak check diagnostic in that situation, and sets `event_timing_qualified=False`.

Consequently, correctly timestamped all-zero decoded PCM can pass the short-case audio checks: it is finite, contiguous, and has an admitted endpoint, while every failed audible-event assertion is excluded from acceptance. The AVFoundation path has the same issue. This also weakens the only nonzero-origin case's encoded evidence.

Require qualified event timing for the existing long fixtures. For short fixtures, add a separately justified bounded absolute-clock content oracle with nonoverlapping identification regions or a same-payload reference; do not fix this by shrinking a window until a shifted event disappears or by aligning PCM to an event. Until that oracle exists, retain short output as unqualified evidence rather than a passing timing case. Add negative tests replacing all short PCM with zero and shifting/cropping an opening or terminal event.

### P2: Ignored GOP/B-frame controls can pass the requested-policy matrix

`inspect_gops` at `qualify_native_encode.py:36-54` proves independence only for keys the decoder reports. The generic oracle records keyframe intervals but never compares them with the half-rate policy. A 90-frame stream with a single opening key passes every fresh-GOP comparison. It also checks only `maximum_b_run <= requested_b_frames`, so a TargetTwo stream with no B pictures passes.

The fresh decoder mechanism is useful and should remain: `export_probe.c:517-537` allocates a new codec, seeks before submitting any packets to it, drains to EOF, and reports all frames; Python compares the complete suffix without dropping preroll. Add a separate measured-policy result using observed key intervals and B pictures. If the platform cannot honor a requested control, retain that capability limitation explicitly rather than classifying the requested control as qualified. Do not use the codec's queried `gop_size` or `max_b_frames` as emitted-bitstream proof. Add tests for one opening key over a multi-GOP fixture and TargetTwo output with zero B pictures.

### P2: New wrapper drops existing actual movie-timescale check and does not gate stream starts

`qualify_native_encode.py:124-135` checks fast-start order and mere edit-list presence, but omits the actual `moov/mvhd` timescale check present in `qualify_encoder.py:212-215`. `inspect_case` checks the Rust fixture's declared `source.movie_timescale`; that is copied from its input policy, not read from the output box. The old check should be retained against independently parsed `boxes["timescales"]`.

The audio oracles retain stream/track starts but gate only stream endpoints and event errors. Neither this new wrapper nor the generic checks explicitly rejects a nonzero presented stream start. This matters under spec 22.3's explicit stream-start requirement: an early interval trimmed from a track, with unchanged terminal time and later markers within tolerance, can evade a starts-at-zero admission rule. Add exact expected presented video/audio stream-start checks, keeping the manual decoder's negative priming samples distinct from the container's presented stream start. The current box parser only counts `elst` entries; it does not interpret edit-list media times/rates, so its two presence booleans alone do not prove delay/padding/reordering-only edits.

## Scope observations

- Rust fixture sample count correctly uses `B(project_start + frames) - B(project_start)` with integer ties-to-even. Video and encoded PCM begin at output zero; the fixture does not claim to read actual project PCM.
- No event alignment, PCM shift, packet dropping, or AAC-block tolerance was introduced in these files.
- The `edges` fixture currently feeds extra opening/terminal content, but both imported oracles explicitly leave edge-content survival unqualified. Retain that limitation; its passing long impulse checks do not prove the boundary samples survived.
- `test_native_encode.py` currently tests range phase and GOP comparison mutations only. Add runner-level tests for the acceptance predicate so diagnostic-only and unqualified reports cannot be promoted accidentally.
- The harness records the exact encoder executable and linked libraries. Its inherited source inventory does not include the new Rust/C encoder crate or Cargo.lock; retain that source/build identity in the eventual qualification evidence alongside binary hashes.

## Suggested verification after fixes

Run the parent's existing bounded Python oracle/runner suite, including new negative controls, then the actual native encoder matrix with both FFmpeg decode modes and AVFoundation. Retain every rejected, unqualified, and successful case independently. These checks qualify a synthetic production adapter boundary; they do not complete project export, full mastering, emitted-project verification, or destination publication.
