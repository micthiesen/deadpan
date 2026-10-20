# One-play Hold duration, 2026-10-20

An existing Hold now accepts an exact length change with This play explicitly
chosen at every enclosing Repeat. Enter and Change duration open its captured
length command. The change records and dot-repeats on a new invocation, and
continues at the actual resized Hold's start. This advances DP-06 and DP-07;
general occurrence editing remains open.

Base `0dbc16e365089bb81b1be3b38cada482e1304d92`, Apple M5 Max,
macOS 26.5.2 (25F84), Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Intent, isolation and exact continuation

Saved `SetHoldDuration { length }` contains no concrete owner, play or cursor.
Each invocation captures separate authoring and presentation addresses with
explicit Play authority at every Repeat level. Exact temporal isolation pools
and the executed command's isolation proof supply the mapped Hold. Its final
local-zero anchor supplies the cursor, including when a shrink removes the
old interior cursor. The ordinary parent and enclosing selection remain
unchanged. Further duration, Zoom, pause and named-call instructions continue
through the mapped target. One run commits once and one Undo reverses it.

No-op lengths refuse before allocation and preserve cursor, history and bank.
All plays, mixed or multiple choices, Retime ancestry and root or attached
sound events refuse this temporal path. A late unsupported instruction rolls
back the complete run. Existing synthesized Tone and RoomTone Hold audio are
supported. The duration restriction on external sound events is separate from
the Hold's own audio policy.

Accepted generated pictures remain accepted when shortened or extended within
their retained coverage. An extension beyond that coverage commits the
deterministic fallback and a durable preparation request; a new candidate still
requires explicit acceptance. Store tests use a real qualified accepted bundle
and verify mapped requests, reopen, Undo cancellation and fresh Redo intent.

Both keyboard and pointer length entry share the pending-action guard. The
inspector hides unavailable duration entry and explains its scope requirements.
Successful receipts carry the verified cursor to scoped reconciliation. Newer
navigation can abandon inspector continuation while retaining the saved edit;
an abandoned recording cannot append the delayed instruction.

## Audio failure and correction

The initial focused run passed five cases and failed both new PCM cases. A
fractional frame/sample boundary exposed a real old-clock error in the occurrence
duration reducer: shortening Hold `[3,5)` to `[3,4)` moved the next Source from
`[5,7)` to `[4,6)`. At 30000/1001 fps and 44.1 kHz, treating rounded sample
6406 as the new source origin instead of exact 6406.4 reset source phase by
`-147/400` sample. Actual PCM `[-0.06055708, -0.06055708]` differed from
expected `[-0.09149733, -0.09149733]`.

After isolation and before reduction, the occurrence path now captures audio
allocations after the concrete old Hold end on their existing clocks. This
includes later ancestor plays and gaps and initially unbound root suffixes.
The changed Hold interior stays on its own policy; existing downstream
lattices retain phase. Terminal Holds need no suffix capture. The historical
non-Repeat wrapper keeps its existing behavior, and repeated raw commands
explicitly refuse Retime ancestry.

The corrected focused run passes all nine cases: six core, one store and two
canonical-PCM tests using deterministic provider samples. The PCM assertions
were not weakened. They compare retained
Sources and gaps across first/last nested plays, grow/shrink, captured Source
fragments, initially unbound suffixes, RoomTone phase and varied chunk sizes.
Minimized pure regressions also cover partial root bindings and direct
Repeat-to-Repeat ancestry. Failed and corrected logs remain retained.

## Checks and independent review

Qualified CLI tests compare dry-run and commit, mapped continuation, every
retained source picture, bank/history, reopen and Undo/Redo. Native service
tests cover interior shrink, already isolated identity, compound continuation,
atomic refusals and a durable successful edit whose first refresh fails.

Independent reviewers audited core production and native integration they did
not author. Review found pointer entry bypassing the Enter pending-action guard
and an offered duration control under unsupported Retime/sound contexts. Both
were fixed, with keyboard/pointer parity and a qualified root-sound replay
witness. Independent review of the PCM fix found no further issue. A fresh
general reviewer could not start because the agent thread limit was reached;
the reused reviewers' scopes and results are retained.

Final `cargo xtask check` passes formatting, strict normal/UI Clippy and all
234 selected tests. Two existing receipt tests report output-handle diagnostics
in the concurrent run; each passes individually in 0.010 seconds without a
diagnostic. The separate UI feature run passes all 103 selected tests without
diagnostics; the later footer-hint change is covered by the final normal check
and release replay. These overlapping selections are not a full
workspace gate. An initial Clippy failure concerned only a one-element Vec of
Range in a new test; the initializer was made explicit without changing its
assertions. The generated command reference was reviewed and its check passes.

A separate 57-test selection passes the existing occurrence edits, ordinary
pause-duration semantics and generated-Hold preparation lifecycle, including
admitted extension, cancellation, compaction and Redo controls. No
output-handle diagnostic appears in this selection.

The first release visual replay stopped at an older Camera recording-gate
assertion that compared the complete help sentence. Camera remained closed,
the recording retained zero instructions and the revision did not change.
The expected sentence now includes the newly supported pause lengths; all
behavior assertions remain unchanged. The failed report and first executable
identities remain retained.

The next replay passed 337 checks, including the new duration interactions,
then a fixture's open predicate timed out. Production had successfully opened
the qualified sound-policy project and was idle with no UI error. The fixture
compared a temporary path's spelling with the service's canonical path; macOS
resolves `/var` through `/private/var`. The expected path is now canonicalized
before comparison, matching production open. The state and policy assertions
remain unchanged, and the failed report is retained.

Capture inspection found a real teaching defect: duration dot worked, but the
scoped footer still called it unavailable. Its hint allowlist now includes
`SetHoldDuration`, and replay explicitly checks `repeat: pause length set to
7f`. The original capture is retained beside the final inspected state.

## Real emitted media

Two release fixtures resize an existing freeze in a nested 2×2 Repeat of the
qualified `cfr-bframes.mp4` Original at 30000/1001 fps. The first/first Hold
grows from three to five frames (62 total); the last/last shrinks to one
(58 total). Every expected picture is specified independently of the plan.
Four retained Original click windows per fixture compare decoded canonical PCM
before and after the change within `1e-6`; explicit Hold and gap windows stay
silent.

Both actual H.264/AAC movies pass `verify-export --every 1`: all 120 pictures,
four full-coverage audio windows and eight compared sound blocks pass. Every
audio window has a verified zero-sample offset, no flags and no uncovered
samples. The minimum block SNR is 31.550 dB for grow and 34.411 dB for shrink;
maximum level difference is 0.063 dB. Emitted timing metadata declares 2,048
samples of priming. Repeated/frozen pictures are index-unobservable where the
report says so; no index distinction is invented for identical images.
Retained movies, full reports and executed CLI/test binary hashes accompany
the independent 44.1 kHz PCM regressions.

## Native replay and measurements

The final release visual and separate performance replays each pass all 444
scoped checks plus Kestrel's 62 reserved bindings and 21,885,504 routing cases,
with no source drift. Keyboard and pointer cases open the exact length field,
grow and shrink an existing Hold, retain its provider and identity, reject
no-ops without moving the cursor, and continue through Zoom and another pause.
Recorded length intent replays on a different black Hold; direct Apply and dot
resize a separate freeze. Pending recording blocks both entry paths, and newer
navigation cannot be reclaimed by a withheld successful receipt. Explicit
All/mixed/multiple-play, Retime and qualified root-sound refusals preserve the
document. The full starting document and independent Original clock are restored.

All three new 960×640 captures were inspected. They show the selected Hold and
its actual start clock, corrected dot teaching and the saved edit after
recording continuation is abandoned. Semantic checks establish the separate
play durations/providers. The compact controls and footer remain legible.

| Measurement | Completed | Median | p95 | Maximum | Failed / timed out |
| --- | ---: | ---: | ---: | ---: | ---: |
| Input to state | 511 | 0.515 ms | 1.026 ms | 1.731 ms | 0 / 0 |
| Input to completed picture | 170 | 2.059 ms | 13.827 ms | 93.848 ms | 0 / 0 |

The performance run submits GPU work without PNG readback. Its process check
shows no Cargo, rustc or Deadpan app process before launch; the recorded power
state accompanies the report. Thermal state, caches and unrelated system
activity remain uncontrolled. The 93.848 ms picture-completion tail is
retained. Short fixtures and offscreen Metal do not establish physical-display
latency, physical keyboard/IME behavior, VoiceOver speech or acoustic quality.
Those owner-only checks remain on the requirements tracker.

## Evidence

The [evidence directory](../../tools/media-qualification/evidence/2026-10-20-scoped-hold-duration)
retains failed and passing logs, replay reports, inspected captures, emitted
movies and verification reports, independent review, collection scripts,
source delta and executable identities, and the checked SHA-256 inventory.
App SHA-256: `a604e86d1543b501874967b1a73fe46ed3b4b60ca15e5154325c8de89f30055a`;
CLI: `c70dcf10833a63ca4de2ee0ce13c54921f61a388f0b0fd0beb3f601c2dec72e3`. Qualified private helpers retain their
recorded baseline hashes. No full product requirement or gate is completed by
this slice.
