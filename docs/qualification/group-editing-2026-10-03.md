# Named groups and development formats, 2026-10-03

Status: this increment's implementation, review and verification are complete.
No product gate is complete.

## Change

`,g` opens a captured name field. `:group name="the answer"` groups an explicit
child or nonempty Visual range into an ordinary Sequence. It retains the full
timing and owner context at partial endpoints. `:ungroup` promotes the children
of an explicitly selected neutral Sequence. Both select their result, preserve
copy registers and use one atomic Undo. Framed or treated Ungroup remains
unsupported and refuses explicitly.

Group and Ungroup share the semantic Apply and Macro planner. Recording retains
the selection rule and label rather than the old node identity. Dot resolves
that intent at the current selection. A counted Macro commits once. The
headless response reports final logical mark changes, including unresolved
marks hosted by a removed wrapper.

See the [Group contract](../GROUP_EDITING.md) for exact selection, text,
identity, mark and history behavior.

The same increment removes obsolete development migrations under the user's
explicit permission. Schema 55 remains current; schema 52 retains its additive,
backed-up upgrade. Other database schemas refuse before acquiring a writer,
creating a backup or repairing the package. Current recovery and frozen audio
context codecs remain required. See [supported formats](../DEVELOPMENT_FORMATS.md).

## Environment

Base: `5fd02ecc91822f3ea3469ee41959766c5f077705`.
Apple M5 Max, 128 GiB, macOS 26.5.2, Rust/Cargo 1.97.1.
Checks use locked dependencies and `/tmp/deadpan-ui-ffmpeg/prefix`.

Final source inventory:
`5faa89bdb6432ebca0cc2cb0887ac1cfc9c6a8aa07eb1b4bfa8d1c15c1b81f8e`.
Rendered and native executable:
`539ec96129c9188a937c3a7771e9335705f5253fdd5d79f826bcbb034fe474d1`.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-03-group-editing/metadata.json)
includes exact source inventories, commands, failed and corrected logs, replay
reports, review notes and native observations. `SHA256SUMS` covers the evidence
files. Selected captures show the
[command field](../../tools/media-qualification/evidence/2026-10-03-group-editing/screenshots/group-command-unicode.png),
[named group](../../tools/media-qualification/evidence/2026-10-03-group-editing/screenshots/named-group-default.png),
[minimum-size breadcrumb](../../tools/media-qualification/evidence/2026-10-03-group-editing/screenshots/group-breadcrumb-minimum.png)
and [keyboard help](../../tools/media-qualification/evidence/2026-10-03-group-editing/screenshots/group-help-minimum.png).

## Review

Independent reviews covered Group core timing, sound routing and marks;
host receipts, retry proofs and register preservation; and native command
capture, text/IME routing and stale targets. No production findings remained.
Separate reviewers checked the removed core adapters and retained runtime
codecs, and the store's current history validation, early refusals and isolated
schema-52 promotion. A follow-up corruption test checks valid JSON whose
history request disagrees with its retained patch and snapshot.

Removing the 32 obsolete full-document adapters eliminates 31,091 source lines
and avoids extending their closed command vocabularies. This is a maintenance
reduction. Build speed has not been isolated or measured; changing source and
feature graphs and warm build caches prevent a causal comparison.

## Execution results

The final default app configuration passes 747 tests and the optional
`ui-harness` configuration passes 783, including four headless integration
cases in each. Formatting, strict workspace Clippy and strict optional-app
Clippy pass with locked dependencies. The complete workspace run and its
corrected targets are described below; that failed run is retained rather than
reported as a passing workspace gate.

Six final rendered workflows pass 1,416 checks:

| Workflow | Checks |
| --- | ---: |
| Group/Ungroup | 329 |
| Dot-repeat | 381 |
| Macros | 372 |
| Repeat operator | 223 |
| Key remapping | 57 |
| Scoped Repeat/Retime editing | 54 |

Each run also passes 3,319,728 production shortcut cases against 62 live Kestrel
reservations, with no conflicts or source drift. The scoped replay covers the
shared inspector-title change in its other native editing context.

Visual comparisons use actual decoded pictures through the shared GPU compositor
at 320×180, independently of changing footer geometry. Source Group/Ungroup
and partial Repeat grouping preserve exact Original frames 20, 23 and 27 and
their distinct working-pixel hashes. Partial Preserve Retime grouping preserves
Original frames 10, 11 and 13 at those same Edit positions. The replay's
intermediate screenshot allowance is exhausted; all semantic checks continue,
and reserved named checkpoints remain captured.

Both new audio tests passed in the workspace run. They compare every raw and
authored f32 sample bit across exact Group/Ungroup operations, including a
12,813-sample NTSC Source with a separate 17-sample offset and a 30,430-sample
partial Repeat/Preserve Retime under framing and treatment. Fresh sessions and
shuffled 193-sample reads match the independently chunked 251-sample baseline.
The composite case retains a routed root sound and silent-Hold permission.

Image inspection confirms the Group command suffix, selected range, named
Sequence, breadcrumb and keyboard help at default and minimum sizes. The first
inspection found missing Japanese glyphs despite exact preserved text. Installed
Hiragino and Arial Unicode fallbacks now provide real glyphs in both families;
the final captures retain Japanese text in the field, title and breadcrumb.

The first run with actual fallback glyphs passed glyph admission and exact
editing checks, then exposed an inspector heading clip: the 17-point name's
paint mesh started three points above its allocated row. The label in the beat
card and footer remained visible. The strict paint-bound check is retained;
the corrected inspector allocates the measured glyph overflow.
The shared title helper measures that overhang and keeps the standard
Label's selection, accessibility and hover behavior. Three CPU regressions
pass: the unpadded title reproduces clipping, the padded ASCII/CJK matrix fits
its actual clips at two widths and 1, 1.25 and 2 pixels per point, and ordinary
ASCII geometry stays equal. The first matrix fixture incorrectly requested a
zoom change before its initial frame, which replaced the intended viewport
with egui's default bounds. It now supplies native scale and explicitly checks
the resulting viewport and scale. Production padding was unchanged by that
test-only correction.

Retained failures and corrections:

- The first workspace build found an untyped test frame count. It now uses
  the checked `FrameDuration` helper. Two unused legacy test helpers were also
  corrected without changing runtime codec behavior.
- The second workspace build found an old parser test assuming command entries
  were `Copy` and an incorrect nested CLI test module path. The test clones its
  expected entry, and the module now names its explicit relative path.
- The optional app build caught SHA-256 output formatting unsupported by the
  pinned digest type. The replay now uses the existing per-byte hex convention.
- Two app assertions needed correction: the dot hint now names the supported
  edits, and the extending Visual fixture now keeps its cursor at its head.
- The initial replay incorrectly placed IME composition in the opener's batch.
  The editor correctly blocks shortcuts in any composition batch. The corrected
  replay opens the field with its immediate text suffix, then sends Preedit and
  Commit in subsequent batches and verifies that each owns Enter.
- The next run exposed a real command-field bug: a previous `sequence` command
  left the caret at character 8. Opening `,g` and immediately typing inserted
  the name inside `group name=`. `open_command` now initializes fresh text state
  with the caret at the new prefill's end, also clearing the prior field's
  selection and undo history. The exact formerly failing replay now passes.
- The complete workspace run finished with 3,438 passes and five failed new
  assertions. Corrections retain the behavior being tested: the Visual cursor
  matches its head, CLI schema refusal uses its public `SchemaUnsupported`
  spelling, authored Retime clocks are distinguished from transparent crop
  Retimes, hidden mark fragments explicitly report `OutsideMapping`, and the
  backup path comparison resolves macOS `/var` and `/private/var` aliases.
  The affected targets now pass in full: 193 core unit tests, 35 CLI project
  command tests and 30 migration tests.
- The first font-test run dropped egui's texture updates without acknowledging
  them, triggering its debug lifetime assertion before glyph checks. The
  CPU-only helper now explicitly clears those updates, matching existing
  headless font/layout tests. All four font tests pass in the rerun; rendered
  atlas checks remain separate.
- Strict lint found a redundant `trim_start` immediately before
  `split_whitespace` in the Group footer hint. Removing it preserves parsing;
  the earlier trim before an optional colon remains necessary.

## Native verification

The native app copied the complete 120-frame beat to register `a`, grouped
`[0..7)` as `First 答え`, entered and left it with Enter/Backspace, then used dot
on `[7..12)` to create a second named group. Ungroup and one Undo preserved both
intervals. Ungroup dot resolved the other selected group. Register `a` and its
full-source copied range remained visible throughout.

After an Undo receipt settled, `qz` recorded one `:ungroup`; `q` saved it and
`@z` ran it with one Undo. Closed-project inspection retains two `First 答え`
groups, the historical edited capture in `a`, and exactly one `ungroup`
instruction in `z`. Cmd-Q exited with code 0 and a targeted process check found
no remaining Deadpan process.

CUA text typing delivered only the ASCII portion of the first name attempt;
native paste entered the exact Unicode string and the actual 2× native heading
was inspected. This is not OS IME or physical keyboard qualification. A recording
started in the same input batch as Undo did not survive the revision change;
the checked recording waited for the saved receipt. Native QA overlapped compiler
work and makes no response-time claim.

## Release timing

Release executable:
`dcdd36b1b1e77666b253c8b07b9b25fa5d8c0f24392f6ca2020f0db1ff50839e`.
The Group performance replay passes without failed or timed-out measurements.
No owned build, other replay or native QA app ran concurrently. GPU readback
comparisons are disabled in performance mode.

| Measurement | Samples | Median ms | p95 ms | Maximum ms |
| --- | ---: | ---: | ---: | ---: |
| UI frame CPU | 891 | 0.219 | 0.685 | 4.638 |
| Input to state | 359 | 0.365 | 0.750 | 4.638 |
| Input to commit | 22 | 5.115 | 6.994 | 77.392 |
| Picture request to GPU completion | 97 | 1.407 | 1.965 | 6.125 |
| Input to picture completion | 74 | 1.619 | 7.077 | 83.521 |

These are scripted mixed interactions on the small `cfr-bframes.mp4` fixture,
including first import/index and later warm operations. The replay advances as
work completes; its configured event clock does not measure physical display
delivery. OS cache, power and thermal state are uncontrolled. This does not
qualify large projects, acoustic playback, memory pressure or full performance
acceptance. The startup cost of installed-font loading was not isolated.

## Limits

Ordinary Sequence grouping does not complete editing inside temporal
occurrences, framed or treated Ungroup, saved gag recipes, or the full slice
placement workflow. Synthetic input does not establish physical keyboard or
operating-system IME behavior. GPU comparisons do not establish physical
display latency or color qualification. Complete acoustic, performance,
packaging and product acceptance remain open.
