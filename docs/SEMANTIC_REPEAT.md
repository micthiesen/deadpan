# Semantic edit repeat

Plain `.` repeats the last committed picture cut, Repeat wrap, play-count setter,
Group or Ungroup against a new current target.
`x`, counted `x`, `:delete-frames Nf`, `d` with a motion, `dd`, Visual `d` and
`:delete` supply cut intent. Repeating a cut is one atomic edit with one Undo entry
and a newly captured editable copy.

`rr`, `r` with a motion, Visual `r` and `:wrap-repeat` supply Repeat intent.
They retain their selector and total plays, preserve registers and leave an
editable structural Repeat. `:repeat N` on an existing Repeat supplies a count
setter; dot applies that total to the newly selected direct-child Repeat. Any
Visual selection refuses, and another beat kind cannot become a wrap. The count
setter preserves pending register choices. See [Repeat selections](REPEAT_SELECTION.md).

[Group](GROUP_EDITING.md) retains its exact name and selector, with a current
Visual range taking precedence. A saved Visual selector needs a new nonempty
range. Ungroup resolves a newly selected neutral Sequence and refuses every
Visual state. Both operations preserve the copy bank and pending register choice.

This covers picture cuts, Repeat wraps/count setters and Group/Ungroup for DP-06.
[Semantic macros](SEMANTIC_MACROS.md) record the effective instruction, so a
recorded dot does not depend on a
later repeat candidate. Other edit kinds and semantic text/role/occurrence
selectors remain required. The resolved
[compound transaction boundary](COMPOUND_TRANSACTIONS.md) is separate.

## Selection and registers

`FrameCut` retains a positive requested count, with strict typed serialization.
Its current-cursor selector resolves anew against the supplied document and
ordinary Sequence owner. It uses the existing absolute Sequence boundary and
range-deletion queries, including endpoint and temporary-node admission.
The result is `[cursor, min(cursor + count, group_end))`, using checked frame
arithmetic. A missing/unsupported owner, cursor outside the group, cursor at
its end, or unsupported partial endpoint refuses without a write. The host
binds resolution to the captured project session and revision.

For example, `7x` near a group's end may remove only two frames. Moving elsewhere
and pressing `.` still requests seven frames. A motion cut such as `d5h` retains
its five-frame backward motion. Beat and group-boundary motions also resolve
again. `dd` targets the newly selected direct child, including an empty child;
it does not infer that child from a shared time boundary.

A current active or finished Visual selection overrides a retained cut or wrap selector.
An explicit empty selection refuses without falling back to the cursor or beat.
A saved Visual cut requires a new Visual selection; old endpoints are never
reused. A successful override becomes the new repeat intent.

Dot uses the shared semantic Apply planner. Its request captures the current
session, revision, bank version, Sequence scope, cursor, selected child and
oriented Visual selection, including absence. The service resolves the effective
instruction against that exact context. Legacy frame and Visual/whole-beat cuts
retain their existing independent cut receipts and validate their intent against
the exact captured range or child. Legacy motion intents and selector repeats
without the full semantic context refuse.

For cuts, the original destination register is retained. An explicit register choice
overrides it for the next repeat, including `""` for the default register.
That successful repeat becomes the last edit with its new destination. A named
cut updates the named and default copies atomically. Failed cut attempts preserve
the previous saved intent and every register slot while consuming the attempted
one-shot register choice. Keys owned by a text field or another mode do not
start a cut attempt.
Repeat wraps/count setters and Group/Ungroup preserve one-shot register intent,
including when they refuse.

## Durable success and asynchronous feedback

The project service owns one last semantic edit and one versioned snapshot.
Before publishing, it reads the actual saved head through a bounded single-row
query, without decoding a document. The visible workspace may still hold an
older revision after a failed refresh.

A supported direct cut, explicit Repeat wrap, semantic count setter, Group or
Ungroup records an exact before/after revision proof immediately after commit,
before optional refresh. This includes a single Apply during
recording. Named macro Run transitions remain unproved and clear the candidate
when they change the document. Direct mark-only saves and Undo/Redo record
preservation proofs. An observed head change applies a proof only when its
session, project and both revisions match exactly. Any other head change clears
the candidate. This covers unsupported native edits, prepared insertions and
headless edits, including saved results whose workspace refresh fails.

The proof is a single consumed value, not a revision-indexed history. A later
mark or history change cannot conceal an unobserved authored transition. An
idempotent cut reply does not create another proof. Failed requests, previews,
navigation, copies and operations that leave the head unchanged preserve the
candidate. Undoing an unsupported edit cannot restore an older candidate.
Direct headless marks and history have the same preservation behavior; unknown
Compound commands clear the candidate even if their leaves happen to be marks.

Head-read failure makes repeat unavailable and discards the candidate. Later
recovery cannot revive it. This observation failure does not retract or alter
an already saved edit's receipt. Version exhaustion makes repeat unavailable
without wrapping identities. Close, project replacement and reopen clear this
session-local state; a failed Open preserves the existing session.

The UI admits only newer snapshots for its current project session. A repeat
request captures the snapshot version along with the current cursor, scope and
revision. The service observes the saved head again and rejects stale versions,
changed operations or an unrefreshed workspace before writing. A superseded UI
copy confirmation cannot erase a successful cut's semantic intent.

## Native interaction

The configurable binding ID is `edit.repeat-last`, default `.`. The footer
shows the effective target, retained count/direction, or required Visual range.
It identifies an empty current range as unavailable. Searchable help
describes the current scope. Counts before `.` and held activation are refused.
Native text, controls, IME and pending prefixes retain input ownership.

Repeating requires Your edit with an eligible video pane. Original, Sources,
Placed sounds and temporary previews refuse. Each invocation captures its new
context once. An attempted cut dot also consumes a one-shot register override when
pending macro work or a full recording refuses it, without changing queued work.
The keyboard
compatibility audit and rendered `dot-repeat` replay exercise the production
paths; physical layout and OS IME qualification remain separate obligations.

No project schema changes are needed. SQLite remains at 55 and core documents
at 43; history uses resolved cut, RepeatSelection and Compound commands.
See the [frame-cut qualification](qualification/semantic-repeat-2026-10-02.md)
for the original increment and the
[selector-repeat qualification](qualification/selector-repeat-2026-10-03.md)
for service tests, keyboard replay, native feedback and remaining limits.
