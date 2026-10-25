# Active navigation cancellation, scratch-only proposal

No repository edit. The full107 run/sourceguard54 remains untouched. No
Cargo/App/replay/native/detector execution. Only scratch candidate was formatted.

Observed all-replays-first/faces/report.json:15checks, Edit11, targets0,
revision unchanged, error null. Steps show the actual L navigation requests
frame11, Camera/pending closes and service reports Cancelling target search.
The old harness incorrectly required a stale-result error after normal active
cancellation.

Exact production path:
- preview.rs request_picture calls cancel_camera before requesting new picture;
- camera.rs cancel_camera takes proposal_zoom and calls cancel_proposal_request
  for the captured session/ticket;
- cancel_proposal_request submits typed Operation::Cancel only in that session
  and without superseding a newer job;
- service/targets.rs accepts that exact proposal ticket and sets cancellation;
- service/targets/picture_proposals.rs scripted_wait observes the cancel flag;
  pump_proposals forces ProposalOutcome::Cancelled whenever that run's flag was
  set, then publishes its terminal state;
- receive_targets calls finish_proposal_zoom, but navigation already consumed
  its pending command, so no stale refusal is manufactured.
This is the intended active cancellation path, not evidence of a product defect.

Prepared faces.rs patch first captures complete document, both cursors, scope,
selected beat and active exact proposal job. The existing600ms script and wait
budget are unchanged; the wait additionally requires job.outcome=None so the
witness actually navigates during an active request. After L and production
completion it requires exact Cancelled outcome on the same ticket/session/
revision/asset/PTS/Faces mode, absent command/Camera pending, no error, complete
document equality (including all framing/targets/assets), explicit framingNone
and targets0, unchanged Original cursor/scope/selection, Edit+1 and revision.
No global notice string is required because its current cancellation notice is
not the terminal outcome's authority.

Separate stale-context guards remain unchanged:
camera.rs zoom_commands_act_only_on_the_context_captured_at_entry;
proposal_picker.rs stale_result_release_preserves_entries_and_unblocks_the_pending_gate;
the actual regions replay's injected-presentation stale batch and fresh retry;
service picture_proposals tests retaining old-revision proposal identity.
The active-cancel patch does not remove or relax those guards.

Later faces and camera_picker scenario checks were read against current product
strings, scripted Both status/origin, geometry ordering and public face facade.
No further stale equality was found: face:3/face:0 refusals retain face-only
semantics, empty and12face0region Camera statuses carry Scripted origin, and
saved-target digits/geometry/paging/Enter/Escape/Undo/full Follow transition
assertions remain. A late closed-Camera completion accepts any terminal outcome
but still requires no reopen/no target/revision edit; active cancel is legal.

Files:
- app-harness-faces-cancel-before.rs.txt: unchanged current repository source
- app-harness-faces-cancel-candidate.rs.txt: formatted exact proposed source
- app-harness-faces-cancel.patch: exact unified patch
- app-harness-faces-cancel-prepared.json: original/candidate/patch/report pins

Root should apply only after full107 finishes, or explicitly authorize the
child. This is analysis/proposed assertion code, not a passing new replay.
