# Next: retained sample routes into real PCM

Core 28/database 34 remain unchanged. Independent catalog source operands now
feed real qualified PCM through the existing tape and Preserve machinery. The
source voice input excludes current Hold gates; its output applies current
structural issuers separately. Host admission, full source/filter support and
strict projection scope remain required. Do not treat this as persisted events.

The previous sound-clock increment supplies AudioSoundRoute, but its sampled maps
are not connected to PCM. The read-only sound_voice_integration explorer proposes
two explicit brands for the next vertical slice:

- AudioRoutedSignal over a complete independent source input or immutable
  Arc<AudioStageProjection>, matched to its captured PointCeil SignalSample grid.
- AudioRoutedRoot over a complete AudioProjectedRoot, matched to its captured
  RoundEven AudioSample grid. Raw source root placement needs a separate checked
  capture; do not infer it from an intrinsic source signal.

This is a proposal, not implementation. Prefer closed source/projected operands,
not a new generic graph. Require matching original Recipe grid, extent, allocation,
origin, spacing, rule, sampling anchor and complete support. Same sample count or
duration alone is insufficient. Reject cropped/rebased operands unless their full
capture is retained. Keep live plan, owner definition/occurrence and opaque identity.

AudioSoundRoute returns exact lookup into the original recipe. Convert that
lookup using the captured old grid into integral old sample labels. Query that
old provider clock, split only at original selection/mask boundaries, shift the
physical allocation, and compose the old AudioSampleMap by integer resume. Do not
build fresh frame TapeRuns from route spans or reconstruct phase from final frame
endpoints. Existing stage_recipe consumes an exact map; its use is safe only when
that retained map is supplied directly.

StageAudio readers should share WorkControl and existing resolve_source,
prepare_source_block, prepare_projected and sample_prepared. A cold suffix or
fully masked route must still preflight complete projected dependencies and keep
the full CanonicalRecipe. Keep one projection Arc/identity across spans; never
restart DSP per fragment. Ordinary Original readers stay unchanged.

Keep three masks separate:

1. Route output allocation, prior gaps and old selected audible masks.
2. Original source filter support or complete prepared-output support.
3. Current consuming Hold policy, resolved by issuer on the destination clock.

Captured provider policy remains part of its old evaluation, separate from new
consuming policy. Initially label this routed preparation before creative edges
and current consuming voice gates. Source admission should require input_signal,
not claim that routing an output view's old gates implements Hold allowances.

Real PCM witnesses should reuse playback tests/source_voice.rs registration and
independent 44.1 kHz resampling/canonical stretch references. For two one-frame
insertions at NTSC, PointCeil's second suffix selects old index3203; RoundEven's
second suffix at new B(4)=6406 selects old index3204, distinguishable from3203.
Use nonzero/changing PCM there. Compare with dense previous-array copying. Cover
cold suffix first, cropped/fractional Window, shuffled reads, full versus wrongly
cropped filter support, preserved gaps, displaced-rounding extra-sample silence,
foreign/mismatched capture, revoked source, fully masked dependency admission,
hidden history limits and unchanged Original PCM.

Suggested disjoint owners: plan route handles/accessors and plan tests; audio
controlled readers and narrow helper reuse; playback real PCM tests. Parent owns
Cargo. Keep all agents in the shared checkout and preserve concurrent work.

After route-to-PCM proof: resolve scoped allowances/edges; persist bounded event
recipes and stable fragments through actual reversible commands/migrations;
compile all voice dependencies and sum the complete bus before one existing
limiter. The event identity/structural transform map remains in the prior scratch
next-slice.md, including strict frozen core28 migration requirements. The full
product, GUI aesthetics/physical keyboard/accessibility, acquisition, AI, export,
recovery, performance and release gates remain active.
