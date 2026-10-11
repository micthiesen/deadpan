# Idle audio preparation, 2026-10-11

Deadpan now prepares canonical audio ahead of a stopped Original, Edit or
focused catalog Sound cursor. After the picture and cursor settle for 400 ms,
the existing preparation worker examines up to 30 seconds ahead. This can
prepare a future continuous Preserve stage before Play reaches it.

## Ownership and priority

Idle work uses the same qualified sources, StageAudio, limited bus and bounded
RAM/disk caches as playback. It neither opens an output device nor changes
authored state, the picture or any editor/heard clock. The worker retains one
cache owner; a new domain or snapshot drops the old owner before admitting the
replacement. No new thread or job-coordinator dependency is introduced.

Pending requests coalesce. Cursor, revision/domain, draft, current-picture and
render changes cancel obsolete work. Waveform requests take priority. Matching
Play lets the current canonical read finish, then uses that cache with fresh
admission checks. Other playback cancels the read. The playback request retains
the promoted cancellation token, so Stop or supersession still cancels it
before activation. Stale idle results cannot replace the current ticket.

The native host consults the existing battery, Low Power Mode and thermal
policy before admission and between reads after two seconds. Policy checks
run on the worker. Direct playback does not invoke the gate, but can wait for
an already running bounded check or canonical read. The horizon has one
60-second processing budget after plan admission; source opening retains its
own limits. Cancellation and deadlines are cooperative. A deferred result is
visible and waits for a new cursor/context rather than retrying continuously.

## Native feedback

The transport and focused Sound card show queued work, actual worker activity
and elapsed time, completion, deferral or failure. Status belongs to the
captured workspace, domain and stopped cursor. Navigation rejects an old
label immediately. A long Sound status owns the full card width.

“Audio cache checked ahead” describes the examined horizon. It does not promise
that every examined tile remains resident: existing LRU bounds still apply.
The tooltip explains reuse and limits. It is never a readiness token and never
changes the meaning of Play, Pause, heard time or export.

## Verification

Apple M5 Max, 18 physical cores, 128 GiB; macOS 26.5.2 (25F84), Rust 1.97.1,
with the qualified FFmpeg 8.0.3/Opus 1.6.1/dav1d 1.5.4 prefix.

The full playback suite passes 80 tests. Three subsequent test-only additions
extend the focused warming suite to ten passing cases. They cover the bounded
output horizon, absent device activation, latest-cursor coalescing, matching
Play cache reuse, foreign-session cancellation, failures, policy deferral,
waveform priority and Stop after promotion. A real decoded-source comparison
matches canonical PCM exactly. A second fixture puts a Preserve stage after
five seconds of silence, warms it from the start, then seeks into that stage
without replacing the prepared owner; its delivered samples match the separate
canonical reader exactly. Device delivery in these tests is simulated.

All 21 final native transport tests, strict all-target workspace Clippy and
formatting pass. The idle replay passes all 26 checks. Offscreen Metal replays
use real PCM
preparation and real source/Sound registration. They check Original, Edit and
Sound completion, unchanged picture/clocks/selection/revision, stale-status
rejection and actual compact/default status paint. Control replays pass another
395 checks across the existing Original and Sound interaction paths, with optional warming suppressed
to preserve their declared cache states. These are offscreen renders, not
live-window screenshots or real-device timing measurements.

## Failures retained

Initial compilation exposed Arc borrowing and replay helper scope/type mistakes.
The expanded PCM test first hit its generic ten-second wait while the real
worker was still progressing; its wait now covers the worker's 60-second
budget. No DSP deadline or production failure policy was relaxed.

The first replay rejected a fixture path containing parent traversal. The
fixture now uses a canonical absolute path, as the real picker does. The next
replay checked the sound catalog's selectable row as a Button and failed even
though the row was fully painted. The new scenario now asks the helper for
paint visibility appropriate to that row, then clicks it through the normal
input path. Both failures and their images remain in the evidence. Neither
required a production change.

An initial control-replay command repeated `--scenario`; the CLI refused it
before executing scenarios. The corrected command uses its comma-separated
list. Both command receipts are retained.

The debug linker retains its large `__eh_frame` warning. Replay findings also
retain input-driven layout retries and explicit unmeasured OS/device scope.

## Remaining work

Preparation while audio is actively playing, persistent cross-restart PCM
reuse and the remaining cache lifecycle stay open. This stopped-cursor horizon
does not prevent every later cold-stage underrun, guarantee an immediate Play
start, or qualify acoustic synchronization, physical power changes or hardware
output. DP-09 and DP-16 remain partial.

Commands, failures, source and executed-binary pins, complete replay reports,
review notes and selected renders are retained in
[`2026-10-11-audio-warming`](../../tools/audio-qualification/evidence/2026-10-11-audio-warming/).
