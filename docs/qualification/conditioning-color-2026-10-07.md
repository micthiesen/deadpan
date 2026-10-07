# AI conditioning colour conversion, 2026-10-07

New AI pause inputs convert BT.709 transfer codes to the model's declared
sRGB space before fitting and PNG encoding. Advisory join measurements use
the same conversion, so equivalent BT.709 Original pixels and sRGB generated
pixels compare in one space. This addresses the colour-preparation gap in
DP-12 and specification §12.4.

## Implementation

`generation::color::srgb_codes` derives a 256-entry channel lookup from the
renderer’s existing inverse BT.709 OETF, working-space transform and sRGB
display encoding. The BT.709 primaries are unchanged; the matrices cancel.
The fixed lookup avoids per-pixel powers and adds no full-frame allocation.
The result is rounded once to RGB8 before the existing Lanczos fit. sRGB
input bytes and alpha remain unchanged, and row padding is excluded.

New context manifests record `rec709_to_srgb` and a new interpretation string,
which changes the bound context identity. Older retained
`rec709_codes_as_srgb` contexts remain readable and are explicitly described
as approximate. Their accepted masters remain immutable and do not require
reconditioning. HDR, deep samples, rotated pictures, linear transfer and
wide-gamut inputs retain their existing explicit refusals.

## Verification

Apple M5 Max, macOS 26.5.2. Evidence is under
`/tmp/deadpan-resume-20261006`.

- `conditioning-color-2.log`: 17 focused tests passed in 0.988 seconds,
  including real-media Original conditioning and HDR refusal. New regressions
  check every byte value against the analytic curve, fixed points around the
  BT.709 threshold, unchanged sRGB, prepared PNG pixels, padding/alpha and
  unsupported interpretations.
- The join regression compares BT.709 `[20, 64, 128]` with sRGB
  `[36, 79, 140]`: both joins have zero measured difference. Reusing the old
  codes instead reports a Noticeable join, with a maximum error of 16 codes.
- `gate-11.log`: strict workspace and UI-harness lint passed, all 4,744
  workspace tests passed in 417.335 seconds, all 1,027 UI-harness tests passed
  in 231.210 seconds, and both doc tests passed. Ten workspace and two
  UI-harness qualification tests remain explicitly skipped. No failure or
  pipe-close warning was reported. The generated-neighbour conditioning,
  speech-preservation and offline portable-copy integration tests also ran.
- Final wording review covered a context containing both an older
  approximation and a converted side; the description now names each side
  without claiming that neither was converted. Its focused regression passed
  in 0.013 seconds (`conditioning-color-label.log`) after the full gate.
- `bundle-color.log` and `bundle-color-verify.log`: a fresh 722.5 MiB ad hoc
  signed bundle passed all relocated, scrubbed positive and negative checks,
  including native smoke shutdown, Metal, the bundled runtime, verified
  Render and refusal of tampered/missing components.

## Real generation and older accepted media

`color-generation/summary.json` records the packaged app with SHA-256
`378b78006550f125ef829bafa6bf4ece44cee058bf982ba4e0c7197e9f04c416`.
A 30-frame bridge over `cfr-bframes.mp4` reached Ready in 78.284 seconds.
The host report and the worker's retained context both named
`rec709_to_srgb` for the two boundaries, with `approximate: false`.
The fallback document was unchanged until explicit acceptance. The accepted
candidate rendered a verified, published movie in 2.555 seconds.

The same bundle reopened the earlier schema-3 update project's accepted
artifact, whose inputs used the older approximation, and rendered a verified
movie in 2.207 seconds. Its authored document was exactly unchanged.
All processes exited and no test window was left running.

The first focused compile caught a test referring to `DisplayP3` instead of
the renderer's `DisplayP3D65`; it was corrected before any test ran. Its log
is retained as `conditioning-color-1.log`.

These checks establish the host's input conversion and reported comparison.
They do not establish the model's own colour handling, perceptual seam quality
or calibrated join thresholds. Human quality review remains scoped by §29.1.
