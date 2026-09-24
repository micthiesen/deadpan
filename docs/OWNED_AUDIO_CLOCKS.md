# Owned audio recipes in explicit root clocks

`AudioDefinition::in_root_clock` evaluates a Source, Hold or nonunity Preserve
definition from one immutable plan in an explicit root placement. The selected
plan supplies the current recipe, media contracts and policies. A placement
supplies coordinates only. It is not a frozen raw body, an authored binding,
or permission to read media.

This provides a tested evaluation boundary for the owned-tree approach to
inserted-time audio. Split already retains full editable child contexts. A future
binding can preserve their evaluation clocks without adding a second graph of
historical raw recipes. Persisted bindings, their complete edit lifecycle and
arbitrary Hold insertion remain open. Core 15 and database 21 are unchanged.

## Placement and support

`AudioRootPlacement` has a closed JSON representation and checked constructor:

| Field | Meaning |
| --- | --- |
| `origin` | Exact root-frame coordinate of definition-local zero; may be negative. |
| `root_frames_per_local_frame` | Positive exact scale from local frames to root frames. |
| `local_support` | Nonempty, nonnegative half-open local-frame support, within the selected definition's duration. |

Project frames map to 48 kHz samples with absolute ties-to-even endpoints.
Neither a signed origin nor a support crop resets the grid to zero. The resulting
borrowed `AudioDomain` retains the definition selector and placement, and accepts
only its own plan's reader. Source filter support and output envelopes honor the
requested support. Preserve prepares its complete intrinsic history before the
placed output is sampled and cropped. Ancestors outside the chosen definition
are not implicitly included; callers must select the processing domain they mean.
An explicit support edge has automatic policy and `placement_support` provenance.
Only an exactly coincident authored edge can contribute a Hard exception; a crop
cannot inherit an old edge's choice merely because both round to the same sample.
A positive support may round to zero root samples, leaving no nonempty PCM read.

A Sequence, Repeat or transparent Retime cannot be treated as one physical
domain. Each genuine domain needs its own placement. In an NTSC `A2f, B2f`
sequence, inserting 1f at frame 1 requires A to resume new sample 3203 from old
1602, while B starts at new 4805 from old 3203. One suffix offset cannot express
both. `RepeatDefault` may select a physical default directly, including an
unplayed default, but it does not invent an enclosing play or a birth binding.

## Reading and identity

`StageAudio::read_domain` and `read_domain_transferred` consume these handles
through the existing source, DSP, RoomTone, cancellation and preparation limits.
Returned records include optional `definition` and `placement`; ordinary physical
domain records omit both. Relative paths remain branded by the definition in
root spans, processing descriptors and nested stage signals. Intrinsic Preserve
and RoomTone preparations can be reused across outer placements of the same
definition because their full local grids and inputs are unchanged. Source
dependencies are still checked on cache hits.

Policy comes from the selected owned tree. A new revision that changes Source
alignment or a silent Repeat gap into RoomTone uses that new recipe even when
given the same placement value. An explicitly opened historical revision keeps
its old recipe. Silent Holds are queried on both Preserve input and output grids;
copying an input mask would lose a Hold that owns no input point but gains output
after slowing. Root silence and envelope exhaustion remain explicit, including
when transferring the placed domain to a point grid. No creative fades, device
playback or final master processing are added here.

## Headless inspection

```text
inspect-audio-placement <project.deadpan> (--node <ID> | --repeat-default <ID>) --clock <clock.json> --samples <START> <END> [--revision <ID>]
```

The command reads 1..256 signed root samples without changing project history.
The clock file is bounded to 4 KiB. Rational components are decimal strings:

```json
{
  "origin": {"numerator": "-1", "denominator": "1"},
  "root_frames_per_local_frame": {"numerator": "1", "denominator": "1"},
  "local_support": {
    "start": {"numerator": "0", "denominator": "1"},
    "end": {"numerator": "1", "denominator": "1"}
  }
}
```

`ProjectAudioSession::read_placement` provides the same operation to the host.
Current and historical sessions retain the same qualified-original checks.
Unsupported definition kinds or support beyond the selected definition return
`AudioPlacementUnavailable`. Malformed clock JSON and invalid geometry detected
while decoding it, such as zero scale or empty support, return `InvalidInput`.
Invalid sample intervals return `AudioRangeOutOfRange`.

## Remaining authored semantics

The evaluation boundary does not select or persist bindings automatically.
That model must separately retain owned local anchors, active-cut phase,
independent later-domain starts and compact stable-play placement. Existing
Repeat plays retain their previous phase after movement; new plays need an
explicit canonical definition environment that replaces enclosing placement
while retaining intrinsic edits. A flattened absolute offset cannot encode
that distinction. Split and occurrence copying must remap live owners while
preserving clock relationships, and changed raw contributions must replace
their old policy and invalidate affected opaque preparations.

Historical timing layouts may supply bounded placement indexes. Historical raw
bodies remain useful for explicit revision inspection, but are not required by
this owned-recipe reader. Neither this API nor the timing-only proposal proves
the complete binding lifecycle; do not add a persistent schema by assuming
those remaining transformations are automatic.

[Qualification](qualification/owned-audio-clock-2026-09-23.md) records the actual
PCM/current-recipe tests, independent review and full 1,108-test repository gate.
