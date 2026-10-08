# Reordered video media duration, 2026-10-08

The extension pixel milestone's workspace gate exposed a real HDR export
failure. A VideoToolbox PQ Main10 file contains 46 pictures at 30 fps, with
46 ticks in its `stts` table, but declares 47 ticks in video `mdhd`. The strict
emitted-file verifier rejected it. An isolated rerun passed, but its file was
not retained. Variable encoder ordering is consistent with this result; the
retained failure is the deterministic regression fixture.

## Cause and retained file

The first encoded PTS/DTS pairs are `(0, -2)`, `(2, -1)`, `(1, 0)`, `(3, 1)`.
Their composition offsets start `2, 3, 1, 2`. Pinned FFmpeg 8.0.3 `movenc.c`
lowers `start_cts` from 2 to 1 while DTS is nonpositive. `get_pts_range` adds
that minimum offset to the first DTS, producing a nonexistent start at -1.
`mov_write_mdhd_tag` consequently writes `46 - (-1) = 47`. Post-edit picture
timestamps remain exactly 0 through 45. Other predicates in the failed
container check match the contract, including PQ color, Main10, dimensions,
mastering/content-light metadata and pixel aspect.

The original failed bytes and manifest are retained in
[`2026-10-08-mux-duration`](../../tools/media-qualification/evidence/2026-10-08-mux-duration/).
The movie SHA-256 is
`01656517ca86729b2b444ef6acdfbd6002acfff429c816d30b6c206a82cc0c97`.
This is synthetic qualification footage, 25,879 bytes.

## Correction boundary

After native muxing closes, the encoder owns a private regular-file descriptor.
A bounded Rust finalizer locates its video media header. Exact durations return
unchanged. A differing duration can be corrected only after the existing
`deadpan-source` MP4 inspector proves the full video packet timing and table
coverage against the authored contract: codec, dimensions, track identity,
time base, normal edit, every CFR duration and every unique presentation ordinal.
The excess must equal the pinned muxer's measured first-CTS/minimum-CTS error.
Unsupported or inconsistent evidence fails.

Only the original four- or eight-byte video `mdhd` duration field can change.
The file is synchronized and the header read back. Packet data, PTS/DTS,
sample tables, edit lists and AAC bytes remain unchanged. The encode report
records the old and corrected ticks and both composition-offset observations.
Host and saved probe-report admission validate that arithmetic against the
captured clock and B-frame policy. They do not treat a report as file proof.

The finalizer uses the existing shared deadline and cancellation flag, bounded
header work and packet inspection, and checks descriptor identity around the
proof. It performs no decoding or path lookup. The full emitted-file verifier
still runs after encoding and retains all existing strict duration checks.
The pinned FFmpeg dependency remains unmodified.

## Verification

The deterministic native regression uses the retained failed file and checks
that only byte 315 changes, from duration 47 to 46, and that another pass is a
no-op. The CLI regression first requires the original file to fail, then runs
complete container, bitstream, every-picture, fresh-GOP and both AAC-decoder
inspection on the corrected copy. Malformed, duplicate, missing, out-of-range,
wrong-clock and unsupported header/table cases must fail without writes.
Version 0 and 1 exact headers, cancellation, deadlines, bounded work and report
arithmetic are covered separately.

Independent review found no remaining defects. The all-target native check
passed; Cargo.lock changed only to record the existing `deadpan-source`
workspace dependency. All 119 focused native/host/extension tests passed,
including the retained file's complete independent inspection and both media
header widths. A fresh workspace run passed all 5,307 tests, with 10 intentional
skips and no leak reports. All 1,071 UI tests passed (two intentional skips,
one pipe-closure leak diagnostic), both doctests passed, and formatting plus
strict workspace/UI Clippy passed. The warning's unestablished cause is recorded
with the [pixel milestone](extension-pixels-2026-10-08.md#verification).

`cargo xtask bundle` built the ad hoc signed release app and audited 74 Mach-O
files. `bundle-verify --keep` passed all positive and negative checks from a
relocated copy in a scrubbed environment, including project creation, verified
export, the bundled AI runtime and tampered/missing-helper refusals. This is
verification on the reference Mac; no clean or second Mac is claimed. Build
provenance, binary hashes, command results and compressed logs are retained
alongside the failed movie. The bundle is 730.5 MiB on disk. No app window or
long-running service was left running.
