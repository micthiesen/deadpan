# Scoped native media runner preparation

Prepared scratch-only media.py; no repository files changed, no Cargo, Git,
native/product process, render, or runner invocation performed. Python AST parse
passed. This note records preparation, not qualification.

Frozen runner SHA-256: d63a53ee979ea11089595a475eadf654b14cce49434b597770e29b89982c9fdf

Run after the release native scoped-follow-creep replay succeeds:

    python /tmp/deadpan-scoped-follow-creep-2026-10-24/media.py --project /absolute/actual-native-project.deadpan --bin /absolute/qualified-release-bin --native-report /absolute/native-output/report.json --output /absolute/new-media-output

The output must not exist and must be outside the project. The bin directory
must contain deadpan-cli and the exact deadpan-app whose SHA-256 is in the
release native report. Root supplies actual retained paths, not a derived fixture.

Nine CLI calls: dump, validate, render, verify-export every1/fullPCM0:384384,
copy-portable, portable validate, portable dump, original checkpoint, portable
checkpoint. Read-only Python SQLite observations do not count as CLI calls.
The driver does not author or submit edits. The two requested checkpoints
can update storage bookkeeping; original authored document/history is guarded.

Before rendering it asserts the actual 240-frame nested Repeat/Retime graph,
three inner plays with4f gaps, two outer plays with8f gap, 2/3 Tape map36->24,
point(.58,.5) corrected to(.68,.5) atPTS12012, two actual stable Play2 overrides,
copied node aliases and complete canonical clone equality, no gap overrides,
fixed1.1 template and exactly one1.1->2.2 linear selected Retime at[164,200).
It derives IDs from the dump, never from a presumed revision/node spelling.

The native report must have no failed or skipped scoped checks. Final snapshot
revision, duration240 and cursor182 bind it to the actual dump. The final native
copy-on-write proof supplies the original-node/audio preservation observation;
the runner separately derives clone aliases from actual ordinary child edges
and verifies every clone's complete JSON against its original, permitting only
the selected framing change. Stable iteration order is checked independently.

The final native all-frame rows are checked with Fraction and independent
integer half-even Q32 math:216 live frames,24 captured gap frames,144 fractional
provider coordinates; q=(2j+1)/3, source correction atlocal18, only selected owner
scale varies. Actual provider instance paths/stable IDs/gap issuers are compared.
Native row expected_point is native's expected value, not a CLI observation.
The native passed assertion separately proves actual plan SourcePoint, owner
clock/effective duration, capture and decoded index equality. The runner compares
that expected value independently and retains this distinction in result scope.

verify-export then compares all240 emitted frames against committed shared
references; each emitted provenance must independently be the expected measured
Original ordinal/PTS. Full384384-sample PCM coverage, declared zero offset,
verified-zero alignment, no decoded gaps and ordinary fidelity gates are required.
This is full-window PCM comparison, not every-sample independent lag measurement.
No packet dropping, alignment correction, threshold relaxation or model fallback.

Portable dump must equal original dump. Actual revisions/history/redo/state
counts and identity/request/edit digests, including real Undo/Redo rows, must match
before/after original and portable checkpoints. Counts are measured from the real
native project, not invented constants. Snapshot materialization bookkeeping is
excluded from the logical history digest.

Each command retains exact argv, selected runtime environment, terminal code,
elapsed time, raw stdout/stderr. Executed CLI/app/report hashes are pinned before
work and rechecked. result.json exists only after all checks pass. Native report,
independent240-row oracle and verifier report remain separately attributable.

media-preparation.json pins the inspected design, native handoff/scenario,
prior runner and exact serde/report source inputs. No runtime pass is claimed.
Potential fixture or runtime failures are intended to remain visible for Root;
the driver has only static preparation evidence at this freeze.
