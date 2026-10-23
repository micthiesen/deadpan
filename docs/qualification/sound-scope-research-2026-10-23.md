# Retained sound-scope grouping research, 2026-10-23

Journal-scope narrowing is not a valid implementation of aligned grouping.
A two-placement counterexample drops a retained terminal label from an existing
ancestor EQ's input and changes 1,899 canonical PCM channel samples. Production
code and its existing grouping refusal remain unchanged. No requirement or gate
is newly complete; coverage remains 89%, with 8/24 strict sections complete.

## Controls and counterexample

The first 40 cases compare a group created before capture against a test-only
manual scope rewrite followed by ordinary grouping. They cover Source and
RoomTone owners, five NTSC phases, existing and subsequently added ancestor EQ,
and early and near-terminal finite events from the existing 8,197-sample WAV.
Complete PCM, alternative read partitions, signed Top/owner inputs, support and
suppression match. Forty paired ablation cases prove that Top EQ and the event
each change numerical PCM. An independent audit compares all 1,409,408 stereo
samples and the retained bit arrays. These constrained passing cases do not
establish the general scope representation.

The initial serial run reached its 600-second harness limit after 30 completed
passing cases. The remaining ten pass individually with at most four processes.
An archive destination setup error and the corrected negative-fixture preflight
assumption are retained separately from audio assertions.

Four attempted public partial Top captures refuse before audio at the existing
supported-endpoint guard. A different test-only experiment adds an exterior
Partition through public JSON admission on both representations. Those four
admitted states match all 44,844 final stereo samples and signed contexts, with
active EQ/event ablations. This does not prove a supported editing path can
create those crops.

The decisive Source case uses the established four-frame `SelectedPlacement`
event from actual WAV samples `[1024,7432)`, Hard edges and zero sample offset.
An ordinary Top Sequence owns a distinct EQ. The independent control groups the
complete owner before capture. The experimental branch manually narrows its
journal after a five-frame prefix and then groups the unchanged owner. Both
branches subsequently insert a separate one-frame prefix through normal commands.

At 30,000/1,001 fps and 48 kHz, the owner allocations are:

| State | Owner frames | Owner sample allocation | Count |
| --- | --- | --- | --- |
| Birth | `[4,8)` | `[6406,12813)` | 6,407 |
| First placement | `[9,13)` | `[14414,20821)` | 6,407 |
| Second placement | `[10,14)` | `[16016,22422)` | 6,406 |

The birth terminal label `12812` transports by `8008 + 1602` samples to `22422`.
Top's complete input retains this label in the control. Manual scope narrowing
changes its right channel from `0.0010018165921792388` to zero. The independent
owner's own context still matches, so an owner-only PCM control misses this loss.
Ancestor EQ spreads the changed input across 1,899 bus channel samples, from
sample 20421 through 22421. Both complete read partitions agree within each
representation. Top EQ and event ablations are active; the terminal witness is
nonzero in the control. No tolerance, alignment correction or DSP change is used.

## Inputs and retained evidence

Baseline production Rust and six supplementary inputs match
`4e357bc573a269782f4515b456fa5e080c37352c` by Git blob audit. Subsequent committed
changes during these experiments affect progress documentation only. All
experiments add test controls and one module registration; production code is
unchanged. Each build freezes the complete Rust path set and supplementary
inputs and records the executed binary SHA-256.

The decisive optimized test binary is
`532f0b6d19b38bf33a81ee5cf66492709f37b66acbbbeb46b72f7502867c52b4`.
Its assertion fails with the measured representation difference, rather than a
setup error, timeout or unsupported command. Complete pre/post documents, PCM
bits, raw context windows, ablations and per-sample differences are retained in
[the evidence directory](../../tools/media-qualification/evidence/2026-10-23-sound-scope-research).
The inventory covers 43 files and 6,249,101 bytes, excluding its own manifest.
Full positive-case arrays also remain under
`/tmp/deadpan-aligned-sound-scope-2026-10-22/`; their durable independent hash audit
is included. The actual WAV provider convention is a test fixture, not a
production media-qualification receipt or a listening claim.

## Implementation direction

Preserve existing journal scopes, timing tables and ancestor aliases. An explicit
per-reference relation can identify ordinary live groups introduced after each
capture without inventing historical processing contexts for those new groups.
A scratch prototype passes seven focused correspondence/serialization tests,
but it is not integrated into the reducer, copy lifecycle, planner or Store.

Copying a new group alone must keep its live scope: older references need an
explicit unpaired unary root, while a later capture can pair the group normally.
The mixed-history terminal case, exact copy renaming, work bounds, durable
history, native workflow and emitted-media verification remain owed. Keep
processor-interior, cropped placement, Repeat/Retime and root-owned transport
refusals until their own complete lifecycle is established.
