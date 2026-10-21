# Processing-ancestor Hold-duration PCM proposal

Prepared 2026-10-20 in scratch only. No repository writes or Cargo execution. The source is a proposed regression, not observed PCM evidence. Preserve/Shift behavior is still unqualified by this proposal until root runs it.

## Minimal runnable integration

Copy `processing_hold_duration.rs` from this directory to `crates/deadpan-audio/tests/stages/processing_hold_duration.rs`. In `crates/deadpan-audio/tests/stages/insert_time.rs`, add:

```rust
#[path = "processing_hold_duration.rs"]
mod processing_hold_duration;
```

This placement reuses the existing stage helpers and `mono_original` deterministic 44.1 kHz provider through `use super::*`; it does not introduce a mock mixer or decode/export claim. The new module has its own narrow fixture and typed input-tape reader.

Focused test names:

1. `ordinary_hold_growth_under_preserve_retains_44100_phase_on_actual_input_tape`
2. `ordinary_hold_growth_under_shift_retains_44100_phase_on_actual_input_tape`
3. `ordinary_hold_growth_under_processing_preserves_prebound_and_cropped_input_clocks`

Run the first minimal Preserve test against unchanged code before a production edit. The predicted failure is at current input sample 9610: old literalphase 3100 versus freshphase 3100+147/400. The existing canonical sample at 3100 is-.09149733; a previously observed affine fresh-phase witness at 3100+147/400 was-.10523279, but this processing-input proposal has not executed yet. Its failure label reports whether ALL 256 actual samples exactly match the independently predicted fresh-phase recipe.

## Actual public API route

`RenderPlan::audio_signal().query(SignalSample(0)..SignalSample(1), AudioQueryLimits::default())` obtains the actual processing `AudioStage`. If the prebound variant exposes `AudioSignalContent::Bound`, use `AudioBound::intrinsic_stage()` instead of rebuilding a descriptor or assuming a leaf.

`AudioStage::input_signal()` returns the complete selected structural operand, before this processor. Put that unchanged handle into one identity `AudioSignalTapeRun::new(support.clone(),support.clone(),input_signal)` and `AudioSignalTape::new(plan,support,runs)`. `StageAudio::read_tape` reads the typed input tape on its actual PointCeil grid. The fixture has only Holds and a Source inside the processor, so this read never compares or invokes the changed outer processor's output DSP.

The typed tape belongs to the exact immutable compiled plan. The helper checks reported revision, stage label, exact sample count and absence of suppression in the interior Source window; old/new reads use separate plans and provider revision admission. Alternate chunk schedules must be bit-identical. No plan coordinate is used to construct the expected PCM.

## Literal clock arithmetic

Project 30000/1001, canonical mix 48000, source 44100. One project frame spans 8008/5 mix samples, source step 147/160. Fixture child is [silent lead 3, silent Hold 2, natural-rate Source 4 at source 3100], child duration 9. Raw Hold 2->3 makes child 10; the selected input end 8 remains valid. Every stage output recipe and root duration stay fixed, and the inverse must restore the exact old document.

Direct minimal Preserve: input selection[0,8), output 4, input tape 12813 points. Shift variant uses selection[0,8), output 8, Shift+3; it processes even at unity rate and has the same input grid.

- Old Source entry childframe 5: ceil(5*8008/5)=8008. Exact oldphase 3100.
- New entry childframe 6: ceil(6*8008/5)=9610. The exact frame boundary is 9609.6; the new live sample is.4 mix samples later. Fresh sourcephase is 3100+.4*(147/160)=3100+147/400.
- Read exactly 256 samples at each known entry. Both reads stay more than a thousand samples from the current finite Source crop, so endpoint sinc changes cannot explain an entry-phase failure.

Cropped variant: selection[1,8), Preserve output 4 or unity Shift+3 output 7. PointCeil grid origin is selectedframe 1; it has ceil(7*8008/5)=11212 points.

- Old entry relativeframe 4: ceil(4*8008/5)=6407, versus exact 6406.4. Old phase 3100+.6*(147/160)=3100+441/800 (2480441/800).
- New entry relativeframe 5:8008 exactly; fresh phase 3100.
- Expected moved input must retain oldphase 3100+441/800 rather than freshly sampling 3100. Using RootRoundEven would give 6406 and the wrong expectation, which this control avoids.

The prebound variants use public `capture_unbound_audio_bindings` on the OLD document and serialize that exact state before the duration command. These are controls for preserving existing PointCeil clocks; an old-code prebound case may already pass even while the initially unbound witness fails. The test follows a bound processing output to its intrinsic stage, leaving child bindings active. It does not clear historical output clocks.

## Evidence limits and next step

This proposal tests first 256 input PCM only, including first-sample phase, explicit audible support and old/new chunk schedules. It deliberately does not compare structurally changed processed output, full crop endpoints, generated fallback, sounds/marks, nested processors, Repeat scopes or derived owned PreserveInputProjection. Those should follow a reproduced minimal input failure and a bounded clock-specific design. The ancestor-above-affine coordinate case from `processing-next.md` is also deferred: it needs PointCeil 7208 rather than RootRoundEven 7207 and step 49/40, and should remain separate from the minimal direct-input witness.

The current fixed-affine source/tests and release-export files were not changed by this scratch work.
