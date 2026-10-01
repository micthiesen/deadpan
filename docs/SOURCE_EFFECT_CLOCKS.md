# Effects across Source context growth

Core schema 38/database 47 retain camera paths and audio treatments when an
existing physical Source gains earlier or later context. These operations are
prerequisites for atomic Trim authoring; they add no Trim command or native mode.
Combine them with [retained audio origins](SOURCE_ORIGINS.md).

## Physical owner policy

Physical owners grow to expose context. Cropping keeps the complete owner behind
a transparent Partition. A prefix moves existing material from `x` to `x + p`,
where `p` is a nonnegative whole-frame duration. Tail growth keeps local zero.
An inverse document patch restores the original owner exactly. These helpers do
not implement destructive removal of a physical owner's leading context.

The caller must update media placement, selected windows, duration, retained
audio bindings and effects in one validated transaction. A successful helper
does not establish media handles or authorize a partially updated document.
Before growth, any `FitBeat` media mapping must become an equivalent explicit
mapping using qualified context, or the operation must reject it. Leaving
`FitBeat` unchanged while enlarging its owner changes the media's playback rate.

## Camera path

`Framing.clock` defaults to `OwnerOutput`, omitted on the wire. Such a path still
normalizes against its current owner's complete duration. Existing Hold/Repeat
duration edits retain that behavior.

`RetainedOutput { offset, duration }` instead maps current physical coordinates
into the original authored domain:

```text
authored_local = clamp(current_local + offset, 0, duration)
```

`Framing::prepend_owner_frames(prefix, previous_duration)` returns a clone with
the original positive duration retained and the prefix subtracted from the
offset. Later calls retain that same duration. A zero prefix freezes the domain
for tail growth. New handles hold the original first/last pose. Normalized keys,
curve controls and the existing exact/Q32 interpolation rules stay unchanged.

Evaluation first checks the current owner's coordinate bounds. Picture-plan
inspection continues to report physical coordinates and current duration; only
the recipe's normalization uses the retained domain. Parent framing retains its
own clock and composition order. Splits and copies preserve the whole recipe.
Camera adjustments retain this clock; an explicit Camera reset creates a new
static `OwnerOutput` recipe.

## Audio treatments

`AudioTreatments::with_owner_prefix(prefix)` returns a validated clone. It adds
the prefix to every gain-envelope range, segment endpoint and mute range. Gain
values, curve controls, trim, global mute and the wire format stay unchanged.
Tail growth needs no translation because these keys already use absolute local
coordinates. New factors use the current physical owner's coordinates.

The existing bound-audio evaluator samples those translated coordinates through
the retained voice clock. Adding a second clock offset to gain evaluation would
shift it twice. Independent root gain and sounds keep their own coordinates.
Mute ranges remain half-open. Checked failures leave the original untouched.

## Format and verification boundary

Supported historical framing readers retain their closed `{ value }` vocabulary.
They reject the new `clock` field even for an explicit default, zero offset,
null or escaped field name. Historical projection accepts only `OwnerOutput`.
Captured Hold framing contains static poses, and frozen audio contexts contain
no live Framing recipe, so neither needs a new grammar.

Unused development databases 39 through 46 refuse without writes or migration.
The existing frozen adapters for databases 1 through 38 remain supported.

Verification covers exact curve/endpoint preservation, checked failure,
current and historical serialization, picture identity through Partition/Split,
Camera draft retention, and independent PCM/gain comparisons. See
[qualification](qualification/source-effects-2026-10-01.md) for actual results.
Atomic In/Out/Slip/Roll, exact linked editorial windows, marks, sound routing,
handle clamping and native Trim previews remain required work.
