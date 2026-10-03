# Semantic frame-cut repeat

Plain `.` repeats the last committed frame cut at the current Edit cursor.
`x`, counted `x`, and `:delete-frames Nf` supply this intent. For example,
`7x` near a group's end may remove only two frames. Moving elsewhere and
pressing `.` still requests seven frames. Each repeat is one ordinary atomic
cut with one Undo entry and a newly captured editable copy.

This is the first implemented semantic edit for DP-06. Other edit kinds,
semantic text/range selectors, macro recording, Macro register contents and
bounded macro call expansion remain required. The resolved
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

The native request retains the count alongside its exact captured range. The
project service resolves it again before committing and rejects any mismatch.
Initial frame cuts and dot-repeat share that path; screen coordinates and
previous timestamps are never recorded as repeat intent.

The original destination register is retained. An explicit register choice
overrides it for the next repeat, including `""` for the default register.
That successful repeat becomes the last edit with its new destination. A named
cut updates the named and default copies atomically. Failed cut attempts preserve
the previous saved intent and every register slot while consuming the attempted
one-shot register choice. Keys owned by a text field or another mode do not
start a cut attempt.

## Durable success and asynchronous feedback

The project service owns one last semantic edit and one versioned snapshot.
Before publishing, it reads the actual saved head through a bounded single-row
query, without decoding a document. The visible workspace may still hold an
older revision after a failed refresh.

A supported cut records an exact before/after revision proof immediately after
commit, before optional refresh. Direct mark-only saves and Undo/Redo record
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
shows the retained requested length when repeat is available. Searchable help
describes the current scope. Counts before `.` and held activation are refused.
Native text, controls, IME and pending prefixes retain input ownership.

Repeating requires Your edit with an eligible video pane and no active or
retained Visual range. Original, Sources, Placed sounds and temporary previews
refuse. Each invocation captures its new scope and cursor once. The keyboard
compatibility audit and rendered `dot-repeat` replay exercise the production
paths; physical layout and OS IME qualification remain separate obligations.

No project schema changes are needed. SQLite remains at 54 and core documents
at 43; the resolved cut history uses the existing typed deletion command.
See [qualification and limits](qualification/semantic-repeat-2026-10-02.md)
for executed tests, rendered replay and source identities.
