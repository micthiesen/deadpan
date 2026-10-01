# Native Slip project-service draft review

## Scope and result

Reviewed `/tmp/deadpan-native-slip-20261001/project-service.patch` against `/tmp/deadpan-native-slip-20261001/project-service-notes.md` and `/tmp/deadpan-source-window-20261001/native-slip-design.md`. Focused on `project/service/slip.rs`, service dispatch/reconciliation wiring, the captured `Target` validation type, and the draft service regressions. No actionable service-layer correctness findings.

No checkout changes, Cargo, native app or test execution by this reviewer. The patch author reports `git apply --check` and rustfmt success; the parent owns runtime verification.

## Checks

- Proposal admission checks nonzero session/draft/change identities, session/project/base revision and pending session change before store preview. A valid newer refinement advances the identity watermark and drops the preceding draft before target/store validation, so a later failure cannot leave an older amount ready. Target validation rechecks the direct child, scope, range and cursor against the exact current workspace.
- The service retains the original `CommandRequest` and uses `preview_source_slip` for both changed and zero-delta proposals. Only a changed preview gets a proposed snapshot; zero movement yields a successful report with `snapshot: None`. Commit consumes the exact matching draft before the store write, refuses zero movement, and submits the retained request so the store can repeat qualification/head checks. Failure therefore cannot cause an implicit retry.
- Success stores a separate receipt before refresh. A refresh failure leaves the durable result available, and the exact ID retransmission branch returns it without another database write. Failed preview/commit and render-history query traffic do not overwrite this receipt. The receipt and other session-local Slip identities clear only when session/project identity changes; ordinary head changes invalidate only a ready draft. This preserves the receipt through Undo while preventing cross-session reuse.
- Service-loop reconciliation runs after requests, asynchronous replies, and refresh. Open/close paths therefore invalidate captured proposals when the workspace/session changes. Commit checks the same session/project and target base revision, and duplicate/stale IDs cannot dispatch another command.
- The tests use database row counts plus a read-only snapshot to check preview neutrality, one-transaction commit, rollback, store qualification recheck, stale ID behavior, duplicate commit, Undo/Redo, refresh failure and close/reopen. The captured cursor at frame 20 lies outside the target range [0,14), which appropriately verifies preservation rather than cursor clamping.

The UI and native preview integration are outside this patch's scope and still need their own review; in particular, they must correlate all replies by proposal ID and require the current proposed picture to be displayed before enabling Apply, as the design requires.
