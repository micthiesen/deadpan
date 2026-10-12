# Self-review

Only sound_placement.rs changes executable source. It adds a visual-mode
readback check after the named checkpoint, so Harness::render returns that
same cached frame. Existing picture geometry and caption-paint assertions run
in both modes. Performance mode adds no screenshot readback.

The image iteration uses the viewport's pixels-per-point and the actual Image
widget rectangle. Color counts require opaque pixels and one strong primary
channel with the other two low. The fixed cfr-bframes fixture has all three
bars at the retained sequence frame. Arbitrary black user footage is unaffected.

The earlier black-viewer observation was wrong. Both archived PNGs have
identical primary-color counts inside the viewer; their original bytes are
unchanged. Current tracker/handoff text and the original qualification are
corrected, while historical evidence remains intact. This is verification
strengthening and correction of evidence interpretation, not a runtime fix.

No independent reviewer was used, as requested. No remaining source finding.
