# Golden renders

`crates/deadpan-cli/tests/golden_renders.rs` renders a small set of
representative structures and compares exact hashes with committed files in
`crates/deadpan-cli/tests/golden_renders/`. It complements the
[preview/export verification](PREVIEW_EXPORT_VERIFICATION.md), which compares
an encoded movie with its committed preview through tolerances. Goldens detect
any change in the exact pixels or samples that the shared plan produces.

## What is rendered

Each fixture is a real one-Original package built only through the public
`deadpan-cli` executable (`project create-original`, then revision-bound
`command --json` requests), reusing the Section 8 recipe builders in
`tests/preview_export/recipes.rs`:

| Fixture | Structure |
|---|---|
| `base` | The shortened 30-frame base edit (Original 12..42). |
| `repeat-with-gap` | Three plays with a silent 6-frame Background gap. |
| `nested-override` | A 2-play Repeat inside a 2-play Repeat with a gap; in outer play 2 only, inner play 1 has a static 1.5x zoom and inner play 2 a -6 dB trim, both through `EditOccurrence` sparse overrides. |
| `retime-half` | A 50% Retime (Preserve when admitted). |
| `freeze-hold` | A 15-frame freeze with captured view. |
| `framing` | Static zoom, smoothstep creep and a target follow. |
| `escalating-repeat` | Three plays escalating +3 dB and +0.08 scale. |

The committed revision is rendered through the same path as export:
`ProjectPictureSession` (immutable revision, `RenderPlan`, verified Original
decode) and `ExportPictureSession` on the qualified Metal device
(composition, captions, working-target readback, limited Rec.709 I420 with
left-sited chroma). Audio uses `OfflineAudioSession`, the limited audition bus
that export and `verify-export` read, over the exact absolute sample interval.

Each golden records the frame count, rate and raster, one SHA-256 of the tight
I420 bytes per output frame, one SHA-256 of interleaved little-endian f32
stereo PCM per 8192-sample block, and the hash of the whole PCM stream. A
mismatch names the differing frames and PCM blocks.

The test also asserts structural consistency independently of committed
values: identical Repeat plays render identical pictures, only the overridden
nested occurrence is zoomed while the trim-only occurrence keeps the shared
picture, and escalated plays differ.

## Explode equivalence

`exploded_repeats_render_exactly_like_their_repeat_goldens` explodes the
Repeat-with-gap and escalation fixtures, and the nested override fixture inner
Repeat first, then outer. Each exploded package must match the frame count,
every picture hash and every PCM block of the original structure's committed
golden. This test never blesses; the goldens belong to the original Repeats.
See [explode and duplicate](EXPLODE_DUPLICATE.md).

## Running and regenerating

```sh
export DEADPAN_FFMPEG_PREFIX=/private/tmp/deadpan-ui-ffmpeg/prefix
cargo test --release --locked -p deadpan-cli --test golden_renders
```

A missing or different golden fails. Regeneration is explicit and refuses to
run when `CI` is set:

```sh
DEADPAN_BLESS_GOLDEN=1 cargo test --release --locked -p deadpan-cli --test golden_renders
```

Bless only after establishing why the output changed (for example an
intentional renderer, DSP or plan change), and review the per-frame and
per-block differences first. Never bless to make an unexplained failure pass.

## Platform assumptions

The hashes are evidence for one qualified platform, not a cross-platform
contract:

- Apple Silicon macOS with the Metal backend (`cfg(target_os = "macos")`;
  the test fails if the adapter is not Metal). Picture composition runs in
  GPU floating point; different GPU families or driver/OS shader compilers
  may legitimately round differently.
- The pinned LGPL FFmpeg developer prefix (`DEADPAN_FFMPEG_PREFIX`) decodes
  the H.264/AAC fixture `native/deadpan-source/tests/fixtures/cfr-bframes.mp4`.
  A different FFmpeg build may change decoded pixels or AAC PCM.
- PCM is produced by the pinned resampler, DSP, edge-fade and limiter code in
  f32/f64 on the CPU. Rust's IEEE arithmetic is deterministic for one target
  and build; vectorization or library changes can still change the last bit.
- Release and debug builds must agree; the goldens were generated with
  `--release`.

If the qualified platform changes, regenerate on the new platform and record
the hardware, OS and FFmpeg prefix in the commit message. Goldens do not
qualify HDR output, encoder behavior or emitted files; those remain covered
by their own verification.
