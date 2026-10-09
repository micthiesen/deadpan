# VFR recipe timing and export, 2026-10-09

DP-02, §4's exact source clocks and §8's creative recipes. Eleven projects now
exercise variable-duration pictures through real commands, canonical preview
preparation, public Render and emitted-file verification. The matrix found a
reverse/bleep phase bug and an incorrect ping-pong endpoint rule. Both are fixed.
DP-02 remains partial: this is a measured subset of the complete source-format
and creative-operation matrix.

## Failure and correction

The first plan run failed at `vfr-reverse-hiccup` output frame 21: Original
ordinal 16 appeared where independently derived ordinal 15 was required. The
requested passage was Original project frames `[24,32)`, but its final decoded
VFR picture continued to project boundary 33. The old provider reversed from
that decoded interval's end, shifting the passage by one project frame. The
bleep provider similarly started at its first decoded picture's PTS, which
could precede the requested project passage. Ping-pong removed a decoded
picture interval rather than one project-frame sample.

`HoldVideo::Reverse` and `HoldVideo::Play` now retain an optional exact rational
source-tick origin at Hold-local boundary zero. Their measured source span
still bounds eligible decoded pictures; it no longer implicitly rounds the
captured clock. The host records the requested passage's exact end for reverse
and start for bleep. Ping-pong moves the end back by exactly one project frame.
Generic providers with no origin default to the former span end/start meaning.
Points outside the selected span hold its adjacent picture. Coverage queries
retain both leading and trailing held intervals, including fractional origins.
Document validation checks source-clock arithmetic before admitting an edit.

Core format 48 and SQLite schema 76 retain this authored timing. Under the
existing unused-development-format authorization, older schemas through 75
are refused before writes, document parsing or writer acquisition. No migration
rewrites existing development packages. Audio keeps its existing exact source
sample capture and canonical preparation; no encoder tolerance changed.

## Fixtures and independent oracle

The committed files have 120 decoded pictures at time base `1/30000`, with
durations 1001, 2002 and 3003 ticks in a repeating cycle. `vfr.mp4` has the
retained legacy 1001-tick terminal interval; `vfr-long-terminal.mp4` has the
independently corrected 3003-tick interval. Both import as 240 project frames
at 30000/1001 fps from their A/V union. The legacy file's last two project
frames explicitly hold its final picture while audio continues.

| Fixture | SHA-256 |
| --- | --- |
| `vfr.mp4` | `3fd07d00aa604554dc5389530cd261ac8961aded6361426fb00bf5519ac64884` |
| `vfr-long-terminal.mp4` | `e92b382d5d9aacde3193651dc0c6d3afa3adfa7f3174fcfd1ccff438938aa597` |

The test's PTS table is authored directly as
`start(i) / 1001 = 6 * floor(i / 3) + [0,1,3][i mod 3]`. It never calls the
production index selector to derive expected ordinals. Manually flattened
recipe clocks choose each expected Original picture or Background at every
output frame. Project creation, cuts, semantic recipes and typed commands use
the real CLI; read-only store access retrieves identities. The test CLI helper
also now uses the shared guarded subprocess spawn boundary.

| Recipe | Output pictures | Audio windows | Signal windows, all zero offset |
| --- | ---: | ---: | ---: |
| Legacy terminal Original | 240 | 8 | 3 |
| Long terminal Original | 240 | 8 | 3 |
| 15-frame freeze and linked audio resume | 45 | 2 | 1 |
| Three-play Repeat and two six-frame gaps | 66 | 2 | 2 |
| Nested half-speed Preserve / Repeat | 69 | 2 | 2 |
| Half-speed Preserve | 42 | 1 | 1 |
| 1.5× Tape | 26 | 1 | 1 |
| Eight-frame reverse hiccup | 38 | 1 | 1 |
| Twelve-frame ping-pong with eleven inserted frames | 41 | 1 | 1 |
| Picture-only cutaway bounce | 30 | 1 | 1 |
| Bleep preserving the selected picture clock | 30 | 1 | 1 |
| **Total** | **867** | **28** | **17** |

All eleven exports pass exact picture count, frame rate, reference provenance,
finished-file timing, pixel fidelity and audio checks. Minimum luma/chroma
PSNR is 54.6792 / 57.2544 dB. The lowest informative audio block SNR is
13.4933 dB in the nested Preserve recipe; maximum block level error is
0.1751 dB. Declared AAC priming is 2048 samples in every export. Quiet windows
remain `not_applicable` for offset instead of claiming a measured zero.

## Local verification and retained evidence

Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1, pinned LGPL FFmpeg 8.0.3.
The sole working agent implemented the change and reviewed the complete diff.

- Default VFR plan matrix: all eleven pass, 36.48 seconds.
- Picture-plan suite: 96 tests pass, including JSON roundtrip and leading/trailing
  endpoint coverage for both directions with fractional origins.
- Optimized actual-export matrix: all eleven pass, 34.88 seconds including
  fixture creation and plan checks; render/verification itself totals 24.18 seconds.
- Workspace with `deadpan-app/ui-harness` and required synthetic workers: all
  5531 runnable tests pass, including 1096 app tests. The initial run passed
  2351 tests and stopped on `doctor`'s stale core-47/database-75 assertions.
  Updating those to 48/76 and running only unfinished coverage passed the
  remaining 3180 tests. An inventory audit confirms no runnable test is missing.
  Eleven explicit ignored tests remain outside that default gate; the new VFR
  export test is one of them and passed separately in release mode above.
  Prior-format refusal through schema 75, current-schema read-only migration
  and an added authoring-time overflow regression all pass.
- Workspace and UI-feature strict Clippy, rustfmt, both doc tests and diff
  whitespace checks pass.
- `cargo xtask replays --scenario hold-effects`: 50 checks pass, no failed or
  skipped scenario, 100.1 seconds after a 29.5-second build/copy. This enters
  reverse, ping-pong, tails, lift and bleep through the production input router
  and service, and checks saved state and Undo. Inspected offscreen captures
  `hold-effects-023.png`, `048.png` and `119.png` show the expected Original
  counters 029, 028 and 030, with saved durations 8, 11 and 5 frames.
  Eight input frames needed a second layout retry; three runs of consecutive
  retrying frames each had fresh input/cause. Those warnings are retained, as
  is the intermediate-image capacity warning; named checkpoints and semantic
  checks continued. No ignored layout retry or failed finding was reported.

Reproduce the new tests with the project's pinned `DEADPAN_FFMPEG_PREFIX`:

```sh
cargo test --locked -p deadpan-cli --test preview_export vfr_recipe_plans
DEADPAN_PREVIEW_EXPORT_KEEP=1 cargo test --release --locked -p deadpan-cli \
  --test preview_export vfr_recipe_exports -- --nocapture
```

Evidence at `/tmp/deadpan-vfr-final-20261009` retains all eleven complete
projects, published movies, per-fixture verification reports, `results.json`
and a complete artifact hash inventory in `pins.json`. The production source
map pins the five changed runtime files over base `bdc1f94b`; test-only
constructor/refusal updates and documentation are outside that runtime map.
`measured-source` retains those exact runtime bytes. The final source cleanup
only removes an unfulfilled `large_enum_variant` expectation and updates
provider comments; it changes no executable statements. Strict workspace lint
passes after that removal. The debug app linker reported that its `__eh_frame`
exceeded the compact-unwind table's 16 MiB encoding limit; linking completed,
with the warning noting possible exception-handling performance impact.

| Artifact | SHA-256 |
| --- | --- |
| Production source map | `392ce848ec1333401a63a52135f7fda2187fc07bc624b24e02bdcccff9b23f51` |
| Executed release CLI | `51f9894d18889b4c2bd2e471cda47b20dda24665047a268af674c99767041710` |
| Executed export test binary | `2d03b388b80ab0753ed0fd5432f4327454be4b6e58f7d9509255fc7e5216b95f` |
| Final-source replay app | `5e2a1463b24a92a662dca91c63e7c4b6ae54fa847d5fe9edac269c69d23f62fe` |
| Matrix results | `02dd9bf4f09420a97d7c6d278b9cef2bbcf80a95c12b5819c7a338c934d2d7c9` |

Logs: `/tmp/deadpan-vfr-plans-20261009.log` retains the original failure;
`/tmp/deadpan-vfr-plans-fixed-20261009.log`,
`/tmp/deadpan-vfr-picture-plan-20261009.log` and
`/tmp/deadpan-vfr-exports-20261009.log` retain the passing focused runs.
The broad gate is in `/tmp/deadpan-vfr-workspace-20261009.log` and
`/tmp/deadpan-vfr-remaining-20261009.log`; its complete coverage audit and
ignored-test list are `/tmp/deadpan-vfr-gate-audit-20261009.json`.
Lint/doc/replay logs use `/tmp/deadpan-vfr-{clippy,clippy-ui,doc,replay}-20261009.log`.
The replay's full report, hashed executable copies and offscreen pictures are
at `/tmp/deadpan-vfr-hold-replay-20261009`.

The encoder and verifier share the canonical picture/DSP paths, as documented
in [preview/export verification](../PREVIEW_EXPORT_VERIFICATION.md). The
independent oracle checks reference source identities; lossy decoded pixels do
not independently identify nearly identical Original frames. This change does
not establish physical speaker output, native-window presentation scheduling,
fractional clean-aperture sampling, all source policies or all creative recipes.
