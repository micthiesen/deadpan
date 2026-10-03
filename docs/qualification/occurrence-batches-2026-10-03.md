# Bounded sound occurrence batches, 2026-10-03

This increment adds current-owner occurrence lookup and additive PCM preparation
for a bounded root window. It advances DP-04 and DP-09 groundwork. Persisted
beat-owned sounds, their independent retained clocks and edit/copy lifecycle,
authored treatments/allowances, final bus integration and native placement remain
open. It does not enable `ib`/`ab`.

## Behavior

The lookup uses current Sequence, Repeat and Retime structure, independently of
Original sampling bindings. Stable play and active explicit gap identities remain
intact. Target ancestry and compact-run boundaries skip unrelated branches;
relevant plays within a Preserve input remain subject to aggregate limits.

Exact picture geometry is separate from audio processing influence. An outer
crop can retain a voice's processed Preserve output after hiding its geometric
owner. Final occurrence admission uses absolute RoundEven sample intervals.
Positive frame duration alone does not guarantee an allocated output sample.

Batch construction validates the independent recipe even for empty windows and
bounds all retained runs and construction work. PCM subreads share one deadline,
work/dependency budget and residency cap. Static processing histories are checked
before media access; runtime reservations remain enforced throughout preparation.
Current Hold gates apply after each independent processing chain. Contributions
sum in f64 and undergo checked f32 conversion, without normalization. A successful
read with no contributing occurrences still revalidates the source.

The batch retains processing graph identities. Projected PCM caching remains
request-local; no persistent cache or performance qualification is claimed.

## Review and corrections

Integration corrected the earlier single-occurrence visibility check: it could
reject processed output retained by a later crop. Other corrections removed
duplicate child/gap traversal and its quadratic deduplication, skipped unrelated
compact Repeat runs, and excluded zero-length rounded output allocations.
An independent review identified source resolution before an initial processing
reservation; unconditional revalidation now occurs after preparation, preserving
successful silent-read admission without opening a source before that failure.

The reviewer also found that rounded-empty Preserve output could still expand
its complete Repeat input before being discarded. Traversal now checks output
sample overlap before descent, using an already active outer Preserve influence
when one exists. A million-play regression exercises the empty-output path;
the hidden-owner crop test protects retained processed output. The final
independent review found no remaining actionable defects.

The first three focused attempts stopped during compilation: ambiguous sample
grid typing and unsupported ordering operations on `ExactRatio`, a moved range,
then a missing `NodeId` import. No tests ran in those attempts.
The first executed focused run passed 93 tests and failed two planner fixtures
that used `play_overrides` instead of the actual `overrides` wire field. The
fixtures were corrected. All five new PCM cases passed in that run.
Strict lint then found one redundant `Ok(...?)` wrapper. Returning the helper's
result directly removes the warning without changing its behavior.

## Verification

The corrected focused command
`cargo test --workspace --locked occurrence --no-fail-fast` passes 96 tests,
with zero failures and zero ignored. It includes 11 new owner-lookup cases,
one recipe/budget case and five PCM cases. Counts overlap with the full suite.

The final source passes `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets --locked -- -D warnings`.
Its source inventory SHA-256 is
`a1bb11067b7195d4546ecea9c4c567e53f681f34862141e66e62a65cb40935d7`.
The focused run predates only the behavior-preserving lint cleanup; its inventory
is `38cc200ca9cfab222c69487228c58b4f9bc8aed90c1600123267dea0ffa8fe24`.

The full `cargo test --workspace --locked --no-fail-fast` run passes 3,540 tests,
with zero failures and zero ignored. It uses the same final source as formatting
and strict lint. Source inventories stayed unchanged during these checks.

## Limits

These APIs are borrowed current-plan preparation state. They neither persist
attachments nor preserve a sound's authored clocks across edits. No UI behavior
changed; optional app-feature tests and native GUI replay are outside this
backend increment. Acoustic delivery, response time, full mastering and product
release remain unqualified. No full-product requirement or gate is complete.
PCM references reuse the existing canonical stretch implementation. They verify
the adapter's placement, independent histories and summation, not the stretch
algorithm independently.

## Environment and evidence

Base: `f9682f73cadfbd78dd349692292393431ae7b9df`.
Apple M5 Max, 128 GiB, macOS 26.5.2, Rust/Cargo 1.97.1, locked dependencies and
FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`.

[Evidence metadata](../../tools/media-qualification/evidence/2026-10-03-occurrence-batches/metadata.json)
retains exact commands, exits, counts, source identities, environment and review
limits. The same directory contains compressed logs for successful and failed
attempts and their source inventories.
[SHA256SUMS](../../tools/media-qualification/evidence/2026-10-03-occurrence-batches/SHA256SUMS)
covers every retained evidence file.
