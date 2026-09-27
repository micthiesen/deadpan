# Original moment selection and paste qualification

This increment adds half-open Original selection, a session-local copy and one
atomic paste beside a selected beat in its current ordinary Sequence group.
It does not complete a DP requirement or product gate.

The native path uses `v`, measured-boundary navigation, `y`, then `p`/`P` in Your
edit. Copy changes no revision. The temporal bar uses measured PTS and keeps its
excluded Out boundary explicit. The inspector owns selection buttons; the viewer
keeps the range bar and concise boundary label so picture space is preserved.
The existing [ImageGen moment board](../design/boards/original-moment-reuse-v1.png)
remains the target, with its original prompt and asset retained.

`SpliceSource` names its actual Sequence owner and slot. It preserves group
ownership at coincident boundaries and each shifted physical audio entry. A new
Source starts unbound on the canonical project-origin sample grid. Native store
admission derives exact picture and audio selections from the prepared existing
receipt, checks source freshness and commits history/relevance once.

Core 27/database 33 add the command. A genuine preserved core26/DB32 executable
authored the migration fixture, including nested pauses and pending redo. The
fixture contains 32 revisions and 15 history entries with one redo available;
its producer, original binary identity and complete provenance are retained.
The positive fixture was not made by relabeling modern JSON.

Three independent reviews covered the native keyboard/service path, exact timing
and structural ownership, and prepared admission/migration safety. The general
review found one misleading instruction while Visual selection was active but
empty. The inspector now asks the user to move with h/l; the contributed harness
checks that intermediate state. No material finding remains from those static
reviews. They do not establish native behavior or a visual match.

The new `original-moment` replay uses production keys and widgets. It selects
Original [10,24), copies without changing history, returns to Your edit, pastes
after and before a selected beat, and checks one-step undo. Its pointer path
checks Select/Copy/Paste controls and focused-pane feedback. Real media service
tests separately cover cached and background-prepared paste, stale receipt and
revision rejection, and an explicit nested-group destination. Actual PCM tests
cover fractional-rate phase, shifted Repeat entries, nested group edges and
independent retained audio recipes.

The final required gate ran against 544 unchanged source/configuration files.
Formatting, strict workspace Clippy, the workspace build and `doctor` passed.
Strict `ui-harness` Clippy and all 212 app/harness unit and integration tests
passed. The workspace test command passed 965 tests before its artifact target
failed while binding a Unix socket at
`crates/deadpan-jobs/tests/artifact.rs:200`: OS error 1, `Operation not permitted`.
No test was disabled. The first gate is retained separately because two reviewed
UI files changed during it; the final gate has an unchanged source manifest.

All six prepared-moment store regressions passed, including rollback after an
injected transaction failure, durable undo/redo, unchanged receipts, revoked
preparation sessions, stale inputs and complete generation-relevance observation.
All three new migration regressions passed, including the real DB32 history with
pending redo. The new picture-plan regression and four independent actual-PCM
regressions also passed.

The continuation used `--no-fail-fast` for the remaining packages. It passed
709 tests, with the same one socket failure and no ignored tests. These totals
are per invocation: the jobs unit and artifact targets also occur in the
workspace invocation, so the counts must not be added as distinct tests.

The initial visual attempt stopped in `egui_kittest-0.36.2/src/wgpu.rs:77` with
`CustomNativeAdapterSelectionError("No adapter found")` before app construction.
It produced zero scenario steps, assertions or captures. Its separate shortcut
audit also correctly rejected live Kestrel source drift despite finding no
routing conflicts. Review traced that drift to a Ghostty help-description-only
change in `dc9245c09db3c101a2419c364174449b6e4415ae`. Re-evaluation produced identical
reservation rows; only the fixture's source digest was refreshed. A supplemental
independent review confirmed that the audit was not weakened. The pre-refresh
failure is retained.

The release build succeeded. Both final visual and release audits passed all
3,472 routing cases against 62 reservations and matched the live source digest,
with no conflicts. Both replays stopped before app construction at the missing
Metal adapter: zero scenario steps, assertions, captures or timing samples.
Source hashes remained unchanged throughout the final gate and final replays.
These runs establish no
GUI aesthetic match, input latency improvement, physical shortcut delivery, IME
or VoiceOver qualification. The source identity and separate visual/performance
reports retain those boundaries explicitly.

Logs, exact commands, reviewed source manifests, the incremental diff, the
shortcut refresh and all failed attempts are retained in
`tools/media-qualification/evidence/2026-09-27-moment-paste/`. The final gate follows
the digest refresh. The broader remaining-package results still apply because
all non-app source is unchanged; the only later source change was the reviewed
reservation fixture digest. All ten ImageGen boards and their original prompts
were checked against their recorded hashes.

Persistent/named registers, the general yank operator, Visual replacement,
arbitrary Repeat/Retime occurrence and fractional-clock insertion, preview trims,
native accessibility/IME and full release acceptance remain required. The
project is still an editing foundation. The user-contributed UI harness and all
prior pending work are preserved. Git metadata is read-only in this session;
no commit or push is claimed.
