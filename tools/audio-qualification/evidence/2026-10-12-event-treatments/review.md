# Self-review

The primary agent implemented and reviewed this increment alone, as requested.
This is not independent review. All changed Rust files, the new tests and the
five-line routing baseline addition were inspected before final release checks.

- Event recipes replace only the previous gain/send fields. Existing scalar
  fixtures retain the same trims and sends; typed gain changes preserve other
  stages. Current packages use core 53/store 81. Frozen schema-43 fixtures stay
  untouched, with their trims adapted in their two test-only loaders.
- Root clocks subtract the complete onset; occurrence clocks also carry their
  owner sampling map. Saved routes select old sample labels. The NTSC regression
  uses an independently assembled rational clock and exact copied samples.
- The new DSP prefix recurses only to stages before EQ. Default and reordered
  recipes are checked against direct convolution. Input edges run before the
  prefix; separate post-treatment gates retain current cuts and Hold policy.
- The complete dry output feeds sends. A sound's own return does not feed its
  Tail tap. Parent processing receives dry plus wet. Neutral processing keeps
  the old scalar path. Allocation, cancellation, plan work, depth and tile
  residency remain bounded, including new EQ workspaces and temporary PCM.
- Native commands capture sound/session/revision. Service writes are reversible
  SetSound commands; no widget directly changes authored state. Gain and send
  updates preserve the complete recipe. Matching envelope ranges replace one
  curve; clearing curves preserves trim/mutes/other stages.
- Mechanical fixture changes preserve all timing, sources, edges and ownership.
  Existing fixture-only root-to-beat copies reconstruct their scalar trim; those
  fixtures carry no other treatment and are unchanged in meaning.

No correctness finding remains from this source review. Final test/export/replay
results and visual inspection are recorded separately in the qualification.
The placement replay's former three-wheel-step reveal bound was too small for
the expanded inspector. Its final helper allows eight bounded attempts and
retains the same real input and complete paint-visibility assertion.
Visual review of the final placement-removal checkpoint found its viewer black
despite a displayed-frame semantic identity. The previous increment's same
capture is also black. Both images are retained and the tracker/handoff carry
the exact follow-up; this checkpoint does not qualify picture presentation.
The new treatment captures do show the picture, and final hashes match the
inspected initial images exactly at both sizes.
Partial attached-sound editing scopes, persistent filtered caching, broader
loop/incoming-tail policy, mastering and performance remain outside this
increment and remain open in the full goal.
