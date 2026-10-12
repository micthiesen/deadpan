# Attached sounds in partial Repeat selections, 2026-10-12

Repeating an eligible interior range now keeps attached sounds on its surviving
fragments and both initial plays. Split reports exact owner aliases; sound
transport permits the Repeat introduced by this command and applies its existing
initial-play cohort mapping. Later count growth creates independent clocks.
Shrinking and regrowing does not reuse retired play identities. Existing external
Repeat ancestry still refuses atomically.

The real-media test exposed a second issue. When every physical recipe already
had a saved clock, Repeat omitted its initial source capture and reused that
identity for a suffix layout captured after Split. Restoring the sound journal
then correctly rejected two different layouts with the same identity. Additional
Repeat captures now reserve the base ordinal for the pre-edit layout even when
the source capture does not use it. Allocation remains checked and bounded.
There is no schema or DSP change.

## Verification

The core regression binds every physical recipe before the partial Repeat,
checks the retained timing against an independent pre-edit capture, and verifies
exact inverse/JSON round trips, initial-play applicability, later births and
shrink/regrow identities. A separate negative keeps the existing external Repeat
guard explicit.

Independent PCM regressions cover dry Source sound, event plus shared owner EQ,
and dry nested Preserve stages. They compare old fragment windows, initial
plays, later clock births, current silent gaps and complete Repeat copy/recopy.
Dry PCM remains exact; treated PCM uses the existing 1e-6 bound. Interior gap
comparisons avoid mistaking intentionally current boundary fades for old input.

The CLI fixture starts with copied and recopied real stereo WAV attachments
after deleting their original owner. It repeats an interior two-frame range,
grows to three plays, adds one-frame silent gaps and performs durable Undo/Redo
with fresh revisions. Independent nonzero PCM windows and all four sound-owner
recipes survive these operations.

All 1,481 core/plan tests, 48 attached-sound audio tests and six store clock tests
pass, with no failures or ignored tests. Strict workspace/all-target Clippy with
`ui-harness` and all three new release PCM regressions pass. Debug and release
real-media runs agree, including the explicit AAC diagnostic failure below.
App/CLI release builds and formatting also pass.

The qualified `414b9d6b` release binary refuses the same partial-Repeat request
against the retained seven-frame project without changing its document. The final
release binary accepts it and preserves all four sound owners. The request,
before/after dumps, responses and both executable hashes are retained.

## Explicit failures and remaining work

The 13-picture public Render result is **not fully qualified**. All pictures pass
and the signal window has measured offset zero, but one of 33 compared AAC blocks
fails the unchanged 1.5 dB minimum SNR. Its measured SNR is 1.0882855905 dB at
sample 11,520; maximum block level difference is 1.5285941696 dB. The regression
checks the sole `audio_block_snr` flag and preserves the diagnostic failure.
This is negative encoded-fidelity evidence, not a tolerance exception or a
listening result. The emitted file and complete report remain available for
encoder/input investigation.

Adding event and owner EQ to the nested Preserve fixture fails its **pre-Repeat**
reference render with `Limit("prepared stages per read")`. The dry nested case
passes. The failed logs, backtrace and exact reproduction patch are retained;
the next preparation-cache work must resolve it without raising the stage cap
or aliasing different independent voices. That combination is unqualified here.

Root-owned attachments, preexisting external Repeat ancestry, changed enclosing
processing subtrees and standalone clones outside their retained scope still
have explicit guards. Native attachment placement and broader mastering/export
qualification remain open. No DP requirement or release gate is newly complete.

The primary agent implemented and self-reviewed this increment alone. No live
GUI, device output or listening check ran. [Retained evidence](../../tools/audio-qualification/evidence/2026-10-12-partial-repeat-sounds/README.md)
includes source/binary hashes, command logs, failures, the implementation patch,
real project/movie and self-review.
