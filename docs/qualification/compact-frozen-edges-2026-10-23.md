# Compact frozen audio edges, 2026-10-23

Implemented and qualified on this Mac: project/history frozen nodes omit only
all-Automatic edge objects and restore the same six policies when absent.
Nondefault objects remain explicit. The exact wire-size counter follows this
encoding. Core schema 57 and SQLite schema 86 identify the format; old unused
packages through schema 85 are refused without mutation. Standalone audio
contexts 1 through 12 retain their explicit historical encoding through a
borrowed streaming serializer. No migration or validation shortcut was added.

| Verification | Result |
|---|---|
| Core | 1,503 tests / 68 binaries, zero skipped |
| Broader selected | 3,499 tests / 109 binaries; 740 skipped or unselected |
| UI-feature | 1,292 tests / 2 binaries; 11 skipped |
| Final strengthened codec tests | Six pass, including two fixed historical fixtures across schemas 1–12 |
| Strict | Changed Core/Store, final Core and explicit UI Clippy pass; formatting passes |
| Debug and release Ungroup export | All 15 pictures / 24,024 samples, zero offset |
| Release group/Split/Move/copy export | All 19 pictures / 30,430 final samples, zero offset |
| Native replay | 197 workflow checks plus Kestrel pass |
| Portable packages | Both nine-file extractions validate and doctor with unchanged paths/bytes |

The media fixtures compare complete authored/limited PCM and limiter-gain bits,
active sound controls, durable Undo/Redo, reopen and portable history. Emitted
files are independently decoded and verified. These are not listening or
independent DSP claims. The native run uses offscreen Metal and does not
measure physical display latency.

The native input-to-state p99 is 1.565791 ms over 330 samples. Input-to-commit
p95 is 85.747709 ms over nine; the complete ordinary-edit 50 ms target remains
open. Retain both layout findings: 51 second-retry frames and seven runs of
three or more consecutive retrying frames with distinct causes.

## Current-format first-pause measurement

Five fresh-package pairs use 10,000 Background/Silence Holds, fixed seed 7 and
two committed pauses per package. Baseline and candidate order alternates.
Each edit starts below load 3 (observed 1.982910–2.068848); no load flags occur.
Both binaries are uninstrumented, so phase multiplicity is explicitly not
applicable. This measures Store commit plus workspace refresh without a picture.

| Binary / edit | n | Min ms | Median ms | Max ms |
|---|---:|---:|---:|---:|
| Baseline first | 5 | 130.485 | 130.636 | 131.493 |
| Core 57 / SQLite 86 first | 5 | 113.103 | 114.691 | 115.853 |
| Baseline second | 5 | 32.500 | 33.206 | 34.673 |
| Core 57 / SQLite 86 second | 5 | 32.547 | 33.265 | 34.175 |

The paired workload's first median improves by 15.945 ms, about 12.2%.
It still exceeds 100 ms before picture presentation. These short diagnostics
establish neither a p95 target nor the visible Hold-insertion bound.
After two pauses both variants have 10,009 nodes and 120,024 frames.
The measured document JSON shrinks from 15,611,962 to 12,319,659 bytes;
the reported database size shrinks from 17,404,896 to 13,592,672 bytes.
The earlier disposable Core56/SQLite85 prototype remains separate historical
evidence; these are the current-format results.

Exact executed SHA-256 identities:

- Baseline perf: af725eba81e35a2a005a1b1e6c855684ff1a5e7328c7c093e86c3a5dc6ce6f28
- Current perf: d8c0cd0fb54c22f8b697ed7e4c9e4f7944f6ea59b0e55f103c58d2ab61e2e7fd
- Release app: cf5f0d1eb4f29180e1ad796cda5c12daf5c0eeff7d1aa0010c9bd5d05ce19079
- Release CLI: 0d1f25a7f9e6cd6f7ac14b26b81f96d8f56ada627f9423dd486b17b71182df44

## Controls, corrections and source attribution

The immutable a18ff654 witness fails exactly the two omission checks, while
four legacy, nondefault and malformed-input controls pass. Final codec tests
use asymmetric node, Source-placement and Repeat-gap policies. Fixed Automatic
and Hard-start/Automatic-end context bytes were emitted by the previous a18
Core56 library, SHA-256
532aa006494ea2c3e8050c1bf14d460d4c550f3ed3cf67b38992ae9b4832673b.
Both fixtures re-encode exactly for schemas 1–12. Linking the final six tests
against that unchanged old library again gives the same two expected failures
and four passing controls, including the fixed historical codec test.

Four obsolete fixture assumptions were corrected without production edits.
The policy tamper test materializes the omitted default object before changing
one forbidden policy. The byte-bound fixture keeps 12,000 nodes with longer
identities after an intermediate 14,000-node fixture correctly hit its separate
work budget. The historical range literal and database rows remain intact;
only expected current writer Values remove objects asserted exactly Automatic.
The history regression checks the complete initial timing/binding patch and
small later tables directly, retaining later row-size and final catalog bounds.

An initial baseline copy omitted twelve historical Rust fixture paths when
excluding bulky evidence. Its complete source guard stopped setup before
compilation; the repaired copy retains every original Rust path.
The original broad archive built, but its driver exited 120 before listing or
running tests after the tool session disappeared. The cause is not proven.
Its source/input hashes matched, and the archive was resumed with retained
stdout/stderr. Both the interrupted driver and failed fixture runs remain.

The initial xtask lint covered changed Core/Store packages. Earlier checkpoint
wording overstated that as ordinary/UI; the separate explicit UI Clippy run now
supplies that evidence. Three independent reviewers report no remaining
production, wire or test-quality finding.

Every phase retains its exact source/input and actual executable identities.
The full Core, broader, UI and media checks precede one final test-only
strengthening; its attribution proves that only compact_frozen_edges.rs changed.
The six stronger tests, Core Clippy, formatting and old-library witness then pass.
All production source remains identical, and current-format perf uses that final
test snapshot. No archived-source comparison shares a Cargo target directory.

## Evidence and remaining scope

[Retained evidence](../../tools/media-qualification/evidence/2026-10-23-compact-frozen-edges/)
contains 1,069 payload files / 46,387,246 bytes. Inventory SHA-256:
2848b62c95487b8c927b7b130faa040bc19fbfd085328d529d101fd0d57e9716.
Every entry was re-read and checked by size and SHA-256. Rust overlays use
.rs.txt. Executables, nextest archives and large benchmark SQLite packages are
excluded; exact binary hashes, commands and benchmark package hashes remain.
Generated media and both portable archives are retained with checksums.

The 203-file checkpoint is preserved as its earlier state. Future capture and
binding-default experiments are isolated in scratch next-profile-plan and are
excluded from this qualification. DP-24 remains partial; no requirement or
gate newly closes.
