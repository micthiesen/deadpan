# Structural speed editing evidence

See [qualification](../../../../docs/qualification/retime-editing-2026-09-27.md).
Logs and the incremental source patch are compressed. The increment compares the
preceding verified Original audition checkpoint, not Git HEAD. Prior pending
work, including the contributed UI harness, is retained.

The final PCM oracle correction landed during the first workspace Clippy run.
`gate-1/source-before-tests.json` seals all source/configuration before workspace
tests started. Remaining gate steps use that unchanged source. The run exposed
a stale CLI schema assertion alongside the known sandbox socket-bind failure.
After the gate, only the CLI doctor labels and its schema test changed. Final
checks repeat formatting, workspace lint, all 17 CLI command tests and doctor,
then attempt production GUI replay. `final-source.json` records that final
source; its two changes from the gate seal and independent review hashes are
checked explicitly. The workspace run itself is not reported as all passing.

The GUI replay requires Metal. Its report records actual startup, executed
steps and captures; the scenario's existence is not a claim that it ran.
Independent audio tests decode actual PCM, use canonical DSP references and
check retained processing history. No physical display, native accessibility,
listening, long-input Preserve or encoded export qualification is claimed.
ImageGen targets and prompts remain intact. Git metadata is read-only, so no
commit or push is claimed.
