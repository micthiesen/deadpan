# Native identity verification

See the [identity assets and provenance](../../../../docs/design/brand/README.md).

- 2,572 locked workspace tests and 338 optional UI-feature tests passed, with
  zero failures or ignored tests. Strict workspace/UI Clippy and formatting passed.
- Journals retain commands, base revision and source inventories. All 1,250
  entries in the final code inventory still matched when evidence was retained.
  The two earlier inventories capture export-tool changes during preparation;
  the Rust integration and approved embedded PNG are unchanged.
- Bare and bundled native smoke runs initialized Metal and shut down normally.
  `bundle-icon.png` is an AppKit `NSWorkspace` icon readback, inspected visually.
  It is not a screenshot of the live Dock. The readback script is retained.
- The final wrapper built a fresh bundle from the same smoke-tested binary.
  Bundle metadata, actual layered catalog entries, binary hash, and embedded
  PNG binding are retained. The existing-output rejection left every file in
  the earlier bundle unchanged.
- `pdf-review` retains all 15 PDF render checks, visible alpha bounds, vector
  audit and the inspected contact sheet. Every PDF has one page, no fonts and
  no raster images. All 83 asset-manifest entries matched their retained hashes.
- The upstream Inter license is retained byte-for-byte, including its trailing
  space on line 21. The staged whitespace check passes with only that upstream
  file excluded; Rust formatting has no exclusions.

The running editor's desktop, size and state were left intact. Executables and
whole developer bundles are excluded. External FFmpeg libraries, signing,
notarization, clean-machine installation and macOS 15 runtime qualification
remain open. These checks establish the icon integration, not release readiness.
