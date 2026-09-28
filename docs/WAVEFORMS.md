# Measured beat audio

The gain editor's **Beat audio · before effects** overview measures the captured
committed beat independently of its unsaved gain recipe. The
[qualification](qualification/gain-waveform-2026-09-28.md) records scoped PCM,
resource, keyboard and painted-layout checks. The
[interface board](design/boards/gain-waveform-board-v3.png) is a target, not test
evidence.

## Meaning and exact coverage

The measurement uses the canonical `definition_output_pcm_before_effects`
reader. It includes the selected definition's timing, pitch, retained bindings,
room tone and structural silence. It excludes creative edges, gain, mastering,
enclosing occurrence processing and independent placed sounds. Before/Draft
continues to audition the full edited mix. Those are deliberately different
scopes, explained in the waveform heading's help.

The local-zero grid is 48 kHz PointCeil. Descriptor identity includes the exact
project/revision, definition selector, owner, frame rate, duration and grid.
Each leaf holds separate signed minimum and maximum values for left and right.
Finite samples outside ±1 remain unchanged in the data; the fixed-scale display
marks over-range peaks. No rectification, normalization or synthetic waveform
is applied. Complete all-zero bins are measured silence. Unmeasured regions
have a separate hatched appearance and coverage/status text.

Leaf stride is the smallest power of two, at least 256 samples, that limits the
overview to 4,096 leaves. Exact leaf support is `[j * stride, min((j + 1) *
stride, N))`. Only complete bins are published. The final short bin has its
actual endpoint; a partially examined nonterminal bin remains unknown.
Coarser min/max levels contain only fully measured support. The renderer uses
coarse bins for the available width and descends for an unpaired measured tail.
It neither rescans covered prefixes nor asserts arbitrary pixel-column extrema.

At fractional frame rates the PointCeil terminal sample boundary can exceed the
owner's exact frame duration. Only that final painted endpoint is clipped to
the owner end. All preceding positions retain their exact phase. Waveform and
gain plots share their horizontal layout geometry, with distinct amplitude and
dB scales.

Wide layouts place the overview in one row, then pair the editable curve with
its exact range/key fields in the next row. The fields stay beside the curve
while scrolling, rather than beginning beside the waveform. Narrow layouts
stack those controls in the existing editor scroller. Apply, Cancel and
comparison actions remain outside that scroller.

## Work and storage bounds

One request examines at most 5,760,000 output samples, or 120 seconds at 48 kHz.
It uses one cumulative canonical preparation budget and a cooperative 20-second
deadline across all 256-sample reads. Existing plan, stage, dependency and
source-check budgets also apply. A complex definition can reach one of these
limits earlier and return only its valid complete prefix.

This is not a hard 20-second wall-time promise. Cold Original snapshot admission
and decoder opening retain their existing separate cooperative bounds, currently
15 and 30 seconds. Cancellation inside media/DSP work remains cooperative.

One Engine shares an aggregate 1 MiB peak-allocation ledger across the builder,
progress mailbox and retained UI results. A reservation lives until the actual
last allocation owner drops. Capacity and bounded descriptor storage are charged;
Arc clones do not bypass the charge. The builder owns its terminal allocation
from the beginning. Progress copies publish at most four times per second;
insufficient headroom skips a progress copy, preserving the final valid prefix.
This ledger is not a total process-RSS bound.

## Preparation ownership and priority

The existing PCM preparation thread owns analysis. There is one running request,
one replaceable pending request and one latest reply. Playback work has priority
and immediately signals analysis cancellation. The old Sources/StageAudio owner
is dropped before a new playback or analysis owner is admitted. The two modes
cannot retain duplicate 1 GiB source caches or duplicate stage caches. This
switch evicts warm audition preparation, so its cost must be measured rather
than described as free reuse.

The controller explicitly acknowledges that output is quiescent. A play request
closes that acknowledgement under the admission lock. Analysis stays deferred
until the active device has been torn down, including completion of a reported
terminal prefix that is still scheduled to play. Cancelled production and an
empty preparation slot do not prove output is idle. Waveform analysis never
constructs a device or publishes a playback `Update`.

Ordinary `Engine::stop` stops playback. Draft closure explicitly cancels its
waveform ticket. Lifecycle StopHandle interruption, worker failure and shutdown
cancel both kinds of work. Resources remain owned until the worker actually
returns; an immediate cancellation reply does not establish teardown.

## Admission and UI lifecycle

Every new request requires a committed snapshot and freshly admits its captured
source receipts and Original bytes. Within-request canonical cache hits retain
their existing dependency checks. No cross-request waveform cache, disk cache
or reusable admission witness is introduced.

Updates carry the ticket, session, project, committed revision and selected owner.
The gain draft checks every coordinate before accepting a result or an error.
Gain field edits and Before/Draft switches keep the same measured reference.
An analysis error is separate from gain readiness and cannot disable a valid
Apply or Pause. Explicit Retry starts a new request; any retained older data
stays labelled as the prior measurement until replacement. There is no automatic
retry. The Retry button participates in native Tab/Shift+Tab traversal and reveals
itself in the existing editor scroller. Retry returns focus to the draft heading
when its temporary button disappears.

No authored command, schema, history revision or stored waveform artifact changes.
Room-tone selection, timeline-wide waveform navigation, scrubbing, range editing,
cross-request reuse and encoded export remain separate work. Headless delivery
or paint tests do not establish physical-device, acoustic, IME or display-latency
acceptance.
