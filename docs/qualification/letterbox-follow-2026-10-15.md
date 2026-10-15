# Letterbox-corrected follow centers (2026-10-15)

DP-08. A `Follow` now centers its target where the renderer actually draws it
when the canvas aspect differs from the Original's, for example a 16:9
Original on a 1080x1920 Shorts canvas.

## Design as built

- `AttentionTarget.display_aspect: Option<ExactRatio>` records the exact
  upright aspect of the picture the regions are normalized to: visible (clean
  aperture) width times SAR over visible height, inverted for 90/270 rotation
  (`deadpan_core::display_aspect`). It must be positive, at most 64:1 either
  way, with reduced terms of at most 2^48. It is omitted from the wire when
  absent, so existing documents and hashes are unchanged.
- `deadpan_render::FrameMetadata::display_aspect` derives it from decoded
  metadata exactly as `PictureGeometry` lays the picture out;
  `deadpan_cli::tracking::exact_display_aspect` derives it from the
  qualification receipt's stream interpretation.
- `deadpan_core::fit_source_point` places a normalized upright point on a
  canvas for `FitMode::Fit` in exact rational arithmetic. Along the axis the
  picture fills, the point is unchanged. Along the other, it is scaled about
  the middle by the covered fraction.
- `deadpan_plan::follow_pose` takes the canvas. `RenderPlan::resolve_follows`
  passes the presentation basis, and Camera's two callers pass the document's
  basis. Preview, Camera previews and export therefore share one rule. A
  target without an aspect keeps source coordinates, as before.
- Creation sites that record the aspect:
  - Camera `n` + Enter and `:zoom … target=face:N`. These use the exact
    Original-tier picture Camera opened on (`None` for a proxy picture).
  - App `T` and CLI `track --save`. These use `PreparedTrack`'s receipt
    geometry.
  - `face_target`. This uses `PreparedFaces`'s receipt geometry.
- Corrections and re-tracking clone the target, so they keep its aspect.

## Evidence

All results were run on this Mac with
`CARGO_TARGET_DIR=target/agent-follow` and the pinned FFmpeg prefix.

- Core: `display_aspects_are_exact_upright_shapes_within_bounds`,
  `fit_points_letterbox_and_pillarbox_exactly` and
  `recorded_aspects_validate_and_stay_off_the_wire_when_absent` pass. They
  cover 720x480 at 32:27 (16:9, 9:16 when rotated) and a fractional
  1920x1080.5 aperture.
- Render: `display_aspect_is_the_upright_fitted_shape_and_places_fit_points`
  passes, including anamorphic, rotated and fractional-aperture metadata. It
  agrees with `PictureGeometry::source_to_input` within 1e-12 on 1080x1920,
  1920x800 and 1000x1000 canvases. `tests/follow_letterbox.rs` resolves
  `follow_pose` on 1080x1920, 1920x1080 and 1080x1080 canvases for landscape,
  portrait and rotated anamorphic metadata. It matches the renderer's
  projection within 1e-9 (the Q32 grid). A target without an aspect stays at
  source y = 0.25, while the renderer draws that point at 0.5 - 81/1024.
- Plan: `a_follow_centers_the_target_where_the_picture_is_fitted_on_the_canvas`
  checks the committed plan. A 16:9 follow on 1080x1920 centers y = 1/4 at
  431/1024. A 9:16 aspect on 1920x1080 centers x = 1/5 at 1037/2560. With
  equal aspects, or no aspect, nothing changes.
- Creation sites:
  - Analysis `to_target` records the aspect.
  - CLI `preparation_measures_the_exact_upright_aspect_targets_record` measures
    16/9 for the 320x180 tracking fixture.
  - `save_tracked` saves it.
  - The `face_target` unit test checks it.
  - The app scripted-tracking test saves 16/9 for `cfr-bframes.mp4`.
  - The app face-proposal save round-trips it through the store.
- Replays (`cargo xtask replays --scenario targets,camera,faces,zoom`, binary
  sha256 `2e40d0b3…cdf2c`): 4 passed, 134 checks. New `targets` checks:
  - The drawn target records the displayed Original's aspect (16:9).
  - The tracked target keeps that aspect.
  The replay fixtures use canvases of the Original's own aspect, so painted
  Camera output is unchanged.
- Preview/export: the new `letterbox-follow` recipe puts the 320x180 Original
  on a 180x320 canvas and follows a 16:9 target at 1.35x. Its builder checks
  the committed plan: 10 followed frames center y = 0.4 at 1199/2560, and the
  others use the fallback. Release `every_recipe_export_matches_its_committed_preview`
  with `DEADPAN_PREVIEW_EXPORT_ONLY=letterbox-follow,framing` rendered and
  `verify-export`ed both recipes, and both passed:

  | Recipe | Pictures | Min luma PSNR | Min chroma PSNR | Max thumbnail MAD | Audio offset |
  | --- | --- | --- | --- | --- | --- |
  | `letterbox-follow` | 30/30 | 44.35 dB | 55.53 dB | 0.129 | verified zero |
  | `framing` | 30/30 | 50.74 dB | 55.33 dB | 0.083 | verified zero |

  `verify-export` proves that preview and export agree on the shared picture
  path. The plan and render tests above prove that the follow centers the
  right point.

## Remaining

- Targets saved before this change have no aspect and keep source
  coordinates. Re-saving them records it, and nothing infers it silently.
- Follows under a pause's captured geometry still use their fallback, as
  before.
- `FitMode::Fill` is not an authored canvas mode, so it is not mapped.
- Physical-display and listening checks are unaffected and not claimed.
