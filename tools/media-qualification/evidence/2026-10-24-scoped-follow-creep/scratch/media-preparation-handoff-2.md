# Scoped media driver preparation 2

Frozen media.py SHA-256: 659c5930c45955ea84e14e44a6e08dc6aff254eed538f328ed88c1950d3f4a7f

Actual native-scoped-first has155/155 scoped checks passed. First media attempt
failed at the blanket skipped-list assertion, before any CLI operation. Its
output and receipts remain untouched. Exact first driver is retained as
media-first-driver.py.txt; original preparation files remain unchanged and
have additional verbatim media-first-preparation copies.

This revision requires exactly the two observed standard harness exclusions:
- OS picker behavior: only its selected path is scripted
- Physical display presentation, VoiceOver, OS IME delivery and audio device output

Any unknown, missing, extra or reordered exclusion refuses. No scoped assertion
is weakened: every check must pass, and no failure finding is allowed.
The exclusions are retained in result.evidence_scope.native_owner_verification_exclusions.

All nine CLI operations, fixture/clock/Q32/clone guards, report/binary/revision
bindings, history checks, every-picture/fullPCM zero-offset requirements and
portable comparisons are unchanged. Use a new media-second output directory
with the command pattern in media-preparation-handoff.md.

Python AST parse passed. This revised runner was not executed. No repository,
Cargo, Git, native or product changes or reruns were made in this preparation.
Original prep's no-skipped wording describes version1; this supplement corrects
that driver-only admission while preserving the original record.
