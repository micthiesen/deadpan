# Render qualification source review

Result: no actionable implementation defect found in the reviewed qualification
example and integration fixtures.

Scope: the root `qualify_project_picture.rs` diff, its new `worker.rs` module,
`tests/render_worker.rs`, and `tests/render_worker/fixture.py`. Read-only source
and diff inspection; no build, formatter, test, or native run was performed.

Evidence checked:

- The native example calls the production `render_worker::prepare` entry with
  an explicit canonical executable. The real-child rejection integration test
  selects Cargo's `CARGO_BIN_EXE_deadpan-cli`, checks exact child diagnostics and
  exit code, and verifies that no output file or authored mutation occurred.
- The example compares every frame in each captured range with a newly opened
  direct production session for that same revision and range. It compares all
  bytes and exact `OutputFrameTiming`, records per-frame hashes, writes both
  complete streams, and checks their aggregate hash against the admitted
  artifact declaration after the worker workspace is gone.
- The odd reference filename matches the direct producer's
  `odd-reference-20.i420`. Generated references match every absolute project
  frame written by the direct 30-frame fixture. These files contain the existing
  independent numerical color/filter oracle, with shared geometry explicitly
  documented. The worker comparison retains that oracle's one-code tolerance.
- The latest live-history check requires progress strictly between zero and
  total before claiming its edit/undo/redo exercise. A separate current-revision
  render must differ from the captured bytes, preventing an ineffective mutation
  from passing. Both history and cancellation callbacks run before host result
  admission; progress alone is not a separate OS process-liveness measurement.
- Cancellation requires nonterminal progress and the specific Cancelled result,
  then performs another successful isolated request. The report explicitly does
  not claim measured hard-timeout latency or OS sandboxing.
- Fault fixtures target distinct admission stages: broken framing and stale
  identities; nonzero exit/post-terminal output; wrong hash/short file; valid
  hashes carrying illegal luma or chroma. The accepted fixture's bytes are read
  after its package is removed. Python is explicitly limited to test fixtures,
  not represented as real rendering evidence.

Evidence scope to retain in the handoff:

- At review time the isolated Original range is [20,63), covering retimed Source,
  a Background Repeat gap, and the next play's first frame. The captured Freeze
  at [122,125) and final Background at [125,128) are covered by the earlier direct
  harness, not this isolated case. Describe that scope accurately or extend the
  range before claiming isolated Freeze coverage.
- The example records the runtime path. Binding that path to the freshly built
  CLI binary and its unchanged executable hash remains the outer qualification
  runner's responsibility. Source inspection does not prove that a native run
  used the intended artifact or passed.
- Encoded media, rendered audio, publication, durable jobs, native Render UI,
  physical display behavior, and performance qualification remain excluded.

Follow-up source inspection: the parent added an isolated
`captured-freeze-and-background` case for [121,128), which covers the prior
Freeze/Background scope gap and brings the selected worker cases to 82 frames.
The outer `run-metal.py` also resolves both example and CLI paths from the exact
Cargo artifact records and records pre/post executable SHA-256 values. These
address the scope/provenance notes above; the actual run remains pending.
