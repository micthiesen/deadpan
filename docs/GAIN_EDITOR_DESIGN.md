# Native gain editor integration

The native app implements the captured commands, exact recipe editor and
temporary audition described here. The focused Metal replay and app/harness
tests pass; [native-gain qualification](qualification/native-gain-2026-09-28.md)
records corrected visual runs, native CUA checks, retained failures and separate
performance and listening limits. The normative behavior is in specification
sections 6, 9.4, 9.5 and 10. Use the
[gain board](design/boards/clip-gain-board-v2.png) and
[authored gain contract](AUDIO_GAIN.md) together.

## Target and command capture

Normal `+` / `-` changes gain by 3 dB, multiplied by an accepted count. `:gain`
opens the draft, `:gain -3.125` sets absolute trim and `:gain-mute` toggles the
explicit whole-beat mute. The inspector exposes the same actions. Resolve
Placed sounds first when that pane owns focus; an empty selection rejects there
and must not fall through to a retained picture beat. Otherwise resolve the
selected direct child of the active ordinary Sequence in Your edit. Original
and catalog Sound focus do not authorize changing a retained edit selection.
Camera keeps its existing scale keys. Use logical keys, preserve text editing,
IME composition and native focused-button activation, and check count overflow.

Parameter entry captures a result, including absence of a valid target. Retain
session, project, base revision, Sequence scope, owner, Edit cursor and complete
entry recipe before opening the command field. Late service updates cannot
create a target or substitute another owner. Recheck this capture before both
preview and commit. Trim adjustment preserves existing envelopes, mute ranges
and stage order. Direct trim entry sets an absolute dB value; the Normal keys
apply relative 3 dB steps. True mute changes its explicit boolean, not a -96 dB sentinel.
An event currently has no explicit mute field; do not invent that operation or
redirect its mute key to a beat.

The writer service resolves the captured direct-child scope and cursor through
the existing `ProjectEdit` path. Gain edits preserve the Edit cursor and selected
owner. The inspector shows the current trim and envelope/mute-range counts;
the draft names its owner and marks its unsaved state. UI code does not directly
mutate a document or write SQLite.

## Proposed content and admission

A draft retains the immutable entry workspace, captured target, unique draft
token, monotonically increasing change number and complete proposed treatment.
Every preparation result and failure carries the same token/change/session/base
revision tuple. Only its current tuple may replace the prepared draft. Keep one
pending preparation and coalesce subsequent draft changes into the latest
complete recipe. A superseded reply releases its pending slot but cannot replace
the prepared current change.

The project service creates an ordinary `CommandRequest` with a fresh proposed
revision ID, calls `ProjectStore::preview`, and applies the returned forward
patch to the captured base document. This is a validated proposed document,
not a committed store revision. Never publish it as the authoritative Workspace,
write history for each adjustment, or relabel a mutated document with the base
revision. The proposed ID is never reused. Apply later submits the captured
command through normal commit with a fresh committed revision after rechecking
the base; Escape simply discards the proposal.
Existing generation-relevance gates still apply at commit; proposals do not
authorize invented context observations or bypass a missing host resolver.

Media evidence remains anchored in the captured committed revision. Resolve
receipts and Original ownership from that revision, prove that the gain-only
proposal preserves its asset contracts, and pass those capabilities alongside
the proposed document. Do not query SQLite for a proposed revision that was
never stored, or invent source qualification for it. Playback still verifies
actual Original bytes on admission and retains the existing muted-cache checks.

Keep committed versus proposed content explicit across playback Snapshot,
Update, Run and Resume. Proposed identity includes its base revision and draft
token/change number as well as the proposed document identity. Compare it when
accepting success, failure, cached preparation, pause/resume and device updates.
A matching base revision, selection or duration alone is insufficient.

`Sources::matches` compares proposal identity alongside document Arc identity
and receipt/Original equality. Do not weaken media admission merely to reuse
canonical PCM across Before/Draft switches. Any later decoded-source reuse must
be separate from proposed processing caches.

## Same-window audition

Capture an immutable half-open Sequence audition window around the selected
owner with the normal 500 ms lead-in and 750 ms follow-through, clamped to the
edit. Gain does not alter the Sequence duration. Before uses the captured
committed document; Draft uses the latest successfully prepared proposal. The
whole Sequence mix is needed, including applicable group gain, sounds, Hold
allowances and the limiter. Catalog Sound audition is not an event-mix preview.

Before/Draft switches retain that same window and its heard content position.
Reselecting the active tab preserves its running generation and exact position.
Read `Run::content_sample()` from admitted device delivery updates, stop/revoke
the previous generation, then begin the chosen content at that exact sample.
Do not use producer progress, rounded picture frames or the monotonically
increasing loop-delivery coordinate as the content position. Explicit Restart
starts at the window beginning. A paused comparison remains paused.

While a newer proposal prepares, retain the already accepted picture and clearly
identify which audio proposal, if any, is playing. Never label old audio as the
new Draft. A failed preparation leaves the editable proposal and error visible;
it does not silently resume Before or another draft. Device faults and sleep
revoke resume as in ordinary playback.

Draft audition may advance its own viewer clock but must not retarget the
captured owner or run the ordinary stopped-Sequence selection-follow behavior.
Track the entry cursors, view, scope and focus separately from draft playback.
Cancel stops draft work and restores entry UI state only if that project session
and base are still current. A session/revision change invalidates the draft;
never restore an old project's state into a newly opened one.

## Interaction and visual structure

The inspector shows trim, true mute and the existing envelope/mute-range counts.
The bottom draft panel names the owner, marks `UNSAVED DRAFT` and keeps its
heading and Before/Draft, audition, Restart, Apply and Cancel controls outside
the scrolling fields. The surrounding workspace is disabled while the draft is
open, with the retained picture painted at full opacity. Tab and Shift+Tab use
native control traversal within the draft and scroll newly focused controls
fully into view. Cancel wraps forward to the heading; the heading wraps backward
to Cancel. Enter applies and Space auditions when the heading owns focus;
focused buttons retain native activation. Text
fields keep typing and arrows. Popup/dialog input and IME batches defer draft
shortcuts; Escape otherwise cancels. Unchanged Apply or Cancel creates no history.

The pure `GainEdit` model preserves the complete recipe, including configured
unity, unrelated envelopes and cubic controls. Each successful row action
validates a replacement before changing the draft. `Set trim`, `Update range`,
`Update key` and mute-range actions apply buffered native text explicitly.
Pending or invalid fields stay visible and block project Apply and new audition;
Pause remains available. Reset fields discards buffered text without reverting
already accepted draft edits. No field edit directly commits a document.

Trim and values accept exact thousandths of a dB from -96 to 24. Local frames
accept bounded nonnegative integers, decimals or integer ratios without authored
floats. Envelope selection, add/remove, previous/next key, insertion and interior
key removal accompany exact time/value and Step, Linear, Smoothstep or Cubic
controls. The initial key is the range start; every later key ends its incoming
segment. Both cubic value controls remain explicit. Changing a range moves its
boundary keys and rejects a range that would drop an interior key. Separate
half-open mute ranges have add/update/remove controls.

The graph plots the selected envelope's actual contribution on the current
owner-output frame axis. Visible key buttons select exact keys; hidden keys
remain accessible through the selector and exact fields. Floating-point values
are used only for painting. Duration shrink does not normalize keys into the
visible range. Pointer point dragging remains open. The separate
[measured beat overview](WAVEFORMS.md) uses actual canonical PCM and shares the
horizontal owner-frame geometry; amplitude and editable dB keep separate scales.
The gain graph does not draw a decorative waveform. Reviewed captures show the full
graph and fixed actions at 960×640 and 1280×820, with a painted viewer measuring
145 and 270.1875 points respectively.

## Verification boundary

The final `corrected-visual-gain` run passes 266 gain checks plus the Kestrel
check covering 5,456 routing cases and 62 reservations. On the same final source,
all 298 app/harness tests and 263 base-app tests pass in their respective
configurations, as does strict workspace/all-target lint in both configurations.
The replay exercises captured commands, buffered exact trim/envelope/mute fields,
coalesced writer proposals, simulated Before/Draft delivery, stale updates,
Apply/undo, native field/IME routing, pending-text Pause and unchanged Cancel
picture continuity. Four full populated Tab/Shift+Tab circuits at the minimum
and default viewports check 136 focused controls' actual paint and complete hit
clips without wheel assistance. Captures were compared with the gain board.
Delivery injection does not prepare PCM or open a device. The earlier full
visual run retains its 1,123 checks and two failures. Corrected follow-ups pass
81 workspace, 103 room-tone, 266 gain and 16 retime checks, each with its own
passing Kestrel check. They do not relabel the earlier failed run.

Native CUA exercised `:gain`, both boundary wraps, -3.125 dB trim, keyboard
envelope insertion, focus-driven scrolling to the key value, a 1.5 dB key update
and literal `dd + y` text. Escape restored the baseline; complete project dumps
before and after were byte-identical. CUA keyboard injection does not certify
physical keyboard layouts, OS IME delivery, VoiceOver or listening. Release
performance verification is separate; see the qualification record for its
status, final source identities and retained evidence.

Use unit tests for routing precedence, absent/stale capture, count overflow,
recipe preservation, true mute, draft state transitions and delivered-position
comparison. Integration tests must prove writer preview leaves SQLite/history
unchanged, stale Apply rejects, Apply writes once, undo restores the exact recipe,
and two proposals from one base produce their own canonical decoded PCM.

Use the existing playback engine's fake device with real qualified media for
cache separation, delayed success/failure, pause/resume, loops and generation
revocation. Retain the real-media permit through worker exit. Replay production
keys, pointer, text and focus through the UI harness for command capture,
Before/Draft, Enter/Escape and restored selection. Reserve native GUI testing
for OS focus/IME delivery, accessibility, physical presentation and real listening
that the painted replay cannot establish. Point dragging, waveform editing,
long-source response measurements and encoded export remain open. Keep those
limits explicit until implemented and measured.
