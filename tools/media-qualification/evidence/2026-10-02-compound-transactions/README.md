# Compound transaction evidence

See [scope, results and limits](../../../../docs/qualification/compound-transactions-2026-10-02.md).

Each recorded command JSON contains exact arguments, exit status, starting
commit, tracked diff hash and complete before/after source inventory. The
inventories include untracked code. Logs retain failures and warnings;
`SHA256SUMS` covers every evidence file except itself. `test-summary.json`
counts each run separately. Suites overlap and their counts must not be added.

`core-initial.log` and `core-second.log` are early diagnostic runs without a
stable full-source inventory. The first exposed missing Serde `rc` support and
an ambiguous error conversion; the second passed after those corrections.
Neither is used to qualify the final source.

`workspace-initial` failed to compile because an app test called a private
helper. The corrected fixture allocates its own deterministic identities.
Unused store imports were also removed. `compound-focused` then passed the
selected compound tests before the final replay-limit hardening.

`replay-limit-witness` records a real failure: a directly constructed request
could save history that exceeded the replay reader's value limit. The fixed
store checks its canonical request through that reader before writing.
`replay-limit-corrected` passes all store library tests, including rejection
without writes for ordinary and compound requests.

`workspace`, `app-ui-tests`, `fmt-final`, `clippy-workspace` and `clippy-ui`
qualify the final source inventory. Workspace lint overlapped the final
workspace documentation tests; both recorded unchanged source. Later Cargo
checks ran sequentially. The checks used pinned Rust 1.97.1 and
`/tmp/deadpan-ui-ffmpeg/prefix`.

Independent reviewers inspected core and store behavior without running Cargo
or opening the app. No ordinary native window or rendered replay was launched
for this backend increment. Physical input, IME and visual qualification are
outside this evidence.
