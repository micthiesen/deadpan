# Extension pixel checks

See the [qualification record](../../../../docs/qualification/extension-pixels-2026-10-08.md)
for scope, measurements and remaining admission work.

- `source.json` records the base commit and exact final changed source bytes.
- `environment.json` records the reference Mac, toolchain and FFmpeg prefix.
- `extension-pixels-check.log.gz` is the initial all-target model crate check.
- `extension-pixels-focused.log.gz` preserves the first 27-test run, including
  its two fixture failures. The qualification record explains both corrections.
- `extension-pixels-corrected.log.gz` records the corrected 33-test pass,
  including six real decoder/converter tests with multiple fixtures each.
- `extension-pixels-gate.log.gz` preserves the first gate, including the real
  HDR media-duration failure. Its source is `source-before-diagnostic.json`.
- `extension-pixels-isolated-diagnostics.log.gz` records the isolated HDR/store
  checks; the HDR rerun passing did not fix the retained failure.
- `extension-pixels-final-*`, `extension-pixels-ui-tests` and
  `extension-pixels-doctests` cover the clock-label fix and remaining initial
  gate stages. The raster review correction came afterward.
- `extension-pixels-raster-check.log.gz` preserves the transitional compilation
  failure while the new encoder report field was being integrated.
- `mux-extension-focused.log.gz` covers the completed raster and mux corrections:
  119 tests passed. `mux-extension-gate.log.gz` records the final combined gate.
- `results.json` extracts test summaries and records review results.
- `manifest.json` hashes every retained evidence file except itself.

The real-media fixtures are synthetic lossless RGB movies with exact retained
black PNGs. They establish decode, conversion, sampling and rejection behavior;
they do not establish model quality or identity. No model inference, packaged
app, live UI or real model extension acceptance is claimed for the pixel checks.
The separate mux correction has retained-file and packaging evidence. Extension
Ready and acceptance remain disabled pending the remaining output checks.
