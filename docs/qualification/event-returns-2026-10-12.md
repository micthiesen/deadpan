# Independent sound returns, 2026-10-12

Root `SoundEvent` and attached `BeatSound` recipes now carry independent
reverb/delay `wet_sends`. Native `:sound-wet-send reverb|delay -6dB|off` edits
the captured placed sound through the typed project writer and durable Undo.
The inspector exposes both levels. Attached events use `SetBeatSound`;
this increment does not add a native attached-sound selector.

## Timing and mix ownership

Finite effects read complete, checked input on the sound's retained sample
clock after source/time/pitch mapping and authored input edges. Constant event
gain applies to dry and wet together, equivalent to gain before these linear
kernels. Saved delivery routes then transport the processed samples. Inserted
route gaps do not advance the retained effect clock, and cuts/copies do not
throw away its previous input history. Current Hold permissions and output
edge fades apply to both dry and returned sound before the group bus.

The sound keeps its selected audible duration. Reverb/delay cannot extend the
picture or the event allocation; a short selection may end before a delay echo.
An explicit Tail pause allocates additional audible time. The event's own
allocated-Tail send excludes its continuous return, while parent processing and
sends receive the combined sound. All existing structural scope refusals remain.

Event tiles share native workspaces with owner effects within one immutable
preparation request. Complete histories, temporary outputs, cached tiles and
contribution arrays use the shared residency bound. Combined owner/event node
count, depth, prepared frames, plan work, cancellation and deadline limits apply.
No prepared PCM is published after a refusal. This establishes bounded
preparation, not sustained playback performance or persistent cache reuse.

Core 52 and store 80 carry the new event fields. Frozen owner context remains
12. Older unused development packages fail their version gate without writes,
under the owner's authorized format break.

## Verification

All 3,962 core, plan, store, audio, playback and app tests pass, including
documentation tests. Nine existing opt-in app checks remain ignored. Subsequent
changes affect only replay setup and are rebuilt and replayed separately.
No manual Rust source edits occurred during an active validation sequence. Strict workspace/
all-target Clippy with `ui-harness` and optimized app/CLI builds pass.

The focused PCM cases compare both event types and both effects with separately
prepared finite-kernel oracles. They cover parent saturation, parent delay after
an event reverb, input and output edge fades, inserted delivery gaps, current
silent-Hold allowances, cold partitions, and copy/recopy through nested Preserve
stages at NTSC timing. Allocated Tail taps remain unchanged when the same sound
adds a continuous return. Cancellation and resource refusals leave readers reusable.

The release export fixture retains a deterministic stereo WAV with a short
windowed burst and selected silence. Its event starts at sample 16,016 and ends
at 48,016. The burst at 17,016 yields delay echoes at 31,416 and 45,816; the
Original click at 28,781 stays unchanged. Limited preview verifies their levels
and timing before rendering the unchanged 30-frame edit. Full emitted-file
verification passes all 30 H.264 pictures, four AAC blocks and one signal window
with zero measured offset. Maximum block-level difference is 0.029007 dB and
minimum block SNR is 43.479556 dB. The file declares 2,048 samples of priming and
48,048 presented ticks. No production verification tolerance changed.

Five final offscreen Metal replays pass 603 scenario checks: 83 continuous-return,
80 allocated-Tail, 234 sound-placement, 124 audio-treatment and 82 EQ checks,
plus five Kestrel shortcut audits. They exercise captured/absent/stale targets,
saved levels, other gain edits, durable Undo, keyboard catalog selection and
pointer placement. The final 960×640 and 1280×820 captures were inspected against
the stored workspace and sound-placement targets. Both return levels, commands
and policy text are readable after inspector scrolling. The two final release
captures are byte-identical to the inspected debug captures.

Initial fixture attempts failed because a Source had no media and then because
its 48 kHz span disagreed with the qualified 44.1 kHz asset. The corrected fixture
uses a valid muted Original and independently qualified sound input. Production
validation was not weakened. Initial Clippy rejected matching a `String` against
string literals; the parser now matches `verb.as_str()`. Command-reference and
routing regeneration intentionally fail after writing their reviewed updates.

The initial minimum-size replay attempted a pointer click on a catalog row
below the sidebar's visible clip. Placement correctly refused the absent sound
selection. The replay now reaches Sources with Tab, selects the sound with `j`,
and checks that exact selection before placing it. This is a fixture setup
correction; no production behavior or audio assertion changed.
The broader sound-placement replay exposed the same unscrolled pointer setup;
it now uses the existing Original-rail reveal helper before catalog clicks,
retaining pointer coverage and all placement assertions.
Further setup checks found that the shared inspector helper assumed a beat
heading and that catalog entries are selectable rows rather than buttons.
The helper now uses the focused inspector's heading and actual viewport size;
catalog setup requires visible painted text before the existing pointer
selection and placement assertions. Both failed attempts remain in evidence.
The debug harness link also reports `__eh_frame section too large (max 16MB)`
for compact unwind offsets, with possible exception-handling performance cost.
It succeeds; final captures and replay qualification use the optimized app.

The primary agent implemented and reviewed the entire change alone, as requested.
No independent review, live GUI, physical keyboard, device output or listening
check is claimed. Remaining creative event envelopes/treatments, partial attached
scope edits, wider loop/incoming-tail qualification, persistent filtered caches,
mastering and performance keep DP-09/DP-16 and the full product goal open.

[Evidence](../../tools/audio-qualification/evidence/2026-10-12-event-returns)
retains the complete source patch, source/executable hashes, commands, compressed
logs, failures, actual movie/project and inspected captures. Run
`python3 check-evidence.py --source` there while the source still matches;
omit `--source` for historical evidence. Validation ran serially on Apple M5 Max,
18 physical cores, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and qualified
FFmpeg 8.0.3. Broader platform and listening checks remain on the
[owner verification list](../REQUIREMENTS.md#to-verify-owner).
