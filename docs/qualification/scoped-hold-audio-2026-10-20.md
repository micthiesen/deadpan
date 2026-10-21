# One-play Hold sound, 2026-10-20

Silence and prepared room tone now support semantic recording, named calls and
dot-repeat with explicit This play authority at every Repeat level. Each
invocation resolves its current Hold and isolates that occurrence before
changing its sound. This advances DP-06 and DP-07; neither is complete.

Base `7b03bba1`, Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1 and the
qualified FFmpeg 8.0.3 prefix. No new primitive, schema or keybinding.

## Exact sound intent and context

Native room-tone recording saves only the exact prepared Original sample
span when Apply succeeds. Editing the sample fields, preparing, auditioning
and cancelling do not append instructions. The separate `SetRoomTone`
instruction resolves its Original register against the invocation's current
register snapshot. Neither saved form retains a concrete Hold or play address.

The planner requires complete single-play authority, exact isolation identity
pools and a matching presentation path. Later staged instructions follow the
actual isolated Hold. A value edit preserves the absolute Edit cursor,
ordinary Sequence selection, picture provider and duration. No-op edits need
no identities. Unsupported or stale requests publish nothing. Existing Retime
and external-sound admission rules remain those for a sound value edit.

Only permissions issued by the selected silent Hold retire when its policy
changes. Other occurrences and unrelated permissions remain intact. Setting
Silence again does not recreate retired permissions. Tail remains unsupported
in semantic programs.

The native commands and pointer controls share recording and pending-action
guards. Apply closes the preview modal before capturing the restored target;
the normal capture path deliberately refuses an open modal. Independent
review caught that ordering error before execution, and the implementation
was corrected. Navigation revokes delayed continuation ownership without
discarding an already committed sound edit. Existing nonrecording ordinary,
All, mixed and Visual paths retain their captured-target workflow.

## Verification

Five focused core tests pass. They cover first and last nested plays, already
owned branches, no-op and invalid authority, exact identity pools, the
Original-register snapshot and exact copied sample span, staged named calls,
later atomic refusal, selective permission retirement and a supported Retime
value edit. They verify inverse patches and preserved ordinary context.

The canonical PCM regression passes with a deterministic 44.1 kHz WAV provider
at 30000/1001 fps. It compares the selected first/last nested Hold against an
independent room-tone reference at the same absolute fractional origin. Source
and gap PCM outside that play remain identical. Partly bound owned branches,
finite source support, exact Silence suppression and alternate read chunks are
covered. This is a canonical sample test, not acoustic listening evidence.

The real qualified store regression passes. A valid Silence step and its
nested isolation still preview when the later room-tone receipt or Original
binding is damaged. The complete two-step preview and commit then fail source
admission without publishing document, register, history or transaction-step
changes. Intact admission, exact preview/commit equality, reopen, Undo and
Redo also pass.

The first store test incorrectly expected a fractional-sample span to reach
store admission. Document validation requires the qualified asset's exact
`1 / sample_rate` clock, so that request fails during planning. The corrected
test checks that early boundary explicitly and uses an admissible exact sample
span plus a damaged qualification for the later store failure. The failed log
and initial source hashes remain retained. Production validation was unchanged.

The first integrated lint build also caught an incomplete native feedback
match: Reverse and Tone were omitted from `HoldAudio`. The match now explicitly
returns no semantic feedback for those policies and Tail, all of which program
validation already rejects. No policy admission was broadened.

Formatting and strict normal/UI Clippy pass. The affected normal selection
passes all 458 tests (22.649 seconds; 3,304 skipped). The separate UI-feature
app/CLI selection passes all 220 assertions (17.819 seconds; 1,792 skipped),
with one output-handle leak diagnostic from
`authenticated_macro_failures_leave_document_bank_and_history_exactly_unchanged`.
That test passed cleanly in the normal selection and its isolated rerun passes
cleanly in 0.358 seconds. Read-only investigation found silent Hold fixtures,
synchronous authenticated socket requests and explicitly awaited shutdown, with
no requested decoding, rendering, generation or preparation subprocess. No
retained PID or descriptor identifies the output-handle owner; the cause
remains unproven. No timeout or assertion changed.

The first release replay reached the new cancellation flow, then stopped when
it tried to click the enabled RoomTone button below the compact inspector's
paint clip. The replay now scrolls the actual nested inspector with wheel
events, checks complete painted visibility and verifies unchanged history,
cursors and captured scope before clicking. Ordinary targets reuse the existing
inspector helper. No production availability was changed. The failed report and
binary/source hashes remain retained.

The second replay passed the new audio flows but exposed retained nested
inspector scroll in a later existing pause visibility check. Cleanup now resets
both distinct inspector scroll areas through wheel input, checks the existing
Insert pause label at 960×640 and restores the original selection, clocks and
window size. The older pause assertion was not weakened. That failed report
and its source/binary identities are retained too.

## Final release replay

The final visual and separate quiet performance runs each pass all 608
`scoped-plays` checks plus the production Kestrel routing audit. The new flow
uses a private portable copy of the qualified Original and production commands,
sample fields, pointer controls and real service receipts. It covers exact
prepared samples, cancellation, saved calls, current-play dot, Silence, ordinary
Visual and All/mixed direct paths, withheld completions and full cleanup.
No ignored layout retry or same-cause repeating retry was accepted.

Both final 960×640 captures were inspected. Sample fields, full Apply/Cancel
controls, nested scope, exact room-tone range and dot feedback remain legible.
The inspector needs scrolling to reach its lower sound controls; the replay
records that extra user step. Existing layout retries are retained as warnings.

Performance measures 890 completed input-to-state samples: p50 0.473 ms,
p95 0.978 ms, p99 1.420 ms and maximum 1.687 ms. All 263 requested picture
completions finish without a timeout or failure: p50 2.016 ms, p95 11.888 ms,
p99 17.374 ms and maximum 94.388 ms. This run does not close DP-24 or establish
physical display/device latency. The maximum remains in the raw evidence.

The final app SHA-256 and unchanged qualified helper hashes are recorded in
`binaries.json`; every modified Rust file has a retained source hash. The CLI
was also built in release, but no new encoded export path is claimed here.
Canonical PCM and qualified source admission cover the changed sound semantics;
existing export qualification is not represented as a new emitted-file test.

Evidence is in
[`tools/media-qualification/evidence/2026-10-20-scoped-hold-audio`](../../tools/media-qualification/evidence/2026-10-20-scoped-hold-audio).
It retains commands, failures, passing logs, source and binary identities,
independent reviews, both final reports, two inspected captures and checksums.
Physical keyboard delivery, VoiceOver speech and acoustic judgment remain on
the [owner list](../REQUIREMENTS.md#to-verify-owner) under specification §29.1.
