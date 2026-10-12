# Solo implementation review

The primary agent implemented and reviewed this change. No subagents were used.

Reviewed the complete changed source, including the new shared mixer and recursive
EQ reader, against the existing Original-only reader and nonlinear group mixer.
Checked stage-prefix/suffix boundaries, summation before saturation, exact event
and owner gain placement, nested f64 intermediates, signed tile coordinates,
source dependencies, retained/current sound clocks, explicit permission masks,
resource admission, cached depth checks and reservation/filter restoration.

The first neutral-EQ regression exposed a saved crop-edge change: reading expanded
history for an identity processor changed the source resampling at current
boundaries. Neutral stages now remain in authored intent and execute as identities
on the current input. The regression checks exact sample equality at four crop
positions. Nonneutral EQ separately compares copied NTSC input against complete
saved-history filtering and rejects a source revoked after successful reads.

The Hold regression supplies one allowed root sound beside Original audio and a
forbidden root peer. During the Hold its output matches a separately constructed
history containing only the allowed sound, with saturation before EQ. Shared
saturation order, nested EQ, hidden crops, cold/partitioned reads and bounded
admission are separately checked. The former blanket mixed-EQ refusal test now
checks that an independent root sound bypasses a child's EQ.

Remaining scope: differing audible automation on independently timed shared
inputs still returns a specific unsupported error; arbitrary sound effect sends
and their incoming-tail lifecycle, further retained binding/Repeat/Preserve
combinations, persistent filtered caching and interactive latency qualification
remain open. This change adds no native EQ authoring controls and makes no
listening/device claim. DP-09 and DP-16 remain partial.
