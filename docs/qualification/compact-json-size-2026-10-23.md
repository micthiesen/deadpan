# Exact compact JSON size counting, 2026-10-23

The five-file change is implemented and qualified. Store now counts exact
compact document bytes for size-only work while retaining actual JSON for
captured checkpoints. All eight main stages and all 26 post steps pass.
Five quiet control/candidate pairs give offscreen composition medians
268.773292 /261.593583 ms, a 7.179709 ms difference of about 2.67%.
These diagnostics establish neither percentiles nor a performance-target PASS.

## Implementation and authority

`ProjectDocument::compact_json_size` uses the identical compact serializer,
64 MiB bound and error precedence as `to_compact_json`, without retaining
its complete output buffer. It does not validate or cache a size. Existing
field serializers may still allocate; peak memory was not measured.

Store uses the counter for entry, final and uncaptured staged documents.
Captured stages still retain the original JSON bytes. Serialization/counting,
processed-byte charge, step bound, captured-checkpoint charge and insertion
keep their original order. All Core and Store work charges, capture/media
admission, guards, history-reader roundtrips and inverse checks remain.
Core 58 and SQLite 87 are unchanged.

The source base is `91e17fe4db1aa44ccc63b3e27c70f6e6f7baa665` plus the five
frozen overlays. The complete 2,050-file Rust identity map and six input
identities remain unchanged through qualification. The five files exactly
match the formatted [private prototype](compact-json-size-prototype-2026-10-23.md);
no source correction followed its twelve passing focused tests. Both current
source reviews report no findings.

## Verification

| Check | Result |
| --- | --- |
| Core | 1,517 passed across 69 binaries, zero skipped |
| Broader selected suites | 3,506 passed, 740 skipped or unselected |
| UI feature suite | 1,296 passed, eleven skipped; one leak warning |
| Strict checks | Changed Core/Store Clippy, explicit UI Clippy and formatting pass |
| Debug/release neutral media | 15 pictures and 24,024 canonical samples, zero offset |
| Release aligned sound scope | 19 pictures and 30,430 final canonical samples, zero offset |
| Native workflows | Six scenarios, 1,340 workflow checks plus six Kestrel checks |
| Portable copies | Both nine-file packages validate/doctor with paths and bytes unchanged |

All four new regression cases are selected and pass in the complete archives.
They cover escaped/UTF-8 lengths, exact limits and overflow, nondefault nested
bindings, and persisted captured/uncaptured checkpoint equality with Undo/Redo,
reopening and register checks. The [earlier Core checkpoint](compact-json-size-core-checkpoint-2026-10-23.md)
remains separate historical evidence.

The UI warning came from
`project::tests::render::unchanged_preview_cannot_reuse_an_earlier_commit_receipt`.
It passes an isolated rerun from the same archive without the warning, with
hardware/synthetic requirements and source/binary guards preserved. The original
warning is retained. Broad tests report no leak warning.

Native reports retain second-retry frame counts 51/115/30/61/22/31 and
distinct-cause retry-run counts 7/32/2/8/2/5 for sound-group-endpoints, macros,
named-registers, dot-repeat, range-set-selectors and transcript-search-selectors.
These are warnings; no check fails. The media reports retain complete canonical
PCM/history controls and explicitly leave independent DSP, listening and
ordinary Move PCM-translation claims false.

## Diagnostic timing

Each trial uses a fresh 10,000-Hold package and edits only a private portable
copy. All seven first-pause checks pass, with the matching new revision and
picture ticket through offscreen Background composition. Complete authored
input maps remain unchanged except the two SQLite coordination sidecars;
WAL length is zero. Source/input/executable guards pass and pre-run load1 is
at most 3. These samples are separate from the private prototype batch.

| Boundary | Control median | Candidate median | Difference |
| --- | ---: | ---: | ---: |
| Input to commit | 266.145459 ms | 259.015416 ms | 7.130043 ms (2.68%) |
| Input to offscreen Background composition | 268.773292 ms | 261.593583 ms | 7.179709 ms (2.67%) |

Executed candidate app SHA-256:
`c36b894466eb4f16fe976924b05c02ada87b0e8872f7f9a7fc6028a60a342037`.
Qualified control app:
`1864c3acd798d7ce68a8d3a62ee144ec6956cdc4750c568bf0001d7c2a67be4c`.
The 100 ms fallback and 50 ms ordinary-edit targets remain open. No physical
display, source-decode, PCM-performance or calibrated-overhead claim follows.

## Evidence

The [retained package](../../tools/media-qualification/evidence/2026-10-23-compact-json-size/)
contains 467 files /15,051,302 bytes. Inventory SHA-256:
`dda2a29542a845ca25e58341ca9a1c90173434a2d552b3aefc29ac25791a47e3`.
Root independently verified every path, byte length, hash and gzip stream.
It includes all three verified emitted movies, decoder/control reports,
logs, receipts, manifests, reviews, exact sources and ten timing trials.
Executables, source videos, Cargo archives/targets and project packages are
excluded. Later build phases legitimately replace shared target executables;
the retained executed copies all match their phase hashes, as recorded by
the helper audit. The final candidate source/copy also match.

All drivers in `/tmp/deadpan-compact-json-size-qualification-2026-10-23` are
terminal. Do not restart them. The private prototype retains its interrupted
pre-test cache copy and corrected pre-output packaging lookup separately;
neither changes this production result. No requirement or delivery gate closes.
