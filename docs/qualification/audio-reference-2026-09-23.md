# Frozen audio reference qualification, 2026-09-23

This increment builds on `06ea9a54aad57efd06a0099d788fc9bd8c8eaaff`. It adds
checked old timing snapshots, explicit reference-policy clocks and a bounded
root-resume PCM suppression consumer. It does not add an authored Hold binding,
change core 14/database 20, or connect the consumer to application transport.
[The contract](../AUDIO_REFERENCE.md) describes the implemented boundary.

## Verification

The exact repository gate passed with **1,011 tests**, zero failed and zero
ignored. All 333 tracked and untracked source/fixture hashes remained unchanged
through the gate. [Raw evidence](../../tools/audio-qualification/evidence/2026-09-23-reference-clocks/summary.json)
retains commands, logs and source hashes.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo test --workspace --locked` | 1,011 passed |
| `cargo build --workspace --locked` | Pass |
| `cargo run -p deadpan-cli -- doctor` | Pass; development foundation, core 14/database 20 |
| `cargo run -p deadpan-app --locked -- --smoke-test` | Pass; Metal window and shutdown |

The environment is macOS 26.5.2 (25F84), Apple Silicon, Rust 1.97.1 and the
pinned compatible FFmpeg prefix. The PCM regressions use actual canonical
Preserve DSP over generated stereo signal. They are not recorded-speech or
listening qualification.

Coverage includes frozen JSON round trips, closed vocabulary and duplicate
rejection; exact signed placements; fractional Retime projection; complete
nested paths; billion-play lookup and sparse overrides; later Split, move,
reorder, shrink and deletion independent of old aliases; Tail maximum in local
Hold/gap units; and root round-even versus selected-input/intrinsic-output
point-ceil policy grids. Root and preparation queries are compared with current
render-plan classifications and full allocated extents.

Canonical PCM tests reproduce both NTSC rounding directions: retained root
silence must cover a sample before the new structural Hold begins, and current
silence must cover a sample before the shifted old mask begins. Suffix-first and
irregular reads agree. Tests also cover exhausted reference domains, wide signed
anchors, cancellation, span/work exhaustion, invalid current ranges and unchanged
PCM on failure.

## Review and corrections

General, exact-timing and hostile-ingress reviews examined the new APIs. The
initial policy consumer treated every `Silence` classification as forced output
silence. Existing Preserve semantics force silence only for `SilentHold`;
missing source audio and outside-placement gaps may contain processed decay.
The consumer now retains that distinction. A regression with a valid no-audio
still Source failed against the broad mask; after the fix both that case and an
outside-placement Source preserve their actual prepared decay. The before/after
logs are retained with the final gate evidence. The first attempted fixture
used a Source with no media and was correctly rejected before reaching the mask;
it was corrected before establishing the regression.

The ingress review identified oversized vectors being allocated before the
100,000-entry checks. A streaming frozen-layout preflight now runs before typed
materialization. It counts aggregate edges and runs, enforces collection and
depth limits, and retains no document-wide JSON value. Capture precharges those
counts before cloning. A frozen-only 100,000 total compact-run cap leaves play
counts compact and does not change the existing authored-document grammar.

Follow-up inspection also caught malformed shapes and duplicate fields that
could otherwise reach serde's temporary enum buffer. Role-sensitive shape checks
and fixed per-record bitsets now reject those inputs before typed parsing.
Tests cover oversized lists and maps with unparsed malformed tails, totals split
across multiple legal-size lists, legal/escaped field-named aliases, nested bad
values throughout scalar/range/policy fields, repeated audibility bodies, and
exact-cap capture/roundtrip versus one-over-cap rejection. The final ingress
re-review found no remaining issue; general and timing reviews also finished
without an outstanding finding.

The first full-gate attempt found module ordering from rustfmt and was corrected.
Later gate attempts were explicitly interrupted during Clippy and test compilation
when the two substantive review fixes became necessary; neither interruption
was an assertion failure or a passing gate.

Document checks verified all eight saved ImageGen boards/prompts, five unchanged
archived spec files and changed Markdown links. No GUI behavior changes in this
increment. Native smoke initialized Apple M5 Max Metal and completed shutdown;
it does not establish aesthetics, focus, IME or keyboard ergonomics.
The [single-original design targets](../design/README.md) and their previous
native review remain current. Authored reference lifecycle, arbitrary-boundary
Hold insertion, cross-grid composition, new seam fades, full playback/export
and release qualification remain open. No requirement or gate is complete.
