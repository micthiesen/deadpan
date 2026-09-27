# Next sound integration work

The current source and sampled-route increments are preparation APIs. They do
not persist sound events, add editor placement controls, grant Hold allowances,
apply creative voice edges/effects, or put a new voice into the final master bus.
Core 28/database 34 remain unchanged. All product requirements and gates remain
active. Do not claim a usable complete editor or release qualification.

The workspace playback wait failures need diagnosis before further large test
expansion. Four cases hit tests.rs:258's ten-second helper in this run; three
were already present in the prior checkpoint, and seek-cache reuse was additional.
Do not infer their cause or erase them with isolated passes. Distinguish worker
state/progress and debug-build test contention from product timing guarantees.
The new complete Preserve PCM test deliberately rebuilds its full request-local
history for shuffled bounded reads and takes substantial time. Evaluate whether
a smaller set of adversarial processed blocks plus the existing dense pure-map
tests retains the same behavioral witnesses with less test contention. Do not
weaken production budgets or silently increase test thresholds to hide failures.

AudioRoutedSignal has complete independent source input and shared intrinsic
Preserve cases on PointCeil. AudioRoutedRoot retains a complete projected output
on RoundEven. Both read exact old sample labels under one StageAudio budget,
including full dependency/history admission for gaps. They are direct read APIs;
they are not yet operands accepted by AudioSignalTape/AudioStageProjection or
the authored voice graph. Preserve that distinction in further composition.

Before schema work, settle the complete authored voice path, its current policy,
and where a retained route enters enclosing time processing. A route selecting
processed output differs from a route editing the input of an enclosing Preserve.
Reuse the existing shared controlled reader and immutable projection identity;
do not create fresh frame TapeRuns that lose retained integer phase. Do not split
the continuous Original voice into per-beat processors. Keep default independent
voice processing separate from explicitly aggregate processing of a mixed input.

Keep three masks separate: route gaps/old output selections, source filter or
full canonical DSP support, and current consuming Hold policy. Current allowances
must match both voice identity and exact Hold/Repeat-gap issuer. Captured provider
policy is part of old evaluation and cannot be mistaken for current grants.
In particular, a future explicit allowance cannot recover samples already gated
away inside a captured raw recipe. Choose the authored raw/provider policy and
current output gating deliberately. Test a permitted sound alongside suppressed
Original speech/decay, a second unpermitted Hold, and copied occurrence scopes.

Persist one complete logical event recipe plus stable physical fragments.
Use record-array wires for composite keys and explicit event/fragment edit scope.
Split retains logical identity; copy/isolation forks one logical recipe per
old represented owner while retaining within-copy sharing. A fragment cannot be
keyed only by event+owner because Ungroup can leave multiple projections there.
Root Split moves the prior root bus into retained contexts and clears the new
root copy. Partition refinement separately projects its own sounds. Deleting
an event's beginning keeps surviving ancestor-event suffixes. Ungroup uses exact
pre-edit prefixes and full processing history. InsertTime/Splice must capture
old clocks before mutation. The detailed previous map is retained in
/tmp/deadpan-sound-placement-20260927/next-slice.md.

FrozenAudioContext schema3 cannot silently omit new vocabulary. The later schema
increment needs strict frozen core28/database34 grammar and authentic old-CLI
fixtures, including patches and operational chronology. Do not add default fields
to OccurrenceIdentities: older adapters reuse that type. The authentic old CLI
remains in /tmp/deadpan-sound-placement-20260927/old-deadpan-cli with its baseline
hash. No new migration was fabricated in this increment.

Final summation belongs before the existing shared limiter, with all live and
retained voice dependencies admitted. No-events output must retain the Original
PCM. Follow this through typed commands, durable undo/redo, native sound placement,
CLI and contributed UI replay. Keep the sound board and single-Original design
philosophy authoritative. Acquisition, analysis, actual local AI, export,
recovery, keyboard/aesthetic/accessibility and release gates remain required.
