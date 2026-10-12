# Owner input contexts, 2026-10-11

The shared plan/audio boundary now retains the raw input contexts needed for
the next part of EQ integration. An owner cannot be reconstructed from its
nominal automation clock when an enclosing Preserve or resumed binding owns
its actual PCM. A current-root halo also loses hidden siblings at some crops.

`RenderPlan::audio_owner_input` validates the observed owner against the same
immutable plan, occurrence and current sample. It captures a whole Sequence,
Repeat or leaf with its signed absolute grid, inherited support and edge
constraints. Transparent Partition allocation does not remove hidden input.
An enclosing Preserve/binding remains intact and is reported explicitly.
Its other descendants are not automatically attributed to the requested owner.

`StageAudio::read_owner_input` uses the existing qualified source admission,
bounded processing, suppression and creative-fade path. Each request shares
one deadline/work allowance, including context resolution. Foreign contexts,
cancellation and invalid ranges reject before opening media.

Resumed bindings retain a separate checked raw operand and exact sample map
through `AudioOwnerInput::retained_at`. Earlier reference history can remain
readable even when it maps before the current root or delivery interval. The
descriptor retains RootRoundEven versus PointCeil coordinates; it does not
reinterpret them as the current project's samples or silently pad missing
history with zero.

## Verification

The complete plan/audio suites passed 932 tests. After adding the PointCeil
regression, all six owner-input plan tests passed. Strict workspace Clippy with
all targets and `ui-harness`, formatting and whitespace checks pass.

The new plan tests cover hidden siblings and negative labels, distinct Repeat
plays/default gaps/whole-Repeat contexts, enclosing Preserve, resumed reference
history, PointCeil labels, foreign handles and exact work exhaustion. A billion
plays still resolve with bounded work rather than expansion.

Five audio tests use the existing real decoded stereo WAV fixture and the
actual native processing path:

- Twenty cold, reverse-order reads of a cropped group's 5,120-frame context
  match its uncropped edge-faded reference exactly. Feeding that context into
  EQ gives identical output; discarding the hidden history changes the result.
- Cropped descendants retain the complete enclosing Preserve output. Cold
  interior and endpoint reads match the uncropped reference exactly, while a
  nominal FollowSpeed reconstruction differs.
- Moved NTSC bindings match current output PCM and retain silent-Hold policy.
- A resumed owner's map exposes real raw PCM before current delivery; the
  transferred hidden history matches the original reference exactly.
- Foreign-plan, cancelled and out-of-range requests make zero provider calls.

The primary agent reviewed the implementation and tests without subagents.
Review checked capture bounds, stable occurrence identity, Preserve/binding
boundaries, exact reference maps, admission ordering and the distinction
between a delivery context and reference history. Initial test compilation
needed explicit array lengths and the existing cancellation predicate. An
assertion initially inspected only the fixture's silent left channel; it now
checks both channels and also proves that discarding history changes EQ.

Verification commands, logs, source hashes, fixture hash and local toolchain
identity are retained in
[`2026-10-11-owner-input`](../../tools/audio-qualification/evidence/2026-10-11-owner-input/).
This is local decoder/DSP verification, without a device run, GUI or encoded
export. No editor EQ control is exposed by this change.

## Still owed

Connect these contexts to ordered authored EQ. Transport retained reference
edges and evaluate gains/mutes at their serialized positions. Build complete
owner contributions and group sums, preserving independent sound identity and
silence/incoming-tail permissions. Extend cache identity and aggregate graph
admission, then captured native/semantic commands and actual preview/export.
`AudioTreatments` still admits gain, saturation and stereo balance only.
DP-09/DP-16 and the complete-product goal remain open; no requirement was narrowed.
