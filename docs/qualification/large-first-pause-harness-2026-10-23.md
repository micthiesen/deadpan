# Large first-pause harness evidence, 2026-10-23

A new diagnostic-only `large-first-pause` UI harness scenario measures the first
12-frame InsertPause on a 10,000-beat fixture. The fixture is 1920×1080, 24 fps,
SDR Rec.709, with 10,000 12-frame Background/Silence Holds, two setup commits,
three validated revisions, and no media. The test selects frame 17,933
(Hold 1,494, offset 5), inserts 12 frames, and verifies the authored result and
same-ticket offscreen composition. It performs one insertion per fresh process
and does not claim a threshold or percentile pass.

The final release run completed all seven diagnostic checks. Its single sample
was 393.681375 ms from input to commit and 396.208834 ms from input to offscreen
Background composition. The first quiet diagnostic sample was 389.723625 ms
and 392.35325 ms. A separate compile-adjacent sample was 398.122959 ms and
400.7225 ms. These are independent n=1 observations: do not pool them or report
them as p95/target qualification. Offscreen composition is not native-window
display or scanout. No source decode or PCM path is measured.

The final run used freshly built `deadpan-app` SHA-256
`3bf2d47c6fbc7eade3a259fc8e9ffc275043af2bd62e698a0b98c0937dcfd9bd`.
The private portable fixture copy preserved the source database and authored
inputs. SQLite opened shared-memory coordination sidecar
`project.sqlite-shm` (32,768 bytes); its WAL was empty (0 bytes). The initial
wrapper's full-path unchanged assertion failed because SQLite created
coordination sidecars absent from the input path map. The complete before-map
was not persisted, which limits the evidence but did not cause the failure. The
main database hash matched. That failed receipt is retained. The final guard
compared complete maps and passed; only the -shm and empty -wal coordination
files differed.

Verification completed with exit 0: 1,295 UI-feature tests across two binaries
(11 skipped), strict UI Clippy and formatting, and the release build. The final
test summary is the exact recorded count; 11 tests were skipped and not
executed. The report also records two frames settling on a second layout retry
and one three-frame retry run, with a distinct cause on each frame.
General review found a missing presentation-basis guard and negative unit
case; both were fixed before the final run. Measurement review reported no
findings. Production Core 58 / SQLite 87 were already present in implementation
commit `8c6e9eb6e3f9da44bb0c767caf8df683713ae811`;
`e9be3a8db6f170b7c4440a8b0a0bbab806862ec3` is a progress commit. This harness
change contains tools and tests only and does not establish a production
latency improvement. In particular, the direct
performance result of 93.633 ms has different measurement boundaries; the
harness does not attribute time to the store.

Evidence, including exact final source overlays (named with the
release-final-source__ prefix), separately labeled historical draft overlays,
the final all-UI archive run receipt/output, manifests, reports, guards, and
compressed command logs, is in
[the evidence directory](../../tools/media-qualification/evidence/2026-10-23-large-first-pause-harness/).
`inventory.json` records each retained file's byte count and SHA-256. Binaries,
generated packages, Cargo targets, and bulky fixture databases are excluded.
The inventory verifies 66 payload files /12,780,151 bytes; its SHA-256 is
`68fa4412154eb2201b07ee4f56d9a16070d1fa34db6d82e73addbddb59e9ee23`.
