# Authored audio definition output

`RenderPlan::audio_definition` and `StageAudio::read_definition` read an authored
recipe directly on its canonical local-zero 48 kHz point grid. This supplies a
deterministic operand for future new-Repeat-play bindings, separately from an old
physical occurrence's sampled continuity. It does not install a binding, grow a
Repeat or insert time. Core 15/database 21 and `FrozenAudioContext` schema 1 are
unchanged.

## Definition and occurrence are separate

An existing Repeat play has a stable identity and a position in its captured
project clock. Its retained physical domain can preserve its exact old phase.
A newly added play has no old occurrence to resolve. Selecting another play as a
substitute can choose an override, and fails entirely when every old play has an
override but the default child remains the intended new-play recipe.

`AudioDefinitionSelector::RepeatDefault { repeat }` resolves that Repeat's actual
default child directly. Normal and retained-context compilation keep its index
even when no current play uses it. `Node { node }` selects a node's authored output,
as needed for a future WrapRepeat prototype. Neither selector probes the project
root or invents an outer Repeat identity.

The borrowed handle retains its exact immutable plan, selector and resolved root.
It exposes duration and an `AudioSignal` whose support begins at local frame zero.
Its count uses point-ceil allocation. For two frames at 30000/1001 fps this is
3,204 temporary signal samples, while the corresponding final root allocation is
3,203 samples. A definition's storage count cannot set a timeline duration or
replace the root's absolute round-even allocation.

Query, span and nested Preserve descriptors carry the definition selector.
Their `InstancePath` values are relative to that definition root. Nested Repeat
paths still contain actual stable play identities inside that root. Equal aliases
and numerical coordinates cannot make these paths project occurrences.
Ordinary project queries omit the optional selector from their existing JSON.

## Rendering and admission

Definition traversal reuses the point-signal walker, including compact Repeat
orders, sparse overrides, meaningful Edit crops and transparent Partitions.
Nested Preserve retains its full intrinsic input/output history. RoomTone keeps
its loop origin. No parent occurrence placement or ancestor Retime is silently
imported into a node definition; select that ancestor's definition when its
processing belongs to the intended recipe.

`read_definition` accepts 1..256 nonnegative `SignalSample` positions in the
definition's output. It rejects foreign plan handles, invalid ranges, cancelled
work and exhausted limits. Source I/O, DSP preparation, cache residency and
deadlines use the same controlled renderer as ordinary reads. Preserve cache
descriptors and RoomTone keys include definition scope, so cached occurrence PCM
cannot supply a definition solely because node aliases match.

The result is raw stereo PCM before creative fades, voice effects and mastering.
It includes project/revision, selector, resolved root, point-grid start and merged
explicit silent-Hold intervals. Policies are queried on the consuming grid,
including after Preserve processing: a Hold with no input-grid point may acquire
output points after slowing. Missing Source audio remains distinct from a silent
Hold. Rounded input masks are not scaled to manufacture output policy.

Retained plans still require `source_for_context` and complete expected asset
contracts, including on cache hits. The project host authenticates the complete
`FrozenAudioContext` against its exact committed historical revision before these
reads. A serialized selector grants neither media access nor an authored edit.

## Headless inspection

```sh
cargo run --locked -p deadpan-cli -- inspect-audio-definition example.deadpan --repeat-default repeat --samples 0 256
cargo run --locked -p deadpan-cli -- inspect-audio-definition example.deadpan --node clip --samples 0 256
cargo run --locked -p deadpan-cli -- inspect-audio-definition example.deadpan --repeat-default repeat --samples 0 256 --revision prior-edit
```

Protocol 1 labels the returned `audio` object
`definition_output_pcm_before_effects`. Invalid or wrong-kind selectors return
`AudioDefinitionUnavailable`; invalid intervals return `AudioRangeOutOfRange`.
The same command is available through `deadpan-app --headless`.
The optional trailing `--revision` selects an exact committed historical snapshot,
including an abandoned branch, through `ProjectAudioSession::open_revision`.
Its aliases and source receipts come from that revision; no current-head fallback
is permitted. `read_definition` also works with an authenticated `open_context`
session. Both paths can read an old definition after its current node is deleted.
Inspection does not move the history cursor or mutate the document.

## Remaining binding work

Future insertion bindings need two explicit operands: a captured physical domain
for a surviving occurrence, and a captured definition for newly born plays.
The planned fresh-play convention binds definition sample zero at each new
play's meaningful start; it does not claim historical occurrence continuity.
One compact allocation-run rule should select the prototype without expanding
every play. Existing survivors retain their separate physical/root maps.

The [owned recipe clock](OWNED_AUDIO_CLOCKS.md) now supplies a physical definition's
current raw body in an explicit root placement. The preferred binding approach
uses the complete children already owned by Split, with timing-only indexes and
explicit lexical Repeat arguments, instead of a second frozen raw-body graph.
Live ownership transforms, exact composed anchors, birth environments and shared
preparation remain required. Explicit historical contexts still authenticate
against committed revisions; never label reducer intermediates as committed
snapshots. See [the splice design](STRUCTURAL_SPLICE_DESIGN.md) and
[physical domains](AUDIO_PHYSICAL_DOMAINS.md).
