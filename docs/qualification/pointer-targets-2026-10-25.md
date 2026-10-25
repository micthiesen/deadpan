# Camera pointer targets, 2026-10-25

Specification §7.6. Camera's existing target editor accepts rectangle drawing,
body movement, corner resizing and point placement/correction. All changes
remain temporary until Enter uses the existing captured, revision-bound target
command. Release never saves; Escape returns to framing. Help abandons Camera
under its existing contract. Keyboard input revokes held pointer capture.

## Coordinate and input contract

The pointer surface uses the actual retained texture rectangle and raster,
not the desired size of a pending replacement. It requires the exact stopped
Original picture for Camera's captured session, revision and Edit position.
Accepted generated pictures cannot supply an Original target. Frozen Original
pictures retain their indexed source identity.

Canonical `PictureGeometry` maps through clean aperture, pixel aspect, upright
rotation, captured Fit/Fill, provider clips and live framing. The inverse is
accepted only where the forward visibility query also succeeds. Bars, hidden
descendant clips, nonfinite coordinates and positions outside the source are
refused rather than clamped to a target edge.

A gesture retains its starting geometry, source-coordinate grab offset and
projection. Every update derives from that start, with one conversion to target
millionths. Centers and sizes keep the keyboard bounds: centers inside the
source, rectangle sides between 1% and 100%. Rectangle corners may extend beyond
the source, matching existing authored geometry. Crossing the opposite corner
is allowed. The central half of even a minimum rectangle remains a body move;
its perimeter retains corner resizing.

egui admits the primary-button press. Input is queued during viewer layout and
applied only after the final undiscarded pass. Capture is revoked on keyboard,
focus loss, pointer disappearance, unavailable exact picture, target save/rebase
or a changed ticket, viewport, raster or framing basis. Returning a held pointer
cannot start another capture. Moving outside visible source support retains the
last valid temporary geometry; release ends capture.

Points never acquire serialized width/height. Pointer editing preserves geometry
kind, the keyboard field and correction-kind lock. The inspector and generated
command reference teach the pointer controls beside the existing keyboard path.
Corner cursors match their diagonals.

## Verification

Base `7ef56b822d6decfe705e3501b22b3657a3359d74`; scratch
`/tmp/deadpan-pointer-targets-2026-10-25`. Mac17,7, Apple M5 Max, 128 GiB,
18 logical CPUs, macOS 26.5.2 (25F84), Rust 1.97.1. Locked dependencies and the
qualified FFmpeg 8.0.3 prefix are retained in invocation receipts. The optimized
development profile uses `CARGO_PROFILE_DEV_OPT_LEVEL=2`.

The source-fourth full App lane passes 1,339 tests, with eleven configured skips.
Two passing tests report lingering-process warnings; both pass a clean serial
recheck. Strict App/all-target/ui-harness Clippy passes. The fifth-source focused
lane passes 60 Camera, reference and routing tests. Source phases retain the
exact changed Rust files plus Cargo.toml, Cargo.lock and rust-toolchain.toml.

The source-sixth minimum-body regression fails against the old rule with
`Resize != Move`. The source-seventh fix passes all 61 focused Camera/reference/
routing tests, strict Clippy and scoped formatting. The final release build
passes in 157.615 seconds. Its app is 78,804,992 bytes, SHA-256
`daec54b73d39a3f86990e1834a1293adb835afa3b1002ad6de993f1b34efa8c3`.
The copied CLI and three helpers are separately pinned and unchanged during
each actual replay. The earlier fourth/fifth release builds remain distinct;
neither substitutes for this final build.

Final release `targets` passes 72 checks plus Kestrel at 60 Hz in visual mode
and again at 120 Hz in performance mode. The separate `camera` regression
passes seventeen checks plus Kestrel. These are separate invocations, not
independent counts to add together. Root inspects the retained release rectangle
and saved-point captures. Both target reports retain six frames needing a
second layout retry and six sequences of differently caused retrying inputs;
the visual report also records its intermediate screenshot allowance. The
assertions reject ignored retries and repeated no-new-input retry loops; these
warnings are not hidden or recast as failures.

The native path exercises draw, body move, reverse corner resize, small-body
movement, outside release, keyboard interruption, raster resize cancellation,
Help ownership, Escape, point creation, manual span-start correction, Camera
rebase and two Undos. The existing target checks retain tracked rectangle saves
through their declared scripted Vision seam. New pointer tracked-rectangle
correction quality is not independently qualified.

Independent read-only audits pass on both final target projects. Each reconstructs
eleven immutable revisions, seven history rows and the two pointer operations
(history 6/7), with the two durable Undos. All 46 database/close-backup table
contents and schemas agree. SQLite file hashes are recorded separately.
The five pointer states differ only in targets and revision identity; the
original target, picture structure, full-source workflow baseline, 16:9 aspect
and indexed target PTS remain intact. The retained Original qualification has
120 pictures and 192,192 audio sample frames; this is metadata/history evidence,
not a new PCM or emitted-file comparison. Each nine-file, 1,202,503-byte package
and its report remain unchanged by the audit.

## Failures and limits retained

- The first compile catches the integration's wrong app type and a Camera test
  fixture missing the pointer state; both are corrected before the passing lane.
- The initial native setup uses the wrong copied CLI basename and places runner
  options after its command delimiter. It fails before app launch. Its partial
  receipt is retained, without inventing a product exit result or missing logs.
- The first actual native run passes all 43 preceding checks, then finds that
  the N-key frame paints Region mode before registering its pointer surface.
  Registration now occurs while the final mutation guard still rejects the
  keyboard frame's pointer update.
- The next run reaches 51 checks before a Help fixture wrongly expects Camera
  to remain open. The replay now checks abandonment, then separately reopens
  Camera for Escape. It also uses the existing span-start correction contract:
  replace the seed geometry, with zero later correction entries.
- A later review identifies overlapping corner hit areas on tiny rectangles.
  The new minimum-size regression is run against that earlier implementation
  before the body-priority fix. Final evidence retains the negative and positive
  results separately.
- The command-reference writer intentionally exits nonzero after writing its
  output. The ordinary current-reference and routing checks pass afterward.
- The independent auditor initially assumes no native warnings and identical
  SQLite file bytes. Its corrected audit retains warnings/scope skips and
  compares database rows/schema, while recording different file hashes. Those
  preliminary assumptions are author-reported; no raw preliminary invocation
  receipt is claimed.
- The first collection passes its byte checks, but its static exclusions text
  incorrectly says replay project trees are excluded even though explicit
  extras retain both final target projects. Before committing, Root preserves
  that untracked tree unchanged in scratch and republishes corrected metadata.
  The original plan, inventory, collection summary, builder and actual receipts
  remain in the final collection under `superseded-first` and metadata aliases.

Vision tracking uses the target scenario's declared scripted seam. Real-person
tracking quality, physical pointer/display delivery, listening and VoiceOver
speech are not established here. This change introduces no new project grammar,
picture rendering, audio or export semantics; no new emitted-file or packaging
qualification is claimed. DP-08 remains Partial, coverage 89%, eight requirements
Complete/sixteen Partial and effects five of thirteen.

The immutable [evidence inventory](../../tools/media-qualification/evidence/2026-10-25-pointer-targets/files.json)
retains 249 logical inputs in 136 deduplicated payloads plus three metadata
files. All 139 files total 5,115,865 stored bytes and 127,505,055 raw/decoded
bytes, including metadata, below each 256 MiB cap. Raw/decoded counts mean
payload decompression, not expanded image/video rasters. There are no omissions.
All seven source phases, 28 complete invocation receipts, the incomplete
prelaunch record, original failed native reports, two selected release images,
both full target projects and independent durable audits remain retained.

Inventory SHA-256 is
`e1dbbc0cd8ba1f7b8bf1062c388faf5d4d8cff7af4e151ca0e0eed9c00197827`;
plan `42be73feb7ea49cf4e716abdafab2618b98e1e9ba74f62c34b5d71cadff8d24f`;
unchanged collector
`a435a0ee061a871ce9ae107dfc29572bfd7e0c2450955a0ed01b121f31b1f118`.
The actual dry run and collection pass; collection verifies all 292 input and
runtime guards before/after and checks the published tree. The collector's
unchanged guard tests are retained in the earlier ranged-Follow qualification.

The independent [post-collection byte audit](../../tools/media-qualification/evidence/2026-10-25-pointer-targets-byte-audit.json)
passes in 0.821 seconds, result SHA-256
`11a704d016b5be754188972aa69a80cd809436aa86e34b7c67b3869e70494c8c`.
It verifies every published byte/decoded alias, all seven source phase pins,
the thirteen current live source/configuration files, all 43 live runtime
guards, actual native outcomes and truthful failed/partial receipts. Its actual
verifier, runner receipt and streams, plus the corrected dry-run/collection
proofs, are [retained separately](../../tools/media-qualification/evidence/2026-10-25-pointer-targets-proof-retention.json)
in fourteen files to avoid a circular immutable inventory. The verifier's
preliminary schema assumptions and unsupported check-name expectations were
corrected before Root's actual invocation; no raw preliminary receipts are
claimed.
