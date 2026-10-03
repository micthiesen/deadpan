# Saved beat sounds, 2026-10-03

This increment connects saved owner-local sound recipes to the existing bounded
occurrence preparation and canonical authored bus. It advances DP-04 and DP-09;
it does not complete a product requirement or gate.

## Behavior and limits

Core schema 44/database 56 retains `BeatSound` recipes under `(owner, SoundId)`.
Set/Delete use revision checks, granular inverse patches and durable history.
Changed attachment addresses require admitted source records and receipt/original
bindings. Historical slice reuse requires recapture from the stored revision.
Root sounds and attachments share the 64-event limit. Empty owner maps and
duplicate owner/local IDs reject.

Whole-owner capture carries recipes and media; imports assign new owner IDs and
retain local SoundIds. This creates recipes in the new current owner clocks.
It does not retain historical independent sound phase across arbitrary edits.
Timing-changing commands reject while beat attachments are present. Partial-copy
timing qualification, independent retained clocks, native placement, scoped
allowances and `ib`/`ab` remain required.

Each current occurrence prepares a separate complete Preserve history. Current
Hold policy and complete-island edges apply after processing. Event gain and
ancestor treatments use their declared current clocks. Exactly coincident Hard
choices include current parent Repeat-gap choices on explicit gap branches.
Original audio, root sounds and independent occurrences sum in f64 before one
checked f32 conversion and the common limiter. Silent or absent occurrences
still admit their source, including warm limiter reads.

Earlier unused packages refuse before writes. Current history/recovery checks
and embedded audio-context codecs remain; the obsolete schema-52 upgrade is
removed under the user's development-format authorization.

## Review and corrections

Independent core review found that an empty local sound map could block all
timing edits without offering a sound to delete. Both parsing and validation
now reject it. Strict duplicate-map regression coverage accompanies the fix.
The recipe validator also now takes its typed recipe directly.

Independent audio review found that explicit gap attachments omitted current
parent Repeat-gap Hard choices. The occurrence now retains the active gap's
exact parent-clock extent and policies. Mirrored start/end PCM tests verify the
fix and retain the same output through query cuts.

Integration fixed missing new-map fields in frozen readers and old test patch
literals, ambiguous test imports and an owned/borrowed test vector conflict.
An initial narrow test build used the wrong PlayOverride field name. The first
executed narrow attempts passed four cases but rejected three malformed
picture-only fixtures before rendering; those fixtures now retain a real video
stream and clear the absent primary audio's explicit duration mapping.
Strict lint also found a complex patch-map return type; a private type alias
names that existing type without changing behavior.

## Verification

The focused `cargo test --locked -p deadpan-audio --test audio_definition
saved_beat_sound` run passes seven tests, with zero failures and zero ignored.
It compares real decoded WAV PCM against independently assembled placement,
edge, gain and mixing references. Preserve references reuse the canonical
stretch engine; this verifies the adapter, not the DSP algorithm independently.

The full `cargo test --workspace --locked --no-fail-fast` run recorded 3,546
passed, four failed and zero ignored. All four failures were existing format
assertions: three expected old schema numbers, and the literal slice fixture
did not expect the new empty `beat_sounds` map. Only those four test files
changed afterward. The historical fixture itself remains intact.

All four affected targets then passed in full:

- `cargo test --locked -p deadpan-core --test node_gain`: nine passed.
- `cargo test --locked -p deadpan-store --test delete_range --test delete_ripple
  --test edited_slice`: 20 passed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.

The reruns had zero failures and zero ignored tests. Production source is
unchanged from the full workspace run; the retained source inventories verify
that only the four corrected test files differ. Final source inventory SHA-256:
`905c25ab186109a3fa9cbc726b05f740ed6bd056bb526b098ed73e5756400cdc`.

[Evidence metadata](../../tools/media-qualification/evidence/2026-10-03-saved-beat-sounds/metadata.json)
records every command, source inventory, exit status, environment and review
result. Compressed complete logs include the failed attempts.
[SHA256SUMS](../../tools/media-qualification/evidence/2026-10-03-saved-beat-sounds/SHA256SUMS)
covers all retained evidence files. Independent reviewers found no remaining
defects in the supported scope after the corrections above.

## Environment and untested scope

Base: `5d2f9087505d750384b15d2bac731bb7abe13dc3`.
Apple M5 Max, 128 GiB, macOS 26.5.2, Rust/Cargo 1.97.1, locked dependencies,
FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

No visible UI changed. Optional app-feature tests, native GUI replay, acoustic
delivery, response-time measurements and emitted-movie equivalence are outside
this increment. The app was not launched for these backend checks. Full
mastering, temporal attachment editing and release qualification remain open.
