# Nested fragment deletion, 2026-09-30

## Result and scope

`DeleteRange` now admits partial Source and ordinary Hold endpoints behind two
or more unity Partition windows. It uses the same bounded endpoint traversal as
edited-slice placement and retains the complete physical owner and intermediate
effect contexts through Split. The public query, typed command, stored format
and inverse are unchanged. Core 34/database 43 remain unchanged.

Original sample lattices are still captured before either endpoint is split,
and surviving suffix entries before removal. Root sounds receive one deletion
transform. The ordinary Sequence scope, exact identity budget, temporary node
limit, checked timing ordinals and final document validation remain in force.
Partial Repeat, general Retime and Generated Hold endpoints remain unsupported.
Historical Source replacement, standalone Split and InsertTime keep their
existing admission rules.

This prepares the deletion side of cut-to-register. It does not publish deleted
contents to a register or implement empty structural capture/paste.

## Decisive checks

- An isolated source copy using the previous two production files from
  `344ec32f243d8f5ccc45cecc7596a089ce0d827f` fails the new untreated two/three-window
  test at endpoint admission. The current code passes it. The shared checkout
  was never rolled back; retained baseline inputs identify the exact old files
  and new test snapshot.
- Nine new core tests cover exact split pools, same/different endpoint owners,
  complete framing/gain and Freeze contexts, biased and unresolved marks,
  root sounds, rejected identities/ranges/timing exhaustion, unsupported
  providers, and unchanged admission of other commands. Existing deletion tests
  also pass, including the temporary node limit and full inverse/serialization.
- Two picture-plan tests check independent Source ordinals/PTS, Freeze identity,
  captured geometry and exact framing clocks through two/three windows. Framed
  and unframed variants cover retained owner effects and the newly admitted
  family. These are indexed-plan checks, not decoded/GPU pixel measurements.
- Four decoded-PCM tests use the qualified 44.1 kHz fixture at 30000/1001 fps.
  They independently reconstruct Source and RoomTone coordinates, check retained
  samples exactly on cold, reversed and warm reads, verify gain application and
  root sound routes, and distinguish an extra allocated sample with continuing
  physical support from one beyond exhausted support.
- The new native service test deletes inside three windows in a nested ordinary
  Sequence. SQLite records exactly one `DeleteRange` with the preflight identity
  count; the receipt reports the exact join and successor. Reopening preserves
  the result; Undo and Redo restore complete authored state with fresh revisions.

The focused runs pass 18 core deletion tests, two picture-plan tests, four PCM
tests and four native service tests. No tests are ignored in those runs.

## Corrections and evidence limits

The first core run passed 17 tests and rejected one new sound fixture because
its selected mapping used 100 frames instead of the exact natural duration
`100000/1001`. Correcting the fixture made all 18 pass; production code did not
change. The first PCM run passed two tests and rejected two reference calls that
exceeded the helper's 256-frame block limit. Batching that independent reference
with exact integer phase offsets made all four pass without changing expected
coordinates or production code.

The first picture run passed before the unframed variants were added; a separate
final run covers those variants. An initial test comment incorrectly described
treated nested chains as newly admitted. Treated chains already passed the old
admission and are preservation regressions. Untreated core, picture, terminal
PCM and root-sound cases exercise the extension.

The native service run retains the existing debug linker warning about the
oversized `__eh_frame` section. It completed successfully. This change adds no
UI controls or layout, so painted replay, device listening, native key delivery,
IME, accessibility and release packaging were not rerun.

The user clarified that the separate Cursor QA window belongs to agent testing.
It was quit through the native UI, and the app inventory then contained no
Deadpan instance. Future native tests may reuse it and must close idle instances.

## Final verification

Independent static review found no defects in endpoint admission/counts,
retained contexts/clocks, other command admission or native persistence. The
parent reviewed the complete change and the independent picture/PCM oracles.

Final checks completed on 2026-10-01 on an Apple M5 Max (`Mac17,7`), macOS 26.5.2
build 25F84, using Rust/Cargo 1.97.1 and the pinned FFmpeg development prefix
`/tmp/deadpan-ui-ffmpeg/prefix`:

| Check | Result | Elapsed |
| --- | --- | --- |
| `cargo fmt --all -- --check` | Pass | 1.46 s |
| Locked workspace/all-target Clippy with `deadpan-app/ui-harness`, `-D warnings` | Pass | 643.74 s |
| `cargo test --workspace --locked --no-fail-fast` | 2,823 passed, 0 failed, 0 ignored | 1,226.77 s |

Every Cargo invocation used `rustup run 1.97.1`. These are check durations, not
editor performance measurements. The full workspace run covers the final source;
the focused UI-feature service run covers the same production/app test code and
predates only later test changes in other crates.

The [retained evidence summary](../../tools/media-qualification/evidence/2026-09-30-nested-delete/summary.json)
includes commands, source manifests, compressed logs, initial failures, baseline
inputs and review reports. Its checksum manifest covers 42 retained files. The
collector rechecked all 1,348 inputs against final source manifest
`6fe99501eec223fc725bb1edd7844b80c23e15c5834e680a4430b3fbe38d53cb`.
