# Place slice macros, 2026-10-18

Place slice Apply can record an existing semantic register paste or replacement
when its displayed proposal resolves to that exact operation. This advances
DP-06 and DP-21. Ranged/one-play framing, further selectors and parser work
remain open.

Base revision `22dd2e088b525225e0d61472b70f5e8dc88aefda`, Apple M5 Max,
macOS 26.5.2 (25F84), Rust 1.97.1,
and the qualified FFmpeg 8.0.3 prefix.

## Exact placement admission

The draft retains the entry macro capture, register name and immutable source.
Apply requires the current session, revision, bank version, ordinary Sequence
scope, parent, selected child, retained cursors and oriented selection to match
that capture. Copy must retain the register's exact source, asset, qualification
and endpoints.

Insertion recognizes the literal before/after child slot of the captured
selected beat, or slot zero in an empty Sequence. Equal-time boundaries do not
collapse empty siblings into one slot. Replacement recognizes the exact entry
time range or Object identity. Time ranges must be nonempty; zero-time
structural Objects remain replaceable with their proper ownership.

Move, refined source endpoints, interior/nonadjacent insertion and insertion
with an Object selection refuse explicitly, retaining the preview and recording
without writes. The footer measures and paints the current restriction or
explains eligible Apply. Restoring an eligible proposal permits Apply.

Successful Apply restores entry context, abandons the prepared proposal and
submits the one semantic transaction. It never commits the preview first.
Only the successful semantic receipt appends the instruction. Replay resolves
the current selected child or oriented selection and uses the named register;
the old proposal destination and newly allocated IDs are not program arguments.
Ordinary unrecorded placement retains its existing commit path.

## Tests and independent review

Three pure admission tests distinguish literal slots at equal-time boundaries,
missing/foreign selected children, empty groups, source/qualification changes,
interior-at-zero, Move, wrong Object identity and zero-time Object replacement.

Two real-media service tests compare semantic Apply against its genuine
placement preview. They decode pictures before, inside and after both joins,
compare complete framing clocks and captured geometry, and compare bit-identical
canonical limited PCM at both joins and the retained suffix. The oracle permits
only fresh abandoned owner IDs and the outer revision header to differ.
Register version remains unchanged, and one Undo restores every authored field
of the baseline document.

The first test attempts correctly exposed those allocation/revision differences
in whole-value comparisons. The corrected oracle first verifies fresh IDs and
exact sample equality, then normalizes only those identities. Both failed logs
are retained. The final focused run passes all five tests.

The scoped `cargo xtask check` passes both strict Clippy configurations and all
89 selected tests. The UI-harness runtime configuration also passes all 89.
These include the existing placement, Move, macro service, receipt, bank and
stale-context regressions. They are scoped checks, not a full workspace gate.

Independent read-only review found no supported correctness issue in capture
admission, draft restoration, asynchronous Apply, refusal behavior or the
equivalence oracle. The reviewer ran no competing Cargo process.

The initial release replay stopped in an older delayed-command fixture before
reaching the new cases. It withheld a native semantic Split receipt and then
tried to open command entry, which the pending-action guard correctly blocks.
The fixture now submits the same real typed service edit directly, withholds
its update, and verifies the intended late-continuation command capture. The
guard remains in place. The failed report is retained.

The next replay reached the new recording cases and caught a hardcoded slot
zero in the fixture. Earlier scenarios retain zero-time siblings around the
Original. The replay now derives the selected child's literal slot, preserving
that useful structural case instead of treating all time-zero seams as equal.
This failed report and its source/binary identities are also retained.
The following attempt passed insertion recording and invocation-relative replay,
then exposed another fixture omission: Place slice opens in Insert mode even
with a retained selection. The replacement case now presses its production
`r` key before inspecting and applying the captured range.

A preparatory visual and separate quiet performance replay each passed 903
checks. Image review found the eligible-Apply capture was taken while the
exact proposed picture refresh was still pending. The native UI truthfully
retained and labelled the old picture. The final replay explicitly waits for
the exact proposed first frame before Apply; preparatory reports remain separate.

## Final release replay and visual review

Final release visual and separate quiet performance `place-slice` replays each
pass 904 checks. Each also passes the production shortcut audit against local
Kestrel: 62 reserved bindings, 21,885,504 routing cases, no conflicts or source
drift. The final format and strict normal/UI Clippy gate passes.

The new cases use production `q`, named registers, `:splice`, endpoint and
destination keys, `r`, `m`, Apply, save, paired native `@` input and Undo.
They check explicit Interior, refined-source and Move refusals with no writes;
eligible Original and Edited placement; exact saved typed instructions;
replay at a different selected child or backward range; unchanged source
registers; and complete authored-document restoration with one Undo.

All five retained 960×640 captures were inspected. The exact proposed frame,
refusal explanations, eligible Apply explanation, recording indicator, selected
inserted beat and replacement result paint fully in their intended regions.
The picture remains prominent. The visual run retains its intermediate-image
allowance warning, 63 second-retry frames and 48 input-driven runs of consecutive
retrying frames. No ignored retry or repeated idle retry fails a check.

The performance run has 1,382 completed input-to-state samples: median 0.364 ms,
p95 0.962 ms, maximum 2.172 ms. Its 430 completed input-to-picture samples have
median 1.777 ms, p95 14.067 ms and maximum 178.995 ms, including initial project
work. Neither metric has failed or timed-out samples. No build or other replay
from this task ran concurrently with this timing run. AC power was observed at
80% battery; thermal state and OS caches were uncontrolled. These short-fixture
offscreen results do not establish physical input-to-display or long-clip budgets.

Executed final app SHA-256:
`1fb8bffdfa44322a630fed1df4366132044bb10e83c85952d7c654a6d8fede07`.
The [retained evidence](../../tools/media-qualification/evidence/2026-10-18-place-slice-macros/)
contains final, preparatory and failed reports, source/binary/fixture identities,
source diffs, reviews, inspected captures and a verified inventory of 54 files.

## Limits

Recording does not acquire a Move instruction, arbitrary placement destination
or local source-refinement vocabulary. Those proposals remain usable outside
recording. No picture, DSP or export algorithm changes in this slice.
No additional encoded-output run is claimed. Physical input, VoiceOver speech
and listening remain owner verification under the
[project scope](../REQUIREMENTS.md#to-verify-owner).
