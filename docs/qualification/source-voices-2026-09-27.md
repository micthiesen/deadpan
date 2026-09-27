# Independent catalog source voices, 2026-09-27

This increment adds a checked independent source operand to the existing audio
preparation engine. A catalog sound can supply PCM without creating a Source
node or changing project history. Core 28 and database 34 are unchanged.
Persisted sound events, sampled-route PCM integration, scoped Hold allowances,
voice effects and the final voice bus remain required. No DP requirement or
delivery gate is promoted.

## Source and policy contract

The immutable plan retains catalog assets, while frozen-context admission remains
explicitly separate from normal revision admission. A recipe requires a contained
audio span, qualification identity and explicit natural-rate mapping. Signed
48 kHz placement offsets remain exact. Separate input/output views share one
opaque identity and complete source recipe. Their scope stays attached to the
checked owner; a catalog asset does not bypass descendant or occurrence checks.

The input leaves current silent Holds to output policy so Preserve can prepare
complete input history. The output applies only current silent-Hold issuer rules
from the structural owner on the consuming grid. It does not inherit Original
absence, endpoint masks, RoomTone, tails, timing bindings or fades. The regular
Original policy and PCM path are unchanged. Own selected source endpoints still
constrain raw sampling; after Preserve they do not erase processed decay.

Pure plan cases exercise fractional source phase, catalog bounds and clocks,
missing qualification, explicit natural-rate mapping, stable voice identity,
foreign-plan rejection, shared limits, current Hold identity and fractional
policy remapping. A Hold ending at 3/2 destination frames owns two samples;
scaling its previously rounded mask would incorrectly suppress three.

## Real media path

Playback tests retain, decode, qualify and register the 44.1 kHz mono fixture
without insertion. The existing plain WAV has no declared speaker layout, so
production playback correctly rejects that interpretation. A temporary
WAVE_FORMAT_EXTENSIBLE container declares mono center while retaining all 44,117
original PCM samples byte for byte. It follows the same production receipt,
snapshot, private decode cache and layout admission path as ordinary media.

The opening guard now admits exactly the signed16 extensible header described in
[source admission](../SOURCE_ADMISSION.md). Fixed extension size, valid-bit width,
speaker mask and the exact PCM GUID are checked before demuxing. Existing file,
sample, packet, header, deadline and channel/rate limits remain in force.
Malformed header fields, every GUID byte, zero/inconsistent/reserved masks,
truncation, selected-stream mismatch and reduced resource limits are exercised.

PCM references use independently specified 147/160 source-sample phase, a 137
sample onset and measured source endpoints. Preserve compares the engine with
canonical preparation of a complete 4,800-sample input to 3,200 output samples.
Tests also cover silent-Hold input/output separation, cold and shuffled reads,
unchanged Original PCM/history, wrong plan/scope, missing receipts, unspecified
layout rejection and revoked original access.

The Preserve reference includes nonzero processed decay after the scaled source
endpoint. A separate cold tape selects `[1/4, 7/4)` output frames and compares
against the complete prepared reference at sample offset 400. This checks that
cropping the output does not restart processing or discard earlier input.

General, timing and native-admission reviewers returned no findings. The initial
plan test fixture incorrectly paired absent audio with an explicit duration and
was corrected to FitBeat. The initial real-media failure exposed the fmt40
admission gap described above; its full failed and successful runs are retained.

## Verification and acceptance limits

Verification used Rust/Cargo 1.97.1 on macOS 26.5.2 (25F84), arm64, with the
qualified FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`. All 581 source/config
hashes stayed unchanged through the complete workspace gate.

| Check | Result |
| --- | --- |
| Source-voice, source-policy and existing Hold-policy plan suites | 20 passed |
| Native input-admission filter, including extensible WAV | 27 passed |
| Real catalog PCM tests, including final cold crop/decay assertions | 4 passed |
| Workspace formatting and strict Clippy | Passed |
| Locked workspace tests | 1,791 passed, 4 failed, 0 ignored |
| Locked workspace build and CLI doctor | Passed |
| App plus `ui-harness` strict Clippy | Passed |
| App plus `ui-harness` all-target tests | 236 passed, 0 failed |

The four workspace failures match the preceding increment. The jobs test
`directories_fifos_and_sockets_are_rejected_without_blocking` fails at socket
creation with `PermissionDenied` / `Operation not permitted` in
`crates/deadpan-jobs/tests/artifact.rs:200`. Three playback tests hit the existing
wait helper at `crates/deadpan-playback/src/tests.rs:258`:

- `tests::canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock`
- `tests::original::original_preserves_leading_audio_trailing_audio_and_exact_rounded_picture_clock`
- `tests::original::original_pcm_ignores_edits_and_target_cache_never_reuses_sequence_pcm`

All three pass individually with the unchanged workspace test binary. Whole-test
times were 11.970 s, 20.845 s and 12.486 s in the order listed above. Its SHA-256
was checked before and after the retries and is retained with their reports.
The cause of the parallel failures remains unresolved. These isolated results
do not erase those failures or establish their cause. The workspace gate remains
failed; test deadlines and thresholds were not changed.

The [verification record](../../tools/media-qualification/evidence/2026-09-27-source-voices/verification.json)
retains commands, compressed logs, initial failures, source hashes, review scope,
host observations and checkpoint tooling. The private scratch checkpoint also
retains the complete tracked patch and every untracked file, including the
contributed harness and all existing design boards/prompts.

No new GUI or device behavior is introduced. The contributed harness remains
preserved and its feature checks are part of verification. GUI aesthetics,
physical keyboard/IME/accessibility behavior, listening, export equivalence and
performance are not established by these backend tests. Prior Metal `NoAdapter`
results remain unchanged. Source pushing remains blocked by this session's
read-only `.git` access; no commit or push is claimed.
