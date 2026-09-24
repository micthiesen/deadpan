# Sequence audition

Space and **Play edit** audition the current immutable sequence revision. Space
pauses, or cancels preparation. `:monitor 25%` changes the independent monitor
level by keyboard; `:monitor 12.5%` restores the initial value. Starting from the
final boundary restarts at zero.
The button, status and shortcut remain visible beside the picture. Native text,
IME composition, dialogs and help retain their own Space input. Holding Space
does not repeatedly toggle playback. Navigation, edits, command entry, help,
project transitions and quit stop the current generation. Sleep and wake revoke
it through an owned main-thread NSWorkspace observer; playback never resumes
automatically.

Explicit pause selects and reveals the beat under the last admitted audio
position. The exact subframe sample is retained for resume; a seek or authored
edit discards it. Selection stays fixed while playing. Opening an inspector
command pauses without changing its captured target.

This is **pre-master audition**, not the completed preview/export audio contract.
It uses canonical source audio, structural Repeat/Retime/Hold semantics, room
tone, authored bindings and edge fades. The full voice/effects graph and final
oversampled master limiter remain required. Monitor volume is independent of
authored and export gain, defaults to 12.5%, and changes while stopped. PCM beyond
the device's finite ±1 range fails explicitly; the application does not clip or
normalize individual blocks. Original-view playback and sound-event placement
remain open.

## Ownership and bounds

`deadpan-playback::Engine` owns a persistent preparation worker and a separate
responsive device controller. The UI submits an immutable document, qualified
source receipts, original records and the connection-free original import
handle. No playback worker opens SQLite or changes authored state. Plan
compilation, byte verification, decoding, resampling and canonical DSP stay on
the preparation worker. Its source cache contains private verified physical PCM,
with a 1 GiB aggregate limit and at most 16 sources. Warm reuse requires the same
session, document and receipt identities and original records. A different
revision rebuilds the cache.

Existing source-admission and StageAudio limits still apply. In particular,
continuous Preserve input is bounded to 1,048,576 frames, approximately 21.8 s at
48 kHz. Exceeding a preparation limit reports failure; arbitrary chunk resets do
not substitute for continuous history. Cold qualification may read the whole
source, but cancellation and editing remain available.

Prepared PCM uses at most two 8192-frame batches plus the native queue. Each
canonical read contains at most 256 stereo frames. The queue reserves 32 PCM
packet slots and one separate terminal slot so a full valid final prefix can
carry EOS without racing its consumer. A fresh channel-scoped generation is
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
