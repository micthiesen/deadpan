# Measured source import timing

`deadpan-media::source_import_timing` derives a candidate Source and presentation
basis from measured video/audio indexes. It does not register an asset, retain
media, establish import readiness or change a project. The separate
[source registration host](SOURCE_REGISTRATION.md) persists qualification
receipts and indexes and applies an atomic authored edit.

## Common origin and independent streams

Both indexes must identify distinct streams of the same original bytes. Video
must have a positive final decoded-frame duration. Audio must have nonempty,
contiguous available sample coverage. An unavailable interior audio interval
fails derivation; it is not collapsed or filled by inference.

The common origin is the earliest selected stream start expressed in exact
seconds. Original source spans remain unchanged. Each stream's project-frame
start is `(source_start - origin) * project_rate`; its duration is the exact
source extent multiplied by that rate. The beat encloses their union, rounding
its end upward once. This full-source enclosure is distinct from the spec's
nearest-frame policy for typed time input.

Core schema 8 adds `placement` to both mapping enums:

```json
{
  "type": "placement",
  "start": {"numerator": "2", "denominator": "3"},
  "frames": {"numerator": "28750", "denominator": "1001"},
  "endpoints": "hold_adjacent"
}
```

The picture mapping requires an endpoint policy. Audio has the same `start`
and `frames` fields without `endpoints`; its signed 48 kHz `audio_offset` remains
an additional translation. Import candidates use offset zero because fractional
cross-clock alignment belongs in the exact placement. Mapping starts may be
negative for authored edits; derived full-source import starts are nonnegative.
Start and end must be representable signed project-frame coordinates, with a
positive duration. Checked arithmetic also covers audio's added mix offset.

Picture requests subtract the exact start before mapping into original time.
Inverse source anchors add it. Nested Repeat and Retime mappings retain this
precision. The existing reversible mapping commands and occurrence edits accept
the new variants; marks remain in their own clocks. Selected endpoint holding
covers picture lead, tail and frame-rounding slack without selecting a frame
outside the original selected span. It never extends source-anchor validity.

`MeasuredAvailableCoverage` preserves every available decoded audio sample.
Explicit skip/discard evidence is already represented by the audio index.
Unknown priming is retained. A codec name, container origin or fixture author's
knowledge cannot supply a missing trim decision.

## Presentation-basis candidates

Cadence uses adjacent measured presentation intervals. Exact CFR is retained.
A VFR candidate requires a repeated modal interval with at least 25% support
and intervals that are integral multiples from one through eight of that mode.
Ties choose the shorter interval. At most 256 distinct intervals are admitted.
A single frame uses its measured positive terminal duration. Other ambiguous
cadences return an error requiring a later host policy, never an invented rate.

Rates above 60 fps choose the highest common rate that divides the observed
cadence exactly, otherwise its smallest integral divisor reaching at most 60.
The result records the observed rate, divisor, confidence and interval counts.
This conservative candidate policy does not qualify all VFR input.

Geometry applies sample aspect ratio before rotation, then rounds each display
axis to the closest positive even dimension, downward on a tie. It records the
exact display dimensions and relative aspect error. Each axis differs by at
most one pixel, including the minimum 2-pixel raster. There is no enlargement to
a target resolution. The current qualified source path supplies SDR metadata;
this helper does not qualify HDR or clean-aperture interpretation.

`SdrRec709` is the project's proposed output policy, not a replacement for source
primaries or transfer. BT.2020 and Display P3 SDR sources retain their native
metadata, as do linear-transfer sources. The shared renderer interprets that
source metadata into linear Rec.2020 before its explicit SDR display transform.
The helper borrows the source metadata unchanged; qualification receipts
retain it separately from the presentation basis.

The provisional audio-only basis is 1920×1080 at 30 fps. These are candidates,
not basis-adoption state. The first primary video insertion, provisional-rate
locking after a timed edit and explicit later geometry adoption remain host
work under specification Section 4.

## Persistence and evidence

Database schema 14 migrates schemas 1 through 13 into core schema 9 by replaying
their complete history against frozen core wires. Schema-12 picture and audio durations remain
unchanged. Older history rejects placement variants and fields, including null
fields in snapshots, commands and patches. Migration preserves all operational
rows, revisions, branches, marks and pending redo with a pre-migration backup.
Schema-13 placements remain unchanged, and old assets gain no inferred source
qualification. See [source registration](SOURCE_REGISTRATION.md).

[Qualification](qualification/import-timing-2026-09-21.md) records actual media
fixtures, exact timing cases and the old-binary migration fixture. No playback,
audio resampling, authored import UI or export is established by this slice.
