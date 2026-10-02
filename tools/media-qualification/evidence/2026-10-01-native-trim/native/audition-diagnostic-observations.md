# Debug native Trim audition diagnosis

All three instances used the real native device through an ordinary debug build
and the retained disposable Trim QA package. The first two instances visibly
kept a Pause caption after Space. Harmless focus changes still showed Pause;
Escape and Cmd+Q worked, and the sampled main thread was idle in Cocoa rather
than blocked. No authored save occurred.

A temporary diagnostic build logged Space routing and transport transitions:

```
Loop: heading focused, background=true, routed=[Loop], false -> true
Space: heading focused, background=true, routed=[Play], false -> true
```

The false state before the second action proved that native playback had stopped
before that action. A following native focus action showed the Audition caption
and accessibility text `Audition stopped: audio output stopped: Starved`.
The native screenshot showed only geometry/handle text in the feedback clip;
the failure existed below that visible region. No key-routing or double-toggle
failure was established. The temporary logging was removed after diagnosis.

The instrumented run is `native-trim-input-diagnostic.json`; the preceding runs
are `native-trim-native-final.json` and `native-trim-native-pause-diagnostic.json`.
All exited 0 after Cmd+Q. The diagnostic snapshot and all-table comparison prove
that cancellation left all 20 SQLite tables unchanged. No Deadpan process or
writer lock remains after that comparison.

Root added separate keyboard/pointer pause assertions and an injected matched
Starved delivery failure after feedback scrolling. The visibility assertion
failed in visual 7 and passed in visual 8 after prioritizing errors and revealing
a changed error. Transport pointer actions also explicitly request repaint.
The injected witness establishes UI failure handling, not real device reliability.
The native optimized build and actual device check are recorded separately.
