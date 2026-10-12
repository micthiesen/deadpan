# Solo review

The primary agent implemented, reviewed and verified this change. No subagents
or live GUI were used. This is a self-review, not independent review evidence.

Reviewed the complete production/test diff, the new contribution module and
callers through plan compilation, source admission, recursive EQ, full-bus
preparation and real CLI Render. No schema, command, policy permission, decoder
timing tolerance or dependency changes were made.

## Findings resolved during implementation

- Evaluation mode initially depended on disagreement in the requested range.
  An immutable plan-wide flag now fixes the arithmetic path across windows.
  Cold reads at activation, deactivation and single-sample boundaries agree.
- Initial fallback repeated sound preparation and exhausted the shared preflight
  budget in two existing Repeat/Preserve tests. Routing before preparation fixes
  this without increasing limits. Both failures and corrected suites are kept.
- Independent clocks initially created duplicate EQ contexts. A one-context
  limit failed with `Limit("EQ owner count")`; choosing one active input context
  for all selected contribution outputs passes that limit. Gain clocks remain
  per contribution. No stale-cache PCM discrepancy was reproduced.
- A proposed inside-owner pause regression failed an existing explicit core
  restriction before audio. Partial attached-sound processing scopes need an
  owner-local timing route and remain open. The final test preserves atomic
  refusal and uses an already-supported root SoundEvent to verify PCM after
  insertion. No enclosing-processor guard was weakened.
- The first export fixture selected a neutral crop wrapper with equal clocks.
  Its own pre-render assertion caught this. The corrected fixture attaches the
  sound and effect to the underlying Source, puts the sound in the visible
  Source range and proves different clocks/gains before successful rendering.

## Reviewed contracts

Saturation derives one factor from the complete permitted sum; exact zero uses
its continuous derivative limit. Single-input paths retain direct shaping.
EQ is linear and recursively reads each selected contribution through the full
jointly driven prefix. Its cache key includes selected output and the complete
permission mask. Selecting an output never selects the saturation inputs.

Each contribution keeps f64 PCM and its own gain clock through child-to-parent
owners. Exact Q32 gain accumulates across linear stages and flushes only at
shared nonlinear/temporal stages. Repeat escalation and event gain enter once.
Bounded PCM admission includes contribution storage and maximum pending gain
programs, occurs before media access and uses existing work/deadline limits.
Cancellation, missing-media recovery and silent-Hold permission isolation pass.

Decoded WAV oracles cover differing mute/envelope clocks, all six gain/EQ/
saturation orders, exact cancellation, nested group reduction, cold partitions
and supported rebound history. Direct convolution starts from inspected f32
PCM and permits four f32 epsilons; no existing tolerance was relaxed.

The new release fixture uses typed commands, proves its two owner clocks and
post-EQ gains differ, and verifies actual encoded output. No listening,
interactive-latency, native UI or second-machine claim follows from these checks.
Arbitrary sends, incoming-tail policy, broader scope/binding qualification,
persistent filtered caching and the full product goal remain open.
