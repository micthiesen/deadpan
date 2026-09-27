# Original moment paste evidence

See [qualification](../../../../docs/qualification/moment-paste-2026-09-27.md)
for scope, results and limits. Logs and incremental diffs are compressed.
The increment compares the preceding verified checkpoint, not Git HEAD.
`gate-3` checks the final unchanged source; `remaining` covers targets after
the sandbox-denied socket test and the first two UI replay attempts.
`final-ui` repeats visual and release replay after the data-only Kestrel digest
refresh. `gate-1` retains the earlier run and its two reviewed UI corrections;
`gate-2` predates the reviewed shortcut fixture refresh. Non-app source is
unchanged between the remaining-target tests and the final gate.

The contributed harness, ten ImageGen boards and prompts, and all prior pending
work are preserved. No requirement or product gate is promoted. Git metadata
is read-only in this session, so no commit or push is claimed.
