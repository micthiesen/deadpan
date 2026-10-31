# Native fixture API compatibility, 2026-10-31

Two existing native test fixtures had fallen behind the current Core API.
The selected extension rectangle now enters TargetGeometry through its existing
lossless TargetRegion conversion. Its center [500000,500000], size
[333333,333333] and serialized region stay unchanged. The saved tracking Follow
explicitly selects Unconstrained centering, preserving the test's direct target
center at scale 2. CanvasClamped would move its first horizontal center from
58/320 to 80/320 and contradict the existing assertion.

The isolated qualification copy also lacked an include_bytes manifest. It is
restored from the existing published encoded-manifest.json, Git blob
3c1d879a9fd8b6269b1e75895a9f55f913cf1fbd, without regeneration. No production
logic, thresholds, geometry, oracle or acceptance status changes.

Whole-workspace all-target compilation now passes in both variants. Main's
required strict check with --no-test and isolated all-target Clippy for the two
native packages pass, as do formatting and hygiene. All 18 native test executables
compile per variant; focused actual worker runs pass six tracking tests and
four extension-geometry tests per variant, with no ignored tests. Thirteen
unrelated extension-quality cases are filtered; other worker executables receive
compile credit only.

The runs exercise saved-target reload/Follow reconstruction, actual tracking,
selected-region measurement, genuine drift rejection, explicit unavailable
regions, cancellation, deadlines and mismatched native input. The two embedded
tracking movie fixtures agree across roots and are verified byte-for-byte
inside the actual test executable. Both real worker executables, the test
executable and the fixture encoder are pinned before and after each run.
Runtime workers use the qualified FFmpeg 8.0.3 prefix. The separate synthetic-RGB
test-fixture encoder is the recorded Homebrew FFmpeg 9.0.2 CLI; it is not a
runtime dependency qualification.

Only the two test constructor files change. They agree across roots; Main's
2,903 and the isolated copy's 2,575 other initial named inputs remain exact.
All 30 immutable before payloads stay fixed. This repairs test coverage and
the compilation gate; the full product goal remains active.

Evidence: [compressed inventory](../../tools/media-qualification/evidence/2026-10-31-native-fixture-api/inventory.json).
Inventory SHA-256: 3aab95da62a1b6e1be77fb154b5ea53a3fc7d4f81b28eb24c3ef446c49f7992d.
Its 72 records retain 18514474 expanded bytes in 4448351 stored bytes.
