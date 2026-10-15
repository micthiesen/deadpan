# Preview-versus-export verification: Section 8 rows, 2026-10-15

Scope: twenty new recipe fixtures for Section 8 rows whose **Export verified**
cell was "No", added to the [preview/export harness](../PREVIEW_EXPORT_VERIFICATION.md)
in [section8.rs](../../crates/deadpan-cli/tests/preview_export/section8.rs).
Each is a one-Original project from `cfr-bframes.mp4` built only through public
headless paths (`command`, `macro` save/run/apply/`yank_original`, `sound`,
`project insert-original`), exported through public `render` and compared with
its committed revision by `verify-export`. Expectations (picture provenance,
256-sample loud/quiet windows of the limited bus, left-channel peak windows)
are derived by hand from the recipe semantics in each fixture's comment. This
is fixture evidence for the automatic SDR path on one machine. It is not native
key-path, release, HDR, device playback or listening qualification.

## Environment and source identity

- Apple M5 Max, 128 GiB, arm64, macOS 26.5.2 (25F84); Metal backend; automatic
  encoder policy `AutomaticSdrV1`. Rust 1.97.1.
- Pinned FFmpeg prefix
  `~/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix`.
  Test-only tools: Homebrew `ffmpeg` 9.0.1 with `libx264` (large and Generated
  fixtures) and release `deadpan-media-worker` and `deadpan-track` beside the
  tested `deadpan-cli` in `CARGO_TARGET_DIR=target/agent-export`
  (`deadpan-cli` sha256 `b4e2291759661f9e…`, `deadpan-media-worker`
  `68421a616e0171b0…`).
- Git HEAD `12f1a84cc636467eb1d139f71ed31136a91a4cbc` with a dirty shared tree.
  This change touches only `crates/deadpan-cli/tests/preview_export.rs`,
  `crates/deadpan-cli/tests/preview_export/recipes.rs`, the new
  `crates/deadpan-cli/tests/preview_export/section8.rs` and these docs. Another
  agent had uncommitted changes in `crates/deadpan-core` (audio binding,
  lineage, exact clocks, derived clocks, `insert_time` including a new
  `retimed.rs`, `command.rs`, `semantic/planner/pause.rs`), `crates/deadpan-plan`
  (`audio_reference.rs`) and tests in `deadpan-audio`, `deadpan-plan`,
  `deadpan-store`; the build included them. These results apply to that tree.
- Command:
  `DEADPAN_PREVIEW_EXPORT_RESULTS=/tmp/… cargo test --release --locked -p deadpan-cli --features synthetic-worker --test preview_export -- --nocapture`.
  33 tests passed in 418.1 s (10 min 55 s wall including the build). All 66
  recipe fixtures were built and passed, none skipped (`synthetic-worker`
  enables the four Generated Hold fixtures; without it they are reported
  SKIPPED). Several tests in the binary write the same results path, so the
  table below comes from the `every_recipe_export_matches_its_committed_preview`
  rows printed with `--nocapture`.
- `cargo clippy -p deadpan-cli --all-targets --locked -- -D warnings` (with and
  without `--features synthetic-worker`) and `cargo fmt --check` for the
  changed files are clean. `cargo fmt --check` reports unformatted files only
  in the other agent's uncommitted paths.

## Fixtures

Unless noted, the Original is first shortened to Original frames [12, 42)
(Edit `f` shows Original `12 + f`; the click at Original sample 48,000 sounds
at Edit sample 28,781).

| Fixture | Rows | Construction | Frames |
| --- | --- | --- | --- |
| false-start | False start | `yank_original` b = [24, 33), a = [24, 27); `s` at 15; apply `"bp` after the first half, `"aP` before b, `3l`, `,h 6f`: a, a freeze of Original 26, then b | 48 |
| interrupted-answer | Interrupted answer | `s` at 15, `delete_range` [22, 30), `set_cutaway` register r (Original 90..96) on the remaining answer | 22 |
| callback-register | Callback, Paste from register, Registers | apply Visual [12, 24) `"ay`, clear, `G`, `"ap` | 42 |
| reaction-ping-pong | Reaction ping-pong | `set_cutaways` [3, 8) 90..95, [13, 18) 100..105, [22, 27) 90..95 on one host, sound kept | 30 |
| abrupt-return | Abrupt return | `s` at 15, static 1.35x on both halves, apply `set_framing null` (`:zoom off`) on the second | 30 |
| long-answer | The Long Answer, Versioned definition | `:gag long-answer pause=12f creep=1.35` at 15 (macro run); label round-trips through `GagRecipe::from_label`; version 2 refused by a dry-run apply | 42 |
| escalator | The Escalator, Versioned definition | `:gag escalator plays=3 gain-step=3dB zoom-step=0.08` on Edit [12, 24); label round-trips | 54 |
| non-sequitur | The Non-Sequitur, Paste from register | `s` at 15, `:gag non-sequitur register=r` (Original 90..96) after the first half | 36 |
| named-group | Group / ungroup, `,g`, `:group name=…` | apply Visual [10, 20) `group` "the uncomfortable answer", then 1.35x zoom and -12 dB trim on it; Visual [22, 26) `group` "aside" then `ungroup` | 30 |
| dot-repeat | Dot-repeat of creative edits | the same `insert_pause 6f` apply submitted at Edit 10 and again at Edit 22 (PARITY: `.` resubmits the same apply instruction) | 42 |
| reuse-original | `,i` reuse Original | `project insert-original --parent root --index 1`: the whole 120-frame Original after the base | 150 |
| sound-place | `,s` place sound | catalog WAV registered, then `sound` `{"type":"place","at":{"frame":10}}` (shared `sound_events::placement`) | 30 |
| trim-out | `,v` Trim, `:trim edge=out delta=-3f mode=ripple` | `s` at 15, one `apply_source_trim` Out -3 Ripple with the literal right sibling | 27 |
| slip | `:slip +5f` | `s` at 10 and 20, `slip_source` +5 on the middle beat | 30 |
| roll | `:roll +2f` | Original [10, 29) + [60, 71), `roll_sources` +2 at the Edit 19 seam | 30 |
| scoped-play | Per-play overrides | Edit [12, 24) wrapped as 3 plays; two `edit_scoped` edits on play 2 only: -12 dB trim, static 1.35x | 54 |
| macro-call | Semantic macros | Macro p = `,h 4f video=black`; Macro m = `@p`, `10l`; `2@m` from Edit 5 (one Compound) | 38 |
| freeze-detail | Freeze a detail, Frozen stare | apply `,h 12f` at 15 then `set_framing` static 2x at (0.30, 0.35) on the new Hold | 42 |
| local-recipe | Save modified group as local recipe, Registers | `:gag one-more-time plays=2 gap=6f shorten=1f` on [12, 24), -12 dB trim on the group, `"ayag` (`:recipe-save a`), `G`, `"ap` (`:recipe a`) | 78 |
| detach-gag | Detach from template, Group / ungroup | `:gag one-more-time plays=3 gap=12f shorten=6f` on [12, 24), then `ungroup` of the gag group | 72 |

Independently derived checks before export (plan provenance and
`inspect-audio --limited`), besides every listed picture:

- false-start: the prefix and the freeze are quiet; b holds the click 9,562
  samples after B(24), at 48,000; the base click moves 18 frames to 57,610.
- interrupted-answer, reaction-ping-pong: the click (Edit 17.97) still sounds
  under the reaction pictures.
- callback-register: the copy's click sounds at B(30) + 9,562 = 57,610.
- long-answer, freeze-detail, dot-repeat, macro-call: the pauses are silent and
  the click moves by exactly the inserted frames (to 48,000, 48,000, 48,000 and
  41,594).
- escalator, detach-gag: each play's click 9,562 samples after its start, gaps
  quiet.
- non-sequitur: the pasted moment holds no click; the base click moves 6
  frames, to 38,390.
- named-group, scoped-play, local-recipe: the -12 dB trim gives a left peak in
  (0.17, 0.21) for the click it owns (about 0.188), while unedited plays stay
  loud: only play 2 in scoped-play; all four plays of the original and the
  pasted local recipe in local-recipe.
- reuse-original: both Original clicks sound again B(30) = 48,048 later
  (96,048 and 240,040).
- sound-place: the derived event has offset 16,016 = B(10), loud from there.
- trim-out: the click moves to Edit 14.97 (23,976) and nothing sounds at its
  old place; slip: it moves to 20,773; roll: it sounds at 31,984, where the
  right beat's silent Original 60.97 played before the Roll.

## Results (optimized build, every frame and every audio window)

| Fixture | Pictures | Min Y PSNR dB | Min Cb/Cr PSNR dB | Max thumbnail MAD | Min neighbor margin dB | Audio windows (signal) | Gated blocks | Min block SNR dB | Max block level dB | Offset status | Render s | Verify s | Passed |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| false-start | 48 | 56.5 | 53.4 | 0.050 | 0.0 | 2 (2) | 3 | 49.0 | 0.01 | verified_zero ×2 | 3.21 | 1.05 | yes |
| interrupted-answer | 22 | 56.5 | 53.4 | 0.050 | 0.0 | 1 (1) | 1 | 55.2 | 0.01 | verified_zero | 2.07 | 0.45 | yes |
| callback-register | 42 | 56.5 | 53.4 | 0.050 | 32.3 | 1 (1) | 2 | 37.7 | 0.04 | verified_zero | 2.26 | 0.82 | yes |
| reaction-ping-pong | 30 | 56.5 | 53.4 | 0.050 | 32.3 | 1 (1) | 1 | 55.2 | 0.01 | verified_zero | 2.00 | 0.58 | yes |
| abrupt-return | 30 | 50.7 | 55.3 | 0.083 | 28.0 | 1 (1) | 1 | 55.2 | 0.01 | verified_zero | 2.02 | 0.54 | yes |
| long-answer | 42 | 55.2 | 53.4 | 0.050 | 23.0 | 1 (1) | 1 | 40.3 | 0.03 | verified_zero | 2.08 | 0.63 | yes |
| escalator | 54 | 54.6 | 53.4 | 0.050 | 30.5 | 2 (2) | 3 | 40.5 | 0.03 | verified_zero ×2 | 2.46 | 1.12 | yes |
| non-sequitur | 36 | 56.5 | 53.4 | 0.050 | 32.3 | 1 (1) | 1 | 36.3 | 0.02 | verified_zero | 2.33 | 0.69 | yes |
| named-group | 30 | 56.5 | 53.4 | 0.050 | 32.3 | 1 (1) | 1 | 55.1 | 0.00 | verified_zero | 2.47 | 0.66 | yes |
| dot-repeat | 42 | 56.5 | 53.4 | 0.050 | 0.0 | 1 (1) | 1 | 55.1 | 0.01 | verified_zero | 2.75 | 0.86 | yes |
| reuse-original | 150 | 56.5 | 53.4 | 0.050 | 32.3 | 5 (4) | 5 | 30.8 | 0.02 | verified_zero ×4, not_applicable | 4.51 | 2.73 | yes |
| sound-place | 30 | 56.5 | 53.4 | 0.050 | 32.3 | 1 (1) | 19 | 7.9 | 0.68 | verified_zero | 2.45 | 0.79 | yes |
| trim-out | 27 | 56.5 | 53.4 | 0.050 | 32.3 | 1 (1) | 1 | 55.6 | 0.00 | verified_zero | 2.31 | 0.52 | yes |
| slip | 30 | 56.5 | 53.4 | 0.050 | 32.3 | 1 (1) | 1 | 53.4 | 0.00 | verified_zero | 2.00 | 0.55 | yes |
| roll | 30 | 56.6 | 53.6 | 0.053 | 34.7 | 1 (1) | 1 | 52.4 | 0.01 | verified_zero | 1.80 | 0.50 | yes |
| scoped-play | 54 | 51.4 | 53.4 | 0.050 | 30.5 | 2 (2) | 3 | 54.5 | 0.01 | verified_zero ×2 | 2.37 | 1.07 | yes |
| macro-call | 38 | 56.5 | 53.4 | 0.050 | 0.0 | 1 (1) | 1 | 60.1 | 0.00 | verified_zero | 2.34 | 0.75 | yes |
| freeze-detail | 42 | 56.5 | 53.4 | 0.050 | 0.0 | 1 (1) | 1 | 40.3 | 0.03 | verified_zero | 2.49 | 0.76 | yes |
| local-recipe | 78 | 56.5 | 53.4 | 0.050 | 0.0 | 3 (3) | 4 | 31.9 | 0.09 | verified_zero ×3 | 3.30 | 1.66 | yes |
| detach-gag | 72 | 56.5 | 53.4 | 0.050 | 0.0 | 2 (2) | 3 | 38.2 | 0.04 | verified_zero ×2 | 2.89 | 1.39 | yes |

Every movie declared an audio edit media time of 2,048 (encoder ABI 2) and
passed the edit, color, PTS-grid and gap checks. A 0.0 dB neighbor margin
comes from static runs (freezes, black pauses, held cutaway pictures) whose
neighbors are identical. The 46 existing fixtures (including the four
Generated Holds and both large Originals) passed in the same run.

## Framing negatives

The picture comparison shares one picture session with the encoder, so a
framing bug would appear on both sides. Two negatives check that the exported
pixels carry the authored framing: after export, a later revision changes only
one beat's framing and the movie is verified against it (`--frames
0,14,15,20,26,27,29 --no-audio`). Exactly that beat's frames must fail.

- abrupt-return against a revision that zooms the second half back to 1.35x:
  frames 15, 20, 26, 27 and 29 fail `luma_psnr`, `chroma_psnr` and
  `gross_structural_mismatch`; frames 0 and 14 (still zoomed in both) pass. The
  export really returns to the full picture at Edit 15.
- freeze-detail against a revision that removes the Hold's framing: frames 15,
  20 and 26 fail the same three gates; 0, 14, 27 and 29 pass. The exported
  freeze carries the magnified detail, and only on the Hold.

## Rows skipped

None of the requested rows was skipped: every construction is reachable through
the headless API ([PARITY.md](../PARITY.md)). Limits of what each fixture shows:

- Dot-repeat: headless `.` is a resubmission of the same `apply` instruction;
  the native dot intent bookkeeping (`RepeatableEdit`) is not exercised.
- Semantic macros: programs are saved explicitly (`macro save`), not recorded
  with `q`; the counted run and nested `@p` call go through the native planner.
- Trim, Slip and Roll commit the commands the native panels apply
  (`apply_source_trim`, `slip_source`, `roll_sources`); panel pictures,
  waveform and audition are interactive and untested here.
- Freeze a detail uses a static Camera pose on the Hold, not a tracked region
  target.
- Registers use `yank_original` and edited `yank`/`yank_selection` into named
  registers within one project; reopen and cross-project transfer are not
  covered.

## Findings

No preview/export mismatch was found and no product defect surfaced. Three
headless contract details shaped the fixtures and are recorded here because
they are easy to get wrong:

- The semantic `paste` instruction places after (or, with `before`, before)
  the selected direct child; it does not split at the cursor and refuses
  without a selected beat ("select a beat before pasting"). Interior pastes
  (False start, Non-Sequitur) therefore split first, as `s` would.
- A pasted edited group arrives as a "Copied contents" root holding the group,
  which keeps its pinned recipe label.
- After a scoped edit isolates a play, later `edit_scoped` targets must name
  that play's override root (`ProjectDocument::overrides`); naming the shared
  definition child is refused ("scoped target is outside the selected play's
  owned branches").
