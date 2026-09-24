# Framing evidence

See the [qualification record](../../../../docs/qualification/framing-2026-09-24.md)
for behavior, exact build identities and limits. No product requirement or gate
is completed by this evidence.

- `gate-1/`: retained initial Clippy failure and source hashes.
- `gate-2/`: passing repository gate before native visual corrections.
- `gate-3/`: final passing five-command gate, 1,323 tests and unchanged source hashes.
- `gate.py`: exact command runner; `.log.gz` files preserve original log bytes.
- `review.json`: independent review scopes, findings, dispositions and parent fixes.
- `native-gui/`: two app build identities, source manifests, synthetic fixture
  identity, native observations, read-only document comparisons and final validation.
- `framing-metal-1.json`: actual 84-case Metal/CPU comparison.
- `plan-render-final-hashes.json` and `plan-render-handoff.md`: final renderer source
  identities and the later scope-cap correction's limited impact on that report.
- `design-integrity.json`: all nine ImageGen board/prompt pairs verified.
- `old-binary/`: actual core-17/database-23 producer and command output. The SQL
  fixture and its provenance live under `crates/deadpan-store/tests/fixtures/`.
- `core-design.md` and `render-design.md`: bounded implementation contracts.
- Scoped core/store and Camera logs retain intermediate checks and the initial
  egui test-harness failure, separately from the final gate.

Native captures were visually inspected in the session and are not included here.
No application binary, user project package, private footage or model is committed.
Scratch paths record development provenance; they are not application dependencies.
