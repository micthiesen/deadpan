# Original, edit and sound audition

Space and **Play Original** / **Play edit** audition the current context. Space
pauses, resumes the exact heard sample, or cancels preparation. `:play` is its
command alias. `:monitor 25%` changes the independent monitor
level by keyboard; `:monitor 12.5%` restores the initial value. Starting from the
final boundary restarts at zero.
The button, status and shortcut remain visible beside the picture. Native text,
IME composition, dialogs and help retain their own Space input. Holding Space
does not repeatedly toggle playback. Navigation, edits, command entry, help,
project transitions and quit stop the current generation. Sleep and wake revoke
it through an owned main-thread NSWorkspace observer; playback never resumes
automatically.

Explicit pause of ordinary edit playback selects and reveals the beat under the
last admitted audio position. Original playback leaves the edit cursor, scope
and selected beat alone. The exact subframe sample is retained for resume; a seek or authored
edit discards it. Selection stays fixed while playing. Opening an inspector
command pauses without changing its captured target.

Entering a Sequence group does not change ordinary Space playback's range.
The absolute heard cursor can leave that
group; the footer then says so. Explicit Space pause and terminal playback
updates return to the nearest containing scope before selecting a beat.
Command entry, help and inspector stops preserve the current scope and target.
Group navigation itself stops playback. See [group navigation](GROUP_NAVIGATION.md).

Shift+Space, **Loop selection**, or `:audition` starts the selected Original
moment or edited beat with 500 ms lead-in and 750 ms follow-through. Context is
clamped to the captured source or full edit, including outside the viewed group.
Use `:audition-context lead=0ms follow=0ms` for an exact half-open selection.
Both arguments are required and may use milliseconds, seconds, clock notation
or project frames. Seconds quantize directly to 48 kHz without an intermediate
project-frame rounding. This app-session setting changes no project revision.
Space pauses and resumes a loop's exact sample and lap; Shift+Space restarts the
current selection when stopped. Playback does not extend an active Visual range.
The selected beat remains fixed during a loop, including while its context plays.
Navigation discards the paused delivery coordinate. A fault never restarts a loop.

This is **limited audition**, with the full preview/export audio contract still open.
It uses canonical source audio, structural Repeat/Retime/Hold semantics, room
tone, authored bindings and edge fades. Persisted
[root sound events](SOUND_EVENTS.md#persisted-root-sounds) enter this same bus
with their own gain and edges, followed by the shared
[finite oversampled limiter](AUDIO_MASTERING.md). The full voice/effects graph,
sends and group mix remain required before this can be a final master.
Monitor volume is independent of
authored and export gain, defaults to 12.5%, and changes while stopped. PCM beyond
the device's finite ±1 range fails explicitly; the application does not clip or
normalize individual blocks. Native sound-event placement and structural sound
editing remain open; Original and catalog audition exclude edit overlays.

[Speed edits](RETIME_EDITING.md) create or adjust the Retime structures already
handled by this canonical path. Command entry stops playback; the committed
revision invalidates old resume and picture identities. Original audition keeps
using the unchanged source regardless of speed stages in the edit.

Original playback derives the full measured A/V stream union from the registered
receipt, including leading and trailing audio. It does not use the edited graph
or truncate the Original to its picture span. An immutable descriptor is prepared
by the project service and cached by asset, receipt and project rate. Its interior
video boundaries use retained PTS minus the common origin; boundary zero and the
final boundary include the complete stream union. Inversion searches those same
rounded sample boundaries, independent of project-frame rate. The Source viewer
holds its first/last picture through audio outside the picture span.
Selection loops use separate measured picture endpoints: with zero context,
selecting the first or final picture does not add audio lead/tail or rounded
project-duration slack. Explicit lead-in/follow-through can include that audio,
bounded by the full Original.

## Sound catalog audition

Selecting an audio-only sound in Sources gives audition its own sample clock.
With Sources focused, `j/k` select sounds, Space plays, pauses or resumes, and
Shift+Space loops the complete sound. The catalog shows the selected sound,
elapsed/total seconds, playback state and visible key hints. Changing sound or
leaving Sources stops audition and discards its resume coordinate. Native text,
composition, dialogs and help continue to own their input.

Sound audition does not retarget or cancel the stopped picture, caption or
geometry. An already pending editor picture request may finish normally; sound
delivery creates no new picture request. It preserves the Original cursor,
edit cursor, selected beat and group. It creates no authored beat or history
transaction. Its qualified `Sound` descriptor is cached by asset, receipt and
project rate; a revision or session change invalidates the captured request.
The preparation worker compiles a temporary audio-only Source view and uses the
same source admission, canonical audio and limited device output as other
audition targets. No blank-picture node is inserted into the project.

The sound endpoint comes from the measured available source span on the 48 kHz
mix clock. Whole-project-frame enclosure must not add silence to the end or loop
seam. Unknown priming stays present unless the receipt has explicit exclusion
evidence. A readable source with an unspecified speaker layout still fails
playback qualification. This listening workflow does not implement sound-event
placement, effects, scoped Hold allowances or mastering.

## Selected-source room-tone audition

`AudioRange` is a qualified audio-only view of an explicit source interval,
including audio from the Original A/V asset. The separate catalog `Sound`
descriptor still rejects video. Construct the range on the project service
worker, checking exact integer source samples, contiguous measured coverage,
receipt and immutable source contract. Playback rechecks those bindings against
its captured revision and opens verified original bytes.

The temporary Source retains the full measured source and its affine phase,
with a separate exact selection starting at local zero. Its playback endpoint
is the selected duration rounded once to the 48 kHz clock, excluding project
frame enclosure slack. Cache identity includes the selected range. Loops reuse
canonical context while delivery coordinates keep increasing. This is raw
source audition through the shared safety-limited preparation, not the Hold's
crossfaded room-tone recipe; committed Sequence audition hears that recipe.

The native sheet applies no implicit lead/follow context. Source playback cannot
move either editor cursor, change beat selection, request a picture or write
history. Field changes, cancellation and stale project context revoke playback
and resume. See [native room tone](ROOM_TONE_AUDIO.md#native-selection-and-audition).

## Ownership and bounds

`deadpan-playback::Engine` owns a persistent preparation worker and a separate
responsive device controller. The UI submits an immutable document, qualified
source receipts, original records and the connection-free original import
handle. No playback worker opens SQLite or changes authored state. Plan
compilation, byte verification, decoding, resampling and canonical DSP stay on
the preparation worker. Its source cache contains private verified physical PCM,
with a 1 GiB aggregate limit, at most 16 resident sources and 1,000,000 indexed
audio frames. Least recently used eviction permits larger catalogs. Cold opens
verify original bytes before eviction, reserve both budgets before decoding,
and publish counts only after successful source preparation. The decoder gets
the exact qualified frame-count allowance. Warm reuse requires the same
session, document and receipt identities and original records. A different
revision rebuilds the cache. Canonical audio caches also require the same
Original/Sequence/Sound target. A temporary validated Source-only view compiles on the
preparation worker, while source admission retains the actual authored snapshot.
This view is never written to SQLite and creates no edit revision or history.

Existing source-admission and StageAudio limits still apply. In particular,
continuous Preserve input is bounded to 1,048,576 frames, approximately 21.8 s at
48 kHz. Exceeding a preparation limit reports failure; arbitrary chunk resets do
not substitute for continuous history. Cold qualification may read the whole
source, but cancellation and editing remain available.

Prepared PCM uses at most two 8192-frame batches plus the native queue. A shared
limited reader retains at most four verified 8192-frame tiles and twelve exact
input bus ranges of at most 8192 frames. Overlapping input is reused without
expanding the requested support. Cold preparation
includes real adjacent project context under one deadline and cumulative source,
stage and plan-work budget. Internal bus reads remain at most 256 frames. Every
tile cache hit rechecks complete transitive source/layout provenance; read and
seek boundaries do not reset gain history. Monitoring gain is applied only after
the canonical limited samples. The queue reserves 32 PCM
packet slots and one separate terminal slot so a full valid final prefix can
carry EOS without racing its consumer. Loop reads end at the content seam and
reuse the same full canonical plan, so reads and laps do not reset DSP context.
One verified lap can be reused within the bounded batch for tiny loops. The
device coordinate increases monotonically while the content coordinate wraps
inside the captured half-open window. A fresh channel-scoped generation is
prefilled before activation. A cloneable stop token revokes its callbacks and
later submissions/activation immediately; stopping an older token cannot mute a
newer generation. Already submitted hardware buffers cannot be retracted, so
revocation is not an acoustic pause acknowledgement.

## Audio clock and pictures

`DeliveryClock` retains at most 64 submitted intervals. The newest callback
usually describes future audio, so a single latest timestamp is insufficient.
Only a covering interval produces a current content sample. Missing coverage
does not extrapolate from producer progress or wall time. EOS and starvation
may contain a future nonempty prefix; their terminal boundary becomes current
only after that prefix's reported playback deadline. Device timestamps estimate
delivery, not sound measured at the speaker.

Foreign generations, discontinuous samples, invalid or contradictory timestamps,
lost reports, route changes, device faults and stalled clocks stop audition.
Sub-sample timestamp overlap of at most one sample is admitted because retained
device traces show this jitter; the newer interval wins. Positive gaps remain
explicit and hold the last picture position. No recovery silently resumes.

The UI admits audio updates by request, session, project, revision and generation,
and inverts exact ties-to-even frame/sample boundaries with integer arithmetic.
It retains the playback domain and window as well as revision and generation.
At a bounded nonloop end the cursor may name excluded Out, but the picture maps
the final included sample, never Out's image.
It schedules one decode/GPU picture at a time and coalesces later desired frames.
Audio continues independently of a slow picture. Picture failure stops audition.
Picture tickets retain the output generation; stop invalidates pending decode
and GPU work while retaining the last actually submitted image and its geometry.
Captions and accessibility labels advance only after successful GPU submission.

The current device adapter requires the default route's existing 48 kHz stereo
float configuration. Automatic device-rate conversion, full route recovery,
acoustic synchronization/listening qualification, long-source preparation,
performance targets and full mastered preview/export equivalence remain open.
Unit and integration tests cover clocks, cancellation, real canonical PCM,
provider identity, queue failures and keyboard/presentation state without a
native window. Physical playback and native interaction evidence is recorded
separately from those tests.
