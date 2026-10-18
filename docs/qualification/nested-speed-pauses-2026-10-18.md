# Pauses through nested tape-speed stages, 2026-10-18

`InsertTime` now descends nested FollowSpeed Retime stages and intervening
transparent windows, retaining every stage as a live owner. The pause adds its
authored number of project frames and preserves the surrounding source clocks.
This advances DP-07; nested pitch-processing stages, Repeat scopes, broader
payloads and the other remaining operations are still tracked separately.

Base revision `a15638e12b7b2519eabb1a48a263233213e630de`, Apple M5 Max,
macOS 26.5.2, Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Exact ownership

Planning retains the output and input slope of each stage relative to the
project clock. It maps the cut through each exact child window and inserts the
Hold below the deepest owner. A single adapter gives the Hold its requested
project duration. Bottom-up clock updates grow each stage's output and selected
child span by their respective exact multiples of that duration. Fractions are
never rounded into intermediate edit decisions. The target query reports them
even when the net speed is unity.

Every later Sequence sibling is captured on its own pre-edit clock. The changed
stages detach their timing lineage; Split fragments retain theirs. Transparent
window offsets, subsequent pauses and entry into an earlier pause's adapter
follow the same path. Existing atomic refusals for unsupported processing,
marks, sounds, AI payloads and insufficient identity pools remain.

The native semantic path also needed a correction. After a pause authored
fractional clocks, its staged scope refresh used an ordinary duration query
outside the sealed read scope and refused the result. Only the ordinary outer
scope-boundary and child-duration reads now use that scope. They still prove
that recipes outside clocked stages equal exact extents, and still reject
Repeat/Retime ancestors. Per-leaf mutation validation is unchanged.

Visual inspection also found a rounded recipe leaking into the inspector: it
showed `245/163×` for an effective `3/2×` stage. The inspector now reads the
derived clock directly, including fractional fragment bounds. Updating the
speed of an already clocked stage remains unsupported; its inspector explains
that restriction and its command preview refuses before using rounded input.
Review found that this restriction must also include a stage with an integral
own clock but fractional descendant clocks. The service indexes that ancestry
once per committed workspace, off the UI thread; the inspector and command
preview both read the retained set. Tests cover an integral outer stage and an
unrelated, ordinarily editable sibling.

## Tests and retained failures

Fourteen core retimed-pause tests pass, including exact inverse, nonzero
Partition windows, intermediate fractions, insufficient identities, processing
refusal and a pause inside a prior adapter. Independent decoded 44.1 kHz PCM,
picture and SQLite tests cover net rates 1, 4/3 and 2 with siblings inside both
stages and after them. Pictures retain their exact source coordinates; commits,
reopens and every Undo/Redo restore complete authored snapshots.

The PCM oracle derives physical-piece boundaries analytically and retains each
piece's original sample support across edits. It checks every sample and silence
flag, raw and edge-faded output, and alternate chunking. It performs no signal
alignment or sample tolerance. On the NTSC grid, B(7→8) adds 1,602 samples but
B(21→22) adds 1,601. A temporarily shortened allocation may also hide a retained
last sample that a later edit exposes again. Those are independent allocation
clocks, not one common suffix offset.

The initial fixture failed before insertion because its selected audio used a
48 kHz time base against a 44.1 kHz asset. Two subsequent test-oracle assumptions
were corrected: a common suffix offset and exhaustion based on a temporarily
shortened render. Their failed reports are retained. The scoped check passed
113 of 114 tests; the corrected audio test then passed separately, alongside
its existing nested mapping test. The broader semantic planner suite passes
144 tests, including staged pauses/motion, Compound replay/inverse, a later
invocation on an already clocked document and refusal of an inner scope.
Final strict Clippy passes in normal and UI-harness configurations.

After the inspector correction, the scoped gate passes all 114 inspector and
retime tests. Nextest reported a leaked output handle on an existing pure
inspector test; that test passes alone in 0.012 s without a leak warning. The
initial warning and isolated result are both retained. The linker also retains
its existing large `__eh_frame` warning.

The final ancestry correction passes 125 of 126 scoped tests on its first run.
The new core fixture failed validation because its integral cover was too
short; correcting that test-only cover gives a passing focused continuation.
Two existing tests reported leaked output handles in that concurrent run; both
pass sequentially without leak warnings. Production native pause and inspector
tests passed on the first run. An earlier harness build also caught a diagnostic
that tried to serialize a non-serializable resolution; it now formats Debug.
All 21 affected app tests also pass with the optional UI harness enabled, and
final strict lint passes for the normal and UI-harness configurations.

The first release replay exposed the semantic scope-refresh refusal described
above. It is retained as a real integration failure, not counted as passing.
The next replay saved the correct freeze but failed a test expectation that
omitted its captured canvas; the assertion now checks the actual 320×180 canvas
and empty captured layers, with all three frozen frames at the measured PTS.
An earlier replay build also caught two private-field accesses in test code;
those use the public accessors now. Independent review found no remaining
clock, ownership, lineage or guard-locality defect.

## Native replay, performance and export

Final release visual and performance `retime` replays each pass 36 checks plus
the production shortcut audit against the local Kestrel source (62 reserved
bindings, 21,885,504 routing cases, no conflicts or source drift). They exercise
a three-frame freeze inside two live tape-speed stages, exact source PTS and
captured canvas, unchanged Original cursor, complete-snapshot Undo/Redo and a
later one-frame black pause. Full-size captures were inspected. The freeze
shows `3/2×` and the exact-clock restriction within the inspector. The later
pause makes the outer clock integral again, while the inspector and speed
preview still explain the retained fractional descendants. The resulting
picture is black and the edit has grown by four frames.

Performance mode records 49 input-to-state samples: median 0.488 ms, p95
1.260 ms and maximum 1.388 ms. Fourteen input-to-picture-complete samples have
a median of 7.625 ms and maximum of 89.268 ms, including initial project work.
All samples complete. These are offscreen replay timings, not physical
input-to-display measurements. Both runs retain inspector-scrolling warnings,
14 frames requiring a second settled layout retry and three runs of consecutive
retrying frames with distinct input causes. No ignored retry or repeated idle
retry fails the replay.

The new `nested-tape-pause` preview/export fixture passes independently expected
picture provenance for all 37 pictures and emitted-file audio verification.
The fixture shortens the source, nests tape speeds 1/2 and 3/2, then inserts
three black frames. Audio has verified zero residual offset, 33.507 dB minimum
block SNR and 0.02323 dB maximum block level difference. Minimum luma/chroma PSNR
is 56.455/53.445 dB. The publication declares 2,048 priming samples; verification
uses the emitted timing metadata, without event alignment. Render/verification
take 1.403/0.569 s. This scoped run selects one of 71 fixture variants; the other
70 were explicitly skipped, not counted as passing.
Automatic encoding rejects the two-B-frame hardware probe for packet PTS before
DTS, then selects the verified hardware path without B-frames. Both decisions
are retained in the publication sidecar.
This export ran before the final inspector eligibility/cache correction;
the pause-timing and render/export algorithms did not change afterward.

Executed release app SHA-256:
`e246de39832c1c188759884a14bd8162eb4ba881dcf7df8a3bebd8d666ebbae4`.
Export CLI SHA-256:
`90337e4e4f6b0526b99e97888d7490570418328294e3f66b09530d2ab6aa592c`.
The [evidence](../../tools/media-qualification/evidence/2026-10-18-nested-speed-pauses/)
retains source and binary identities, failed attempts, final reports, inspected
captures, the actual emitted MP4, publication sidecar, probe and verified inventory.

## Limits

These fixtures are short, deterministic media and structural cases. They do not
qualify long-clip performance, listening, device output, physical keyboard or
VoiceOver behavior. Exact physical-input and listening steps are on the owner
list in [Requirements](../REQUIREMENTS.md#to-verify-owner). No AI provider,
mark/sound transport, nested Preserve/Shift insertion or Repeat occurrence
capability is implied by this work.
