# Authored node gain, retained clocks and durable history

This checkpoint connects the [gain contract](../AUDIO_GAIN.md) to authored
documents, actual decoded PCM and the shared limiter. It does not complete
DP-09, native gain editing, the voice graph or export.

## Implemented boundary

- Core 33 stores optional node treatments with direct and occurrence setters,
  reversible patches, configured unity intent and independent mute policy.
  Splits, treated Partition refinement, repeated pause insertion, duration
  changes and copies preserve complete owner recipes and exact clocks.
- Public documents enforce 100000 gain records and 16 active treatment layers.
  Incoming recipes and patch sides are bounded during ingress; a private
  occurrence copy may temporarily retain 200000 records before the pending
  operation retires context. Final validation retains the ordinary limits.
- Database 39 replays database 38 through frozen core 32. Every earlier adapter
  rejects modern gain fields and commands. The genuine migration fixture retains
  direct Hold setters, isolated Repeat edits, original receipts, historical
  sound routes/allowances, abandoned branches and pending redo.
- Frozen context schema 4 authenticates a sparse map of every nonempty owner
  recipe, including groups and picture-only nodes. Versions 1 through 3 reject
  its new field even when null or empty. Timing layouts and raw lineage remain
  independent of treatment evidence.
- The authored bus sums exact Q32 dB after complete time/pitch and existing
  edges, then converts once to amplitude before mixing and limiting. A Source
  affects its own Original contribution. Independent sound events combine their
  own gain with root treatments once. Default Repeat gaps do not duplicate the
  Repeat's gain. Unity and opposing gains retain bit identity in the fixtures.
- Checked gain-owner queries explicitly represent exhausted retained support;
  the renderer requires zero Original PCM there while independent sounds retain
  their current root clock. Missing provenance and invalid current coordinates
  still reject. Mute never grants silence policy or bypasses source admission.
- CLI `command` persists the same typed setter. `inspect-audio --authored-bus`
  returns up to 256 canonical pre-limiter samples. Existing inspection stages
  retain their meanings. Invalid ranges return `AudioRangeOutOfRange`.

## Review and focused evidence

Independent persistence review found no actionable defect in complete replay,
atomic promotion or context authentication. Independent plan/audio review found
a buffer-accounting defect: allocating a second f32 conversion output would
exceed the gain-only reservation. Conversion now writes into the existing
private Original buffer; the exact 767/768-frame boundary test verifies admission
and repeated successful preparation.

Author and parent review also corrected gain-bearing Partition identity counts
and repeated-insertion admission, and overly conservative replacement budgets.
Regression tests retain source/Preserve clocks and allow valid final inventories
without removing the bounded transient-copy checks. The native integration map
records the need for explicit draft identity, captured absent/stale targets,
scope-sensitive keys and the same delivered-sample comparison window.

The first core run compiled successfully but stopped on two context-test fixture
errors: schema 4 was still used as an unsupported version, and a duplicate-field
injection assumed a different JSON key order. Both fixtures were corrected.
An aggregate context-ingress test was added during that diagnostic run, so its
start manifest does not describe every later test-source byte. Subsequent
focused checks and the final gate provide the qualification boundary.

The first full Clippy pass found two playback fixture constructors inside
`json!` macro tokens that the AST-based default-field update had not visited.
Both now initialize empty treatments; the subsequent full strict Clippy pass
completed successfully. No compiler or test process was killed or restarted.

The full workspace invocation later stopped at an outdated expected error
message in the existing duplicate-node test. The new bounded deserializer
correctly rejected the duplicate with `excess or duplicate node identities`;
the test still expected `duplicate identity key`. Only this assertion changed,
also checking the `InvalidJson` category and printing the actual error on failure.
A formatting check caught the assertion's required multiline layout, which was
corrected. Production source did not change after the initial full gate began.

Cargo's workspace artifact inventory rebuilt that one test. The continuation
retained every already-passing fresh target, then ran the corrected and remaining
native targets once in their package working directories. Exact executable
hashes and per-target exits are retained; doctests use Cargo separately. The
initial failed target's partial passing count is excluded from the final total.

Focused checks completed before the full gate:

| Scope | Result |
| --- | --- |
| Core node gain and frozen contexts | 23 passed |
| Plan unit/owner queries and retained contexts | 28 passed |
| Decoded gain PCM, CLI history and gain migration filter | 17 passed |

PCM coverage includes overlapping envelopes, exact mute ranges, unity and
opposing gain identity, Preserve/Partition processing, Repeat room-tone gaps,
source versus root sound scope, exhausted retained support, odd-sample resumes,
fixed Sequence keys, whole/cold/shuffled limiter reads, revoked muted cached
dependencies, nonfinite output and reservation recovery. CLI coverage compares
actual decoded output, frozen-context authentication and durable mute/undo/redo.

The [fixture evidence](../../tools/audio-qualification/evidence/2026-09-28-authored-gain/fixture/README.md)
records the old executable, exact generation script, all failed preparation
attempts and the final SQL hash. The fixture contains 30 revisions and 21 history
entries with no local absolute paths. No modern snapshots were relabeled as old.

## Final verification

The complete workspace coverage is **2006 passed, 0 failed, 0 ignored**:
2004 native tests across all 140 Cargo test targets, plus 2 doctests across
15 crate groups. The first invocation's 39 successful targets contribute 819
passes; the corrected/remaining 101 targets contribute 1185. Its failed target's
16 partial passes are not counted twice. The original workspace invocation
remains recorded as exit 101; the continuation and doctests both exit 0.

| Final check | Result | Elapsed |
| --- | --- | --- |
| Formatting | Pass | 4.35 s |
| Strict workspace/all-target Clippy recheck | Pass | 32.56 s |
| Current workspace artifact inventory | Pass; only document test rebuilt | 31.47 s |
| Corrected and remaining native targets | 101 targets passed | 606.04 s |
| Workspace doctests | 15 groups passed | 440.04 s |

The initial workspace run took 1229.07 s, including 17 m 34 s compilation.
No passing suite was restarted to repair the diagnostic assertion. Project
instructions now retain a stable build selection, use `--no-fail-fast` for
full milestone runs and describe artifact-verified continuation when necessary.

Base commit: `5fd8b23365df0d203e3a9ae8f5c0c14cc69b6040`.
Final source manifest:
`7e99880521776d6ef6156c95be7a6b8dc2f2b66bfb153a10fd6d9ec2ae8e26e6`.
Only `crates/deadpan-core/tests/document.rs` differs from the initial workspace
run's source manifest; every production source and previously passing target
remained unchanged. The [retained evidence](../../tools/audio-qualification/evidence/2026-09-28-authored-gain/README.md)
includes commands, complete logs, source manifests, executable inventory,
individual continuation exits, derived nonoverlapping totals and byte hashes.

## Qualification limits

Hardware: Apple M5 Max, Mac17,7, 128 GiB; macOS 26.5.2 build 25F84. Rust 1.97.1
and locked dependencies use the pinned development FFmpeg prefix. Check duration
includes compilation and is not a product performance measurement.

Native gain controls, temporary Before/Draft audition, measured waveforms,
keyboard/IME and aesthetic comparison with the retained ImageGen board remain
open. This backend checkpoint does not repeat unchanged optional app-feature or
painted replay tests. No new native GUI, VoiceOver, acoustic listening, long-source
performance, physical display or encoded export qualification is claimed.
All product requirements and Gates A through G remain open or partial.
