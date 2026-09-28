# Native gain editor integration

This is the implementation design for the next native gain increment, not a
claim that its controls or temporary audition exist. The normative behavior is
in specification sections 6, 9.4, 9.5 and 10. Use the
[gain board](design/boards/clip-gain-board-v2.png) and
[authored gain contract](AUDIO_GAIN.md) together.

## Target and command capture

Normal `+` / `-` changes gain by 3 dB, multiplied by an accepted count. Resolve
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
owner. Status and inspector show the actual scope and previous/resulting dB or
mute state. UI code does not directly mutate a document or write SQLite.

## Proposed content and admission

A draft retains the immutable entry workspace, captured target, unique draft
token, monotonically increasing change number and complete proposed treatment.
Every preparation result and failure carries the same token/change/session/base
revision tuple. Only its current tuple may replace the prepared draft. Keep one
replaceable pending preparation and bounded results rather than queueing every
slider movement.

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

The current `Sources::matches` uses document Arc identity and receipt/Original
equality. Retain those checks when adding proposal identity. Do not weaken media
admission merely to reuse canonical PCM across Before/Draft switches. Any later
decoded-source reuse must be separate from proposed processing caches.

## Same-window audition

Capture an immutable half-open Sequence audition window around the selected
owner with the normal 500 ms lead-in and 750 ms follow-through, clamped to the
edit. Gain does not alter the Sequence duration. Before uses the captured
committed document; Draft uses the latest successfully prepared proposal. The
whole Sequence mix is needed, including applicable group gain, sounds, Hold
allowances and the limiter. Catalog Sound audition is not an event-mix preview.

Before/Draft switches retain that same window and its heard content position.
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

Keep the viewer dominant and show an unmistakable unsaved Draft label. The
inspector carries owner name, owner-local units, trim, true mute, existing
envelope summary, Before/Draft, Apply and Cancel. Show keycaps where actions
live. Commit once with Enter when the sheet owns that action; Escape cancels.
Focused text fields, native buttons and IME retain their own event ownership.
Opening and closing an unchanged draft creates no history.

Envelope keys remain exact owner-output positions. A graph must use those
coordinates and explicit In/Out boundaries; it cannot stretch hidden keys into
the current visible owner duration or shift fixed Sequence keys on insertion.
Keyboard range/keyframe editing is required alongside pointer manipulation.
Draw only measured waveforms. A decorative waveform, editable-looking graph or
unimplemented control is not an acceptable intermediate feature.

## Verification boundary

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
for the painted board comparison, focus/IME, accessibility and real listening
that a headless test cannot establish. Keep open limits explicit until measured.
