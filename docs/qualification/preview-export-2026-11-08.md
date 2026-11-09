# Full preview/export run, 2026-11-08

`DEADPAN_PREVIEW_EXPORT_RESULTS=… cargo test --release --locked -p deadpan-cli
--test preview_export every_recipe_export_matches` at `67b8c15c5`, on this
Mac (physical Apple Silicon), built without `--features synthetic-worker`.

Result: 76 rows. 71 fixtures passed, 4 were skipped (`generated-pause`,
`generated-repeat`, `generated-reframe`, `generated-prefix`: they need the
synthetic AI worker feature, which this build omitted), and 1 failed.

The four fixtures added today all passed with zero-offset audio, luma PSNR at
least 56.45 dB and no flagged picture:

| Fixture | Frames | Operation |
| --- | ---: | --- |
| `ramp` | 25 | stepped `:ramp 1 3 steps=3 pitch=tape` |
| `role-repeat-across` | 28 | `:repeat 2 role=audio overflow=trim` across a real cut |
| `video-repeat-across` | 28 | `:repeat 2 role=video` across a real cut |
| `audio-shift` | 28 | `:audio-shift +5f overflow=trim` across a real cut |

The failure was `scoped-audio-chain`: `audio [0, 48000): audio_offset_unobservable`.
Its first default window starts in the fixture's exact 1 kHz Bleep tone, whose
correlation maxima are periodic, so the verifier correctly reports the offset
as unobservable rather than zero. The fixture's dedicated test
(`scoped_audio_chain_preview_and_complete_encoded_output_isolate_one_nested_play`)
passed in the same tree with six contiguous windows covering all 115,315
samples, each `verified_zero`, so the exported audio is correct. The fixture
now declares those six windows (`Fixture::verify_args`) for the all-recipes
run too; a focused all-recipes rerun of `scoped-audio-chain` passes with six
`verified_zero` windows. The full suite was not rerun after that change.

Not covered here: the synthetic-worker AI fixtures (run with the feature),
HDR (`preview_export_hdr`), and any physical display or listening judgement.
