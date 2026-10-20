# Single-play pause macros, 2026-10-20

Freeze and black pauses now record and dot-repeat with This play explicitly
chosen at every enclosing Repeat. Each invocation resolves its current exact
boundary and retains the actual new Hold for subsequent instructions. This
advances DP-06 and DP-07; general occurrence editing remains open.

Base `2b8a27dee4bc0a5e867481a96ae3f840491ff471`, Apple M5 Max,
macOS 26.5.2 (25F84), Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Intent, provider and continuation

Saved `InsertPause { length, black }` intent contains no concrete owner, play,
presentation, picture or invocation boundary. Scoped Run and Apply require
separate explicit authoring and presentation addresses, with Play authority at
every Repeat level. Each expanded named call receives the same admission.

`PauseSite::ScopedBoundary` passes the captured occurrence to the existing
qualified freeze resolver. It retains measured Original PTS and captured
framing below the insertion parent. Black bypasses picture resolution. A host
callback returning non-silent or unsupported picture content refuses before
allocation. Freeze capture retains its existing foreign-parent refusal.

The planner uses the exact occurrence identity requirements and
`prepare_occurrence_time`'s actual new Hold target, presentation and boundary.
A subsequent pause, Zoom or call continues there. The ordinary parent, cursor
and enclosing selection remain unchanged, including a Repeat seam beside a
sibling. The native receipt checks that invariant before publication. One run
commits once and one Undo restores the complete document and isolation.

Native single-play freeze/black controls now use semantic Apply even outside
recording, establishing dot intent. The captured macro and scoped targets must
agree, including their navigation generation. Mixed choices, selected-play
batches and AI keep their existing direct editing paths; they cannot supply
this semantic authority. New writes still wait for pending work. Non-recording
scoped navigation may continue and revoke the queued inspector continuation.
An abandoned recording cannot append its delayed successful instruction.

Zoom requires qualified picture geometry. Framing a black Background pause
refuses the complete run without writes. Retime ancestry, Visual selections,
scoped AI, reverse/tail intent and other scoped instructions remain unsupported.

## Tests and review

Six new core cases cover first/last nested plays and start/interior/end cuts,
existing overrides, exact pools, provider failures, mapped pause/call/Zoom
continuation, sibling seams, strict authority, complete inverse and atomic late
refusal. All 13 focused pause/framing tests pass.

Qualified CLI cases compare dry-run and commit, measured freeze PTS and
captured geometry, every retained Original picture coordinate, invocation on
another play, unchanged register banks, history and reopened Undo/Redo. Native
service cases retain the actual Hold, ordinary context and source pictures,
rebind dot, preserve durable receipts through refresh failure and reject late
unsupported calls atomically. All 10 focused native/CLI tests pass.

Initial failures exposed two test-oracle mistakes: reopen was compared against
the original pre-Undo revision instead of the durable Redo result, and a CLI
success recipe asked Zoom to frame black Background. The corrected success
recipe frames the freeze before inserting black; separate normal and dry-run
cases prove the black-to-Zoom refusal leaves the complete database unchanged.
The failed log and independent follow-up review remain retained.

Independent review found no actionable source or integration findings. Replay
review caught the pending-action navigation gate before execution; scoped pane,
command and analysis navigation now retain their previous behavior while
revoking stale continuation ownership. Existing delayed keyboard, pointer,
word and search witnesses pass with the semantic path.

Final `cargo xtask check` passes format, strict normal/UI Clippy and all 211
selected tests. The separate UI feature run passes all 101 selected tests.
Each concurrent run reports one output-handle diagnostic in an existing pure
receipt/telemetry test. Their isolated reruns pass in 0.011 and 0.010 seconds
without diagnostics. Both original results and reruns remain in evidence.
These overlapping selections are not a full workspace gate. Tests retain
debug assertions and overflow checks in the optimized debug profile.
The generated command reference was reviewed and its check passes.

Timing reduction and rendering use the existing occurrence command. The
[mixed-pause qualification](mixed-repeat-pauses-2026-10-20.md) retains its
decoded 44.1 kHz PCM and real H.264/AAC export evidence; this slice adds no new
DSP or encoder path.

## Native replay and measurements

The final release visual and separate performance replays each pass all 312
scoped checks plus Kestrel's 62 reserved bindings and 21,885,504 routing cases,
with no source drift. Actual keyboard cases record/save freeze and black
intent, execute on another play, dot at a new invocation, retain the exact Hold
through another pause and Zoom, refuse unsupported scopes and preserve newer
navigation after a withheld recording receipt. The original document and
independent Original clock are restored after verification.

All three new 960×640 captures were inspected. The authored crop, selected
pause, independent clocks, focus cues and footer remain legible. The delayed
case truthfully shows the saved edit and expired inspector continuation.

| Measurement | Completed | Median | p95 | Maximum | Failed / timed out |
| --- | ---: | ---: | ---: | ---: | ---: |
| Input to state | 352 | 0.498 ms | 1.040 ms | 1.656 ms | 0 / 0 |
| Input to completed picture | 114 | 2.091 ms | 11.167 ms | 109.054 ms | 0 / 0 |

The performance run uses GPU submission without PNG readback. Its process check
shows no Cargo, rustc or Deadpan app process before launch; AC is attached at
80% battery. Thermal state, caches and unrelated system activity remain
uncontrolled. The 109.054 ms tail is retained. Short fixtures and offscreen
Metal do not establish physical-display latency, keyboard/IME behavior,
VoiceOver speech or acoustic results.

## Evidence

The [evidence directory](../../tools/media-qualification/evidence/2026-10-20-scoped-pause-macros)
retains failed and passing logs, final replay reports, inspected captures,
review, collection scripts, source delta and hashes, executable identities and
the checked SHA-256 inventory. App SHA-256:
`4d299f4fc348b8299f87869299122090dcc78f99f7e13aa9663bd384eccd40d7`;
CLI: `3a98f5542974f69b41e633ad6986a50df2c29059e92b717310d3569941c370f6`.
Qualified private helpers retain their recorded baseline hashes. No full
product requirement or gate is completed by this slice.
