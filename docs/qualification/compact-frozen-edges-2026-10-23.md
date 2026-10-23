# Compact frozen audio edges, 2026-10-23

The Core 57 / SQLite 86 candidate passes Core, broader, UI-feature, strict
Core/Store, debug/release media, native replay and portable validation checks.
Explicit UI Clippy and current-format performance remain in progress. No
requirement or gate closes.

The current project/history encoding omits only all-Automatic frozen-node edge
objects and restores the same six policies when absent. Nondefault objects stay
explicit; the exact wire-size counter follows the encoding. Standalone audio
contexts 1 through 12 retain their explicit edge objects through a borrowed
streaming serializer. Old unused SQLite packages through schema 85 are refused
without mutation, under the development-format authorization.

| Completed check | Result |
|---|---|
| Core | 1,503 tests / 68 binaries, zero skipped |
| Broader selected | 3,499 tests / 109 binaries; 740 skipped or unselected |
| UI-feature | 1,292 tests / 2 binaries; 11 skipped |
| Strict Core/Store and formatting | Pass; explicit UI Clippy is queued |
| Debug media | 15 pictures / 24,024 samples, verified zero offset |

The debug fixture compares complete authored/limited PCM and limiter-gain bits,
active sound controls, Undo/Redo, reopen and portable-copy history. Its encoded
output is independently decoded and verified. This is not a listening or
independent DSP claim.

The immutable a18ff654 previous-code witness fails exactly the two omission
checks; four legacy, nondefault and malformed-input controls pass. Initial Core
runs exposed two obsolete fixture assumptions. The policy tamper test now
materializes its omitted default object before changing one forbidden policy.
The byte-bound test keeps 12,000 nodes with longer identities after an
intermediate 14,000-node fixture correctly hit the independent work budget.
The first broader run exposed two more expected-encoding assumptions. The
historical literal remains intact; only its expected current writer value
omits explicitly asserted Automatic objects. The history regression now checks
the complete initial timing/binding patch and small later tables directly,
retaining its later row-size and final catalog bounds. Production code stayed
unchanged through these fixture corrections. Independent review reports no
remaining codec or fixture defect.

The original broad archive built, but its driver exited 120 before listing or
running tests after the tool session disappeared. The cause is not proven.
All source/input hashes matched the archive; its resumed run used retained
stdout/stderr. Both the interrupted driver and failed fixture runs are retained.

A final test-quality review identified symmetric policy controls and a legacy
expectation derived from the current serializer. Fixed Automatic/asymmetric
legacy bytes have now been generated using the previous a18 Core library,
SHA-256 532aa006494ea2c3e8050c1bf14d460d4c550f3ed3cf67b38992ae9b4832673b.
The stronger tests are prepared for the serialized qualification driver after
media/native checks and before current-format performance; they are not yet
applied or executed.

The prior disposable prototype's five-pair medians, 127.513 versus 111.362 ms,
remain diagnostic only. It used Core 56 / SQLite 85, not this current-format
candidate. The current performance run is still owed; neither the 100 ms
Hold-insertion bound nor a p95 target is established.

[Checkpoint evidence](../../tools/media-qualification/evidence/2026-10-23-compact-frozen-edges/checkpoint/)
retains 203 payload files / 7,415,361 bytes. Its inventory SHA-256 is
1b8e99d8ae400d6d6774df39e4dcec08cb7624d5c2504a088069a0c106201627.
Every phase records source/input and actual executable identities, with Rust
overlays stored as .rs.txt. Root scratch is
/tmp/deadpan-compact-frozen-edges-2026-10-22. PID 10303 owns the current serialized
driver and writes final-driver.stdout/stderr plus per-step records. No binary,
nextest archive or large SQLite benchmark package is included in this checkpoint.

Follow-up: both release media fixtures pass, including all 19 pictures and
30,430 final samples at zero offset in the larger case. Native replay passes
197 workflow checks plus Kestrel and retains the existing two layout warnings.
Both nine-file portable packages validate and doctor with unchanged paths/bytes.
Six strengthened codec tests pass after the test-only refinement. The initial
xtask lint checked changed Core/Store packages only; the prior ordinary/UI
wording was broader than that command. Explicit UI Clippy is now queued before
timing. Remaining raw follow-up evidence will join the final inventory.
