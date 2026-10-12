# Selected tail sends, 2026-10-12

Beats, groups and independent sounds now have separate reverb/delay send levels
for explicitly allocated hanging-tail pauses. The native commands are
`:tail-send`, `:sound-tail-send` and `:tail ... input=sends`. Sends preserve the
dry mix and picture time. Whole-beat edits support dot and macros; individual
Repeat plays isolate their recipes. Captured targets, durable Undo and reopening
an existing tail retain the chosen input and effect.

The [processing contract](../REVERSE_AND_TAILS.md#selected-sends) defines the
send locations, shared-owner treatment and two-second history window. All
permitted contributions still drive shared saturation and EQ; output selection
cannot change that drive. Shared destination ancestors apply once on the tail
output. Other tail returns are silent during preparation. Core 50, store 78 and
frozen-context 11 retain these recipes; unused older development packages are
refused without writes under the existing owner decision.

## Audio and persistence

All 12 new PCM checks pass. Independent input rendering followed by the fixed
effect is the oracle for selected beats, groups, root sounds and attachments.
Checks cover disabled/wrong sends, exact dry invariance, shared saturation and
EQ order, ancestor gain once, cold/irregular reads, silent-Hold permissions,
no feedback, Repeat gaps, split/bound tail phase, cancellation and memory
refusal before media access. Core tests cover closed and bounded wire values,
recipe preservation, context version gates and reversible routed-sound edits.

The release CLI fixture registers a real stereo WAV, sends it at -6 dB and
inserts a 30-frame delay tail at NTSC Edit frame 18. All 60 pictures are checked.
The Original's click remains dry at sample 28781 but its echo at 43181 is absent;
the selected sound's first and second returns occupy 30416..38613 and
44816..53013, inside the allocation. Public H.264/AAC Render and emitted-file
verification pass: 78 audio blocks and two signal windows with measured offset
zero. Maximum block-level difference is 0.6844499978 dB, minimum block SNR
3.688123722 dB and declared priming 2048 samples. No timing tolerance, event
alignment or packet policy changed.

An initial test incorrectly expected the first return below 0.26. Independent
s16 WAV decoding shows a 0.75 peak, giving 0.3758904252 after the send before
reconstruction overshoot. The corrected fixture requires 0.37..0.39. The
unwanted-echo and silence checks had already passed. Full-suite failures also
exposed three stale version assertions; their corrected tests retain refusal
of future contexts and obsolete stores. Initial failures are preserved.

## Native verification and review

The final release app passes 335 scenario checks: 79 selected-send, 50 existing
Hold-effect, 124 audio-treatment and 82 EQ checks, plus four Kestrel shortcut
audits. Sends survive Undo, dot/macros, one-play isolation and placed-sound gain
edits. Stale or absent captures refuse changes, and a stale tail cannot insert
time into a newer revision.

Image inspection found that an existence assertion admitted clipped inspector
text. Final checks require visible paint after actual scrolling. Captures show
beat sends, selected tail input and placed-sound sends. The tail row elides
the trailing history description at the default width; its selected input
remains readable and its full value stays in the accessible text. The inspector's
retained scroll position can require scrolling, which the replay records. Nine existing
opt-in app tests remain ignored. No new ignore or numerical tolerance was added.

All 3,927 unit, integration and documentation tests pass across audio, core,
planning, playback, storage and the app. Strict workspace all-target Clippy
with `ui-harness`, rustfmt and whitespace checks pass.

The primary agent implemented, reviewed and verified this increment alone.
Review covered the complete substantive diff, constructor defaults, schema
admission, per-route send recipes, shared drive, permissions, cache identity,
resource bounds and native capture rules. This is self-review evidence.

## Evidence and remaining scope

[Evidence](../../tools/audio-qualification/evidence/2026-10-12-tail-sends)
retains full compressed logs, source/executable identities, implementation
patch, inspected PNGs, replay reports, the actual movie, render receipt and
authoritative project package. `python3 check-evidence.py --source` checks
current source; omit `--source` for historical validation.

Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1,
qualified FFmpeg 8.0.3. Cargo sequences and replay runs were serial. No live
GUI, physical audio device, listening or other-hardware claim is made.
Continuous wet returns and their full incoming-tail policy, partial
attached-sound processing-scope edits, broader retained/Preserve coverage,
persistent filtered caching and remaining mastering/performance work stay
open. DP-09 and DP-16 remain partial; the full product goal is active.
