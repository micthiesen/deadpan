# Native integration

Implemented the internal engineering-render service adapter in the shared checkout.

- `ProjectRequest::Render` wraps the shared Start/Retry/Reconcile/Cancel API with a nonzero ticket and captured native session/project. Start uses its exact current revision; recovery and cancellation retain operational identities independently of the edited revision.
- Native configuration derives package from the workspace and executable from `current_exe`, routing workers with `--headless`. Only trusted internal requests provide engineering policy and limits. No visible Render control or public codec setting was added.
- `ProjectUpdate.render` retains separate command outcome, coordinator status and service diagnostic. Render requests/progress preserve editor completions, proposals and generic editor errors.
- The existing service loop polls one coordinator turn alongside editing. User admission stays short. Close/switch defer replacement until `can_release_writer()` and checked drain; valid Open is prepared before cancellation so a failed Open leaves active work intact.
- Shutdown continues polling, preserves admitted command completion, releases the writer only after coordinator authority is safe, then signals `is_shutdown_complete`. The existing closing notice exposes unresolved coordinator diagnostics.
- New `project/tests/render.rs` contains five actor tests for held capture-result admission with concurrent edits/undo, preserved proposals/feedback, stale identities, deferred close/switch, and shutdown. These pause the native owner's poll only; capture may already be complete. They cancel before encode dispatch, and make no encoder/performance claim.
- Added approved `deadpan-jobs` dev dependency to construct exact test identities.

No builds, tests, formatters, native runs, commits or pushes were executed by this agent. Parent owns all execution and native full-workflow qualification. Source freeze handed to parent before independent read-only teardown review.

Review corrections: shutdown preserves the final admitted command workspace in its mailbox, matching the existing test contract; explicit Close clears it. Deferred document-session changes clear transient editor command feedback, while operational render requests do not. Safe release uses the coordinator's `can_release_writer`, never progress, an empty channel, or `cleanup_confirmed` alone.
