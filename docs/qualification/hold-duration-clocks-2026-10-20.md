# Ordinary Hold duration clocks, 2026-10-20

Ordinary Hold resizing now retains surviving audio allocations on their old
sample clocks when the change moves output time. This includes a shared Hold
definition used by several Repeat plays and a Hold in an owned override.
The [one-play duration qualification](scoped-hold-duration-2026-10-20.md)
identified this adjacent gap; its explicit occurrence path remains separate.
This advances DP-07 without completing the requirement.

Base `114823c1`, Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1 and the
qualified FFmpeg 8.0.3 prefix. No UI controls, keybindings or format changes.

## Failure reproduced before the fix

The minimized timeline is a three-frame silent lead, a two-frame Hold and a
two-frame Source at 30000/1001 fps. Its Original audio is 44.1 kHz, beginning
at source sample 3100. The Source has no retained timing binding. Shrinking
the Hold to one frame moves the Source from `[5,7)` to `[4,6)`.

`B(5) = 8008`; `B(4) = roundEven(6406.4) = 6406`. Deriving a fresh Source
clock from the new live position resets source phase by `-147/400` sample.
The old implementation fails at new sample 6406: actual stereo
`[-0.06055708, -0.06055708]`, expected
`[-0.09149733, -0.09149733]`. A separate store regression fails because the
surviving Source still has no old physical clock after the resize. Failed logs,
the old executed PCM test binary hash and source identities are retained.

The PCM oracle independently constructs the first 256 samples at literal old
sample 8008 and source phase 3100. The complete old canonical Source allocation
then supplies its finite endpoint filtering for an exact before/after comparison
of all 3,203 retained samples. The new allocation is 3,204 samples long because
`B(6) = 9610`; its extra sample at 9609 stays suppressed zero. Alternate read
chunk sizes produce identical results. These are deterministic canonical WAV
provider tests, separate from decoded real-media qualification below.

Initial oracle attempts requested an oversized block, then compared an
independent full-span resample against the finite Source's endpoint filtering.
Those failures occurred before resizing and are retained separately. The final
oracle uses bounded reads and keeps exact comparisons at the entry and across
the full retained before/after window. It performs no alignment or phase
correction. A separate initial store-test compile failure used obsolete
Compound field names; the fixture now uses `ResolvedTransaction`.

## Capture and preservation

The existing reducer first creates an unpublished candidate, preserving
generated-provider fallback and Tail clamping. When structural root duration
changes, the command captures affected owners against the complete old root
window before publication. Compact Repeat arguments retain each concrete old
entry without enumerating plays. Existing bindings keep their phase; initially
unbound Sources and gaps acquire their old clocks.

Provably silent Holds, absent or silent default gaps and nonphysical Sequence
owners do not acquire new reanchor steps. Their existing bindings stay intact,
including through a later explicit RoomTone change. Every Source and Retime
remains conservative, as do non-silent Holds and gaps. This avoids quadratic
clock work when a Compound resizes many silent Holds. Sound-event journal
capture remains separate and unchanged.

The full old timing layout remains available through attached-sound
restoration. Physical bindings and sound journals can therefore share the
leaf's timing ordinal 0 without identifying different layouts. Existing sound
admission, lineage and mark transforms remain in their previous order.

No-op, terminal and dormant-definition edits avoid unnecessary timing capture.
The dedicated clocked-Retime path is unchanged. Ordinary Retime edits with no
root ripple retain their existing behavior; this change does not qualify those
timing semantics. `EditOccurrence` still uses its own preparation and direct
reducer, so it does not capture twice.

## Focused checks and review

All eight new core tests and the durable store test pass. They cover direct,
shared, nested and owned Repeat targets, partial bindings, exact inverse,
no-op/terminal/dormant cases, marks, invalid requests, Tail clamp and a complete
attached-sound journal clock. A million-by-million nested Repeat retains at
most one new timing table, under 50 KB serialized bindings and the same graph
node count. This is a compactness assertion, not a GUI latency measurement.

The store test checks a raw no-op, resize, validation, reopen, Undo, Redo and an
atomic late Compound refusal. Metadata fixtures do not claim decoded-media
qualification.

All three PCM tests pass. They cover the literal failing entry, grow/shrink,
Silence and RoomTone, unbound and partly bound suffixes, separate default and
owned plays, gap and tail allocations, current Hold support, exact suppression
and chunk independence.

Independent read-only review found no concrete concern in dispatch ordering,
compact owner capture, binding lifecycle, sound restoration or lineage/marks.
The reviewer ran no Cargo; execution belongs to the main session.

The first broad check passed formatting and strict Clippy, then ran 2,615 tests:
2,613 passed and two failed. The older captured-Repeat PCM test required the
entire binding representation to stay identical. Its assertion now preserves
the old lattice, resume, existing reanchors, unrelated binding and every old
timing record, while retaining its independent per-play PCM and inverse checks.
The new conservative Repeat capture can add a timing record without changing
those old clocks. A separate 257-Hold generation-preparation Compound exhausted
the existing audio-binding work budget. That is an implementation regression;
the silent-owner filtering above addresses it, with a new 257-Hold historical
binding Compound regression and a mixed audible/silent transport regression.
The failure is retained and limits are unchanged. The first run also reported
one output-handle diagnostic for
`generation::operation_tests::legacy_and_bridge_wrappers_preserve_their_explicit_protocol_contracts`.

The corrected focused run passes all 16 selected tests in 15.159 seconds,
including the 257-Hold store transaction in 15.157 seconds and the pure bounded
Compound in 0.455 seconds. Its one output-handle diagnostic has a clean isolated
rerun in 0.098 seconds. Read-only inspection found no worker, subprocess or
intentional sleep in that metadata test; the diagnostic's owner remains
unproven. No timeout or work limit was raised.

The final `cargo xtask check --since 114823c1` passes formatting, strict Clippy
and all 2,617 selected tests in 109.742 seconds, with no output-handle diagnostic
(724 outside the selection skipped). This covers core, audio and store, plus
CLI macro, scoped and completion paths. No native UI changed in this slice;
the separate one-play duration replay record remains its own evidence.

## Real-media fixture oracle

The first release attempt passed the four complete pre/post authored-PCM
comparisons, then failed the generic fixture's `peak > 0.5` loud marker at output
sample 72024: the decoded limited peak was 0.3876102566719055. That generic
amplitude assumption does not qualify independently rounded unbound plays.
The corrected fixture retains the same nonzero 256-sample old/new comparisons
within `1e-6`, at the same four literal offsets, and records each pre-edit
authored peak. Only explicit Hold/gap silence windows use the generic marker.
The shared verifier, PCM tolerance, full-file SNR and zero-offset requirements
are unchanged. The failed attempt, its source and binary identities remain
retained. Production code did not change after the clean 2,617-test run.
Independent read-only review found no loss of phase/content coverage. This
fixture does not establish an independent absolute limiter-gain oracle.

After that fixture-only correction, `cargo xtask check --since 114823c1
--no-test` again passes formatting and strict Clippy. The release test passes
both real `cfr-bframes.mp4` recipes in 9.61 seconds on this Mac:

| Shared Hold definition | Pictures checked | Audio windows | Minimum block SNR | Maximum block level error |
| --- | ---: | ---: | ---: | ---: |
| 3 to 5 frames, all four nested plays | 68 | 2 | 36.808 dB | 0.018790 dB |
| 3 to 1 frame, all four nested plays | 52 | 2 | 38.434 dB | 0.025734 dB |

All four audio windows cover their complete allocations with no uncovered
samples, empty flags, `verified_zero` offsets of 0 and no low-SNR, level or
sound-in-silence failures. Nine sound blocks and 391 silent blocks are checked.
AAC declares 2,048 samples of priming. Every emitted picture is decoded and
compared; minimum luma/chroma PSNR is 56.455/53.445 dB. Each export has 70
observable neighbor-index comparisons; repeated identical pictures make
64/32 comparisons unobservable, with no missing neighbors or failures. Literal
plan provenance is checked independently for every output frame.

Executed release CLI SHA-256:
`a4f8ca9375244ded628ea442edd4d78a97f370ef5d13c075fbcf8aa345b9bb7f`.
Test binary SHA-256:
`629bf098d351ca5ff35b84c68bda4f18157af1e538b2ce1af9cff79423d0a246`.
The qualified private helper baseline is unchanged. Logs, source deltas and
identities, review, complete reports, emitted movies and their verified SHA-256
inventory are retained in
[`2026-10-20-hold-duration-clocks`](../../tools/media-qualification/evidence/2026-10-20-hold-duration-clocks).
