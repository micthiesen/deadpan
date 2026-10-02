# Admitted Edit-window waveforms

Edit-window analysis measures exact absolute root sample ranges from an immutable
committed snapshot or a service-admitted proposed snapshot. It is a playback
preparation helper used by the native [Trim draft](COMBINED_TRIM.md#native-trim).
Native execution evidence and its limits belong to the
[Trim qualification record](qualification/native-trim-2026-10-01.md).

## Native Trim consumer

The native waveform belongs to the selected Before/Proposed inspection, including
the complete proposal identity even while viewing Before. Zero intent uses the
captured committed snapshot explicitly. A changed draft, junction or comparison
cancels the old request; replies must match the current ticket, base, snapshot,
junction and absolute sample window before they can replace the display.

Context starts at the exact 48 kHz boundary `B(c)`, subtracts the requested lead
samples and adds follow samples, clamped to `B(0)..B(duration)` for that snapshot.
Before and Proposed each use their own junction and duration. No rounded frame
duration is accumulated. The bounded display keeps measured signed peaks and
truthful queued, partial, interrupted or unavailable status; unmeasured support
stays absent. State-only progress or interruption retains an admitted prefix,
while unavailable or mismatched results clear it.

Optional audition shares this inspection window and moves only the draft's heard
position. The outgoing/incoming picture pair and both editor cursors stay fixed.
An empty context cannot start audition; an empty or unavailable waveform and an
audition failure do not block a valid picture proposal from Apply. These peaks
do not qualify device delivery, physical listening or full audio processing.

## Measured stage

`EditWaveformStage::AuthoredBusBeforeLimiter` is labeled
`authored_bus_pcm_before_limiter`. It includes implemented time/pitch mapping,
voice edge fades, authored owner/ancestor gain, root sound routes and permissions,
and group mixing. It excludes the common limiter, mastering and monitor gain.
The peaks are not a claim about final heard or mastered output. Signed extrema
are retained without normalization or clipping.

`StageAudio::measure_edit_window` prepares the full authored plan. Its requested
half-open sample range bounds the returned peaks, not source filter support,
retained envelopes, RoomTone or Preserve history. It uses one cumulative
preparation budget and reads at most 256 output samples per block. Unsupported
or unavailable content produces a truthful partial or unavailable result.

## Identity and clocks

`EditWaveformRequest` carries the committed `base`, the committed/proposed
`snapshot`, exact `samples: Range<AudioSample>`, and bounded `WaveformLimits`.
A Before snapshot must share the captured document, receipt map and Original
session. A proposed snapshot must pass its private admission and match the exact
base document and session. The caller supplies the current base; this read-only
engine neither reads the store head nor authorizes a commit.

Replies carry the ticket, session, project, base revision, measured revision,
complete `ContentIdentity`, and requested sample range. A consumer must retain
that complete identity when deciding whether results belong to its current view.
A changed intent should cancel its old ticket immediately.

Captured identity and live media authority are separate. Synchronous request
validation checks identity only. The preparation worker checks the existing
edited-slice revocation flags before preparation and before publishing any peaks,
including progress, partial and empty results. Polling checks those flags again
so a completed DTO queued before its admitting store closes cannot deliver
revoked peaks. Revocation returns `Unavailable`, no peaks and an explicit media
admission error while preserving the captured ticket/content/window identity.
These checks do no I/O and do not revalidate Original files or change committed
and base-only proposal warm-cache policy. A DTO already delivered to its consumer
is display data, not continuing media authority. The consumer still owns session
replacement and cancellation. Revocation checks are cooperative boundaries;
they do not make a later store close atomic with an already completed poll.

The descriptor uses the 48 kHz root `RoundEven` sample grid at the project frame
rate. `bin_samples` and `measured_end` are absolute sample coordinates;
`examined_samples` is a relative count. `bin_project_frames` returns exact grid
coordinates. An arbitrary sample request never acquires a new clock or a
whole-frame enclosure. Negative, reversed and out-of-project ranges reject;
there is no clamping. An empty range is a complete, empty measurement.

## Work, memory and cancellation

`Engine::request_edit_waveform` and `Engine::poll_edit_waveform` share the existing
idle analysis lane with definition waveforms. Both kinds use one active worker,
one replaceable pending job, one reply slot, one ticket namespace and one
`WaveformMemory` ledger. A new request supersedes either kind. Polling the other
kind does not consume a reply. `cancel_waveform(ticket)` applies to both kinds.
Active playback takes priority; analysis never opens an output device and drops
its media/DSP cache before playback preparation is admitted.

Both result types use the same bounded signed-extrema accumulator. UI-held
progress and completed peak buffers remain charged until their final Arc drops.
Terminal publication uses the builder's existing allocation. At most 4,096 leaf
bins and 13 levels are allocated; limits cap sample work and elapsed time.
Unmeasured samples are absent, not zero. An incomplete leaf stays unpublished
unless it reaches the exact requested endpoint. Coarser bins publish only when
their complete support has been measured. A partial result can therefore have
`examined_samples` beyond its published `measured_end`.

The committed definition API remains `definition_output_pcm_before_effects`,
with its original zero-based definition `SignalSample`/`PointCeil` descriptor.
Its semantics are unchanged by the shared aggregation implementation.
