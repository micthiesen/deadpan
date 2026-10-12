# Retained viewer pixels, 2026-10-12

The previous sound-treatment qualification's black-viewer observation was an
incorrect visual assessment. The saved PNGs themselves contain the picture.
An original-resolution inspection and direct RGBA measurement resolve the
observation without a production renderer change.

The prior event-return and current event-treatment captures at `Sound removal
is reachable at 960x640` both contain 2,860 red, 2,860 green and 2,912 blue
pixels inside the reported viewer `[156, 93, 756, 252]`. Their caption and
semantic identity both identify sequence frame 3. The original PNGs remain
unchanged; the inspection script records their hashes and the report hashes.

The sound-placement replay now checks the final composed frame at both 960×640
and 1280×820 after scrolling to Remove sound. It verifies fitted picture
geometry, visible navigation, caption paint and opaque red/green/blue fixture
pixels inside the viewer. This catches an empty or fully covered fixture viewer even
when a retained semantic identity still exists. It applies only to this known
color-bar fixture, not arbitrary user pictures. Pixel readback is visual-mode
verification; performance mode retains geometric checks without readback.

The debug and final release replays each pass 273 scenario checks plus the
shortcut audit. The final release capture contains 2,860 red, 2,860 green and
2,912 blue pixels at minimum size, and 7,387 red, 7,475 green and 7,470 blue
pixels at default size. Both final captures were inspected at original resolution.
The release build, strict workspace/all-target Clippy and formatting check pass.
[Retained evidence](../../tools/ui-feedback/evidence/2026-10-12-retained-viewer-pixels/README.md)
includes source/binary identities, reports, command receipts and both captures.

The primary agent worked alone and self-reviewed the change. No live window,
physical display, audio device or listening check is claimed. This closes the
incorrect replay observation, not a product requirement or gate. Partial
attached-sound scopes, broader loop/tail qualification, persistent filtered
caching, mastering and the remaining full-spec work stay open.
