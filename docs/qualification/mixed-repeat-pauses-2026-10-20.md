# Mixed and selected-play pauses, 2026-10-20

Freeze and black pauses now follow each enclosing Repeat's explicit All plays,
This play or selected-plays choice. Selected plays commit together and retain
the actual inserted Holds for subsequent edits. This advances DP-07; broader
occurrence editing remains open.

Initial qualification base revision `8d226bb5846db502718b59d5ed7075b84e958d1e`,
Apple M5 Max,
macOS 26.5.2 (25F84), Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Explicit scope and exact clocks

`InsertOccurrenceTime` accepts an explicit authoring `ScopedNodeTarget` separately
from its concrete `within: InstancePath` presentation. An omitted target retains
the previous command behavior. Explicit authority requires a presentation and
does not combine with the old `all_plays` switch. Default and Play choices are
validated at every level before any isolation or write.

The first Default ancestor is the shared edit root. Only unoverridden Default
uses multiply the inserted duration; a concrete Play contributes one use. In a
3-by-2 Repeat, outer Default/inner Play grows by three pauses, while outer
Play/inner Default grows by two. Existing overrides retain their own content.
Shared sound ripple starts at the shared root's old concrete end. Later plays,
gaps, root sounds and direct Repeat ancestry retain their own exact sampling
entries rather than accumulating a rounded root offset.

`prepare_occurrence_time` returns the actual new Hold address, presentation and
local-zero boundary, with an isolation proof that maps other captured addresses.
Continuation no longer searches the old cursor's picture, which could select
ordinary source content after a shared edit from a later representative.
Explicit Default admission does not require capacity for a hypothetical
all-Play isolation that the command will never perform.

## Atomic selected plays

The native host admits 2 through 1,024 distinct concrete plays of one Repeat
under the same exact prefix. Each selected owner uses the same local cut and
captures its own qualified picture and framing. Each later stage resolves its
boundary in the staged document and maps both remaining owners and earlier
inserted Holds through the actual isolation proof. The final bounded Compound
is independently previewed before one store commit and one Undo.

Different child durations can place the new Holds in different Sequence slots.
A queried composite owner can also resolve into different inner plays or
different ancestry depths at that local cut. The receipt retains each actual
Hold and its own suffix. It never rebuilds the set by child position. Subsequent
pause and gain edits retain the set; explicit row, scope, Enter or Backspace
navigation ends it. The inspector says `2 captured pauses · shown: …` when the
displayed inner choice is not common to the complete set.

Value-edit preparation now returns the complete multi-target isolation proof,
so a later gain edit cannot silently change a neighboring child. Empty and
malformed target sets fail without panicking or writing.

## Delayed results and review

Native captures include a runtime navigation generation. Frame, row, scope,
pane and view navigation revoke delayed cursor and inspector ownership even
when the user returns to equal coordinates in the same input batch. Macro
continuations restore only their exact pending runtime capture after checking
the service's remaining authority fields; saved programs contain no runtime
generation. Durable edits remain saved when a continuation is revoked.

Independent source review caught old-cursor receipts, mapped-prefix loss,
positional retargeting after unequal child durations, inferred owner descents,
multi-value receipt loss and same-batch navigation resurrection. The corrected
paths have focused regressions; final read-only review found no remaining
supported findings. The independent heterogeneous keyboard replay and private
package cleanup also passed source review. Follow-up review found two additional
navigation bypasses: an empty Sounds list can repair focus back to Viewer, and
pointer view tabs could retain scope through an Original/Edit round trip while
Viewer kept focus. Sounds entry and focus repair now advance the navigation
generation; view tabs use the existing command navigation methods. Delayed
receipt replays cover both paths.
The Original card and its inspector's return control use the same rule.
Successful Edit word, sentence, pause, shot and search seeks also revoke the
capture, including away/back navigation over the same analyzed words. Original
seeks and background analysis refresh do not revoke an unrelated Edit capture.

Initial failures and their corrections remain in the evidence: an incorrect
test oracle unwrapped a positional retarget that was allowed to refuse; helper
fixtures used retired copied IDs; old test constructors omitted a previously
added semantic context field; and strict lint rejected a single-range Vec and
a manual range check. No failure was hidden or disabled.

The initial optimized replay exposed a test synchronization error: its final
selected-play assertion ran after worker completion but before the refreshed
revision was admitted. The immediately following recorded frame already had
the saved 12-frame batch and closed inspector. The witness now waits for the
actual revision transition before checking continuation ownership. The failed
report, executable/source identities and complete source snapshot remain.

## Tests and media

Core matrices cover all mixed branches and start/interior/end cuts, sparse
overrides, direct Repeat ancestry, exact identity pools, marks, strict wire
admission and complete inverse. Qualified native and public CLI tests compare
every source picture, per-play freeze contexts, dry-run/commit behavior,
history, reopened Undo/Redo, stale capture and atomic refusals. Unequal child
slots and independently inferred inner plays receive repeated pause and gain
edits without losing their exact selected set.

Decoded 44.1 kHz audio at 30000/1001 checks both chunk schedules, all original
resumptions, pause/gap silence, owned sounds and root-sound ripple. Origin-based
sample allocation stays exact; no event alignment or AAC-block tolerance is
introduced.

The initial qualification's targeted gate passes 136 selected tests, format
and strict normal/UI Clippy. The broader core/store run passes all 1,874 tests; one generation-copy
test reports an output-handle leak in that concurrent run, then passes its
isolated rerun without a leak. The full native app UI configuration passes
1,220 tests with 11 existing skips and no output-handle diagnostics. These
overlapping runs are not a full workspace gate. A final format/normal/UI lint
pass includes the additional navigation and replay sources.

The optimized public CLI test builds and exports two actual H.264/AAC files:
69 pictures for Default/Play and 66 for Play/Default. Every picture passes
`verify-export --every 1`; all four emitted-audio windows have verified zero
offset, with 12 blocks compared and declared encoder priming retained in
metadata. Minimum luma PSNR is 56.455 dB and minimum block SNR is 31.359 dB.
Repeated identical pictures and black frames have index-unobservable neighbor
comparisons, reported explicitly rather than counted as ordinal proofs. Pure
and decoded-source tests independently establish those mappings. All six raw
click windows in each edited arrangement retain their original samples within
1e-6, before encoder loss.

Debug app linking retains the existing `__eh_frame section too large (max
16MB)` compact-unwind warning. It is not suppressed. The final optimized app and CLI build without that warning.

## Native replay and responsiveness

The initial qualification's optimized visual and separate performance replays
pass 226 and 226 scoped-play checks respectively, plus the live Kestrel
audit. The audit retains 62 reserved bindings, 21,885,504 routing cases and no
conflict or source drift. Existing gain, framing macro and Camera paths remain
covered alongside new mixed, selected-play, unequal-inner-play and delayed
receipt paths.

All five new 960×640 captures were inspected: This play, selected plays, mixed
authority, revoked delayed continuation and independently inferred inner plays.
Scope labels, available actions, independent clocks and the footer remain
visible. The synthetic transcript witnesses explicitly exercise stored
annotations and input routing, not recognition inference.

Separate optimized Original-layout and sound-playback regressions pass
333 and 60 checks, plus their Kestrel audits.
They retain same-Original cursor/picture behavior and independent sound focus
after the shared navigation changes.

| Measurement | Completed | Median | p95 | Maximum | Failed / timed out |
| --- | ---: | ---: | ---: | ---: | ---: |
| Input to state | 261 | 0.500 ms | 1.093 ms | 1.769 ms | 0 / 0 |
| Input to completed picture | 82 | 2.105 ms | 9.034 ms | 100.915 ms | 0 / 0 |

The 100.915 ms picture-completion tail remains recorded. These measurements do
not assert that every picture completed within one display interval.


The keyboard replay uses production widgets, SQLite, the actual project service
and offscreen Metal. It opens no native window and does not establish physical
keyboard/IME, VoiceOver speech, display or acoustic behavior. The separate quiet
performance run uses GPU submission without PNG readback. AC is attached with
the battery at 80%; caches, thermal state and unrelated system activity remain
uncontrolled. Short fixture measurements do not establish full-resolution or
physical-display latency.

## Integration with concurrent main

The feature rebased cleanly onto concurrent main
`8a6aeb4353632baf2bcb130a310d0732623bb40c`. Independent integration review found
no supported incompatibility with the new test completion receipts, subprocess
launch locks or physical-Mac test convention. The new export test now guards
only encoded Render on virtual CI; both fixtures still run their picture-plan
and decoded-PCM checks there. Local qualification sets
`DEADPAN_REQUIRE_PHYSICAL_MAC=1`, so both encoded outputs remain required.

On the integrated source, `cargo xtask check --since origin/main` with the
retained scoped/pause/completion filter passes format, strict normal/UI Clippy
and all 161 selected tests. The separate UI feature configuration passes all
98 selected scoped/macro/completion tests. One existing remote Visual-yank
test reports an output-handle leak in that concurrent run; its isolated rerun
passes in 0.248 seconds without the diagnostic. Both results remain in the
evidence. These focused runs do not repeat the earlier full core/store or app
suites. Tests use the updated optimized debug profile with assertions and
overflow checks retained.

The integrated release CLI regenerates both H.264/AAC files. All 135 pictures
and four zero-offset audio windows pass again, with the same minimum luma PSNR
56.455 dB and block SNR 31.359 dB. The final integrated release app passes
226 visual and 226 separate performance checks plus both Kestrel audits.
All five final 960×640 captures were inspected. The unchanged Kestrel source
retains 62 reserved bindings and 21,885,504 routing cases without drift.

| Integrated measurement | Completed | Median | p95 | Maximum | Failed / timed out |
| --- | ---: | ---: | ---: | ---: | ---: |
| Input to state | 261 | 0.501 ms | 1.098 ms | 1.709 ms | 0 / 0 |
| Input to completed picture | 82 | 2.054 ms | 9.042 ms | 99.188 ms | 0 / 0 |

The separate performance run starts with no rustc, cargo-nextest or Deadpan app
process in the recorded process check, on AC power at 79% battery. Thermal state,
caches and unrelated system activity remain uncontrolled. The 99.188 ms tail
is retained; these short-fixture measurements do not establish physical-display
latency or a one-display-interval bound.

The [integration evidence](../../tools/media-qualification/evidence/2026-10-20-mixed-repeat-pauses-integration)
retains the new logs, both exports and complete verifier reports, final replay
reports and inspected captures, source and executable identities, independent
review, scripts and checked SHA-256 inventory. Final integrated app SHA-256:
`f20731dbb5f620d990166ebf257074c6bdd499ce4d35695f55ef626650542689`;
CLI: `319727ff20ed8ba4db384da5519a3d2896e89427e536afbe7dd7980c7e895e3c`.
The export test's default-feature CLI and test executable identities are
recorded separately. The qualified private helper baseline remains unchanged.

## Evidence and remaining work

The [evidence directory](../../tools/media-qualification/evidence/2026-10-20-mixed-repeat-pauses)
retains failed and passing logs, final visual/performance reports, inspected
captures, both emitted movies and complete verifier reports, source snapshots
and hashes, binary and fixture identities, independent review, collection
scripts and the checked SHA-256 inventory. Initial qualified app SHA-256:
`c32d97f6247dab878fc0437a6b64e939587f3e4e7c62125201042c11d61d2d62`; CLI:
`bb6f07de5fea7198ea6a25f85ba94590aefc73a836449b4202380ea6a2e427ec`. The export test's executable identities are retained
separately. Unchanged private media, tracking and transcription helpers retain
their recorded hashes. The source manifest binds the executable to this base
revision plus its retained changes.

AI insertion still requires one explicit Play at every enclosing Repeat.
Selected-play AI batches, Retime ancestors on this occurrence path, broader
cuts/replacement/count/speed edits, implicit gap authoring, Original-moment
payloads, occurrence ranges and general semantic recording remain open.
Unsupported operations refuse before writes. No full-product requirement or
gate is completed by this slice.
