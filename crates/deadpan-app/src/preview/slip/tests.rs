use super::*;
use deadpan_core::{FrameRange, ProjectId};

fn draft() -> Draft {
    let target = Target {
        session: 7,
        project: ProjectId::new("slip-ui").unwrap(),
        base_revision: RevisionId::new("base").unwrap(),
        scope: SequenceScope::default(),
        parent: NodeId::new("root").unwrap(),
        node: NodeId::new("selected").unwrap(),
        range: FrameRange::new(ProjectFrame(10), ProjectFrame(20)).unwrap(),
        cursor: ProjectFrame(99),
    };
    Draft {
        capture: Capture {
            target: target.clone(),
            source_cursor: 42,
            selected_source: None,
            pane: Pane::Sequence,
            view: View::Sequence,
            selection: edit_range::Selection::default(),
        },
        proposal: Proposal {
            target,
            draft: 11,
            change: 1,
            delta_frames: 3,
        },
        prepared: None,
        pending: None,
        issued: None,
        dirty: true,
        applying: false,
        invalidated: false,
        error: None,
        amount: "+3f".into(),
        limits: None,
        before: false,
        inspection: ProjectFrame(19),
        observed: None,
        focus_pending: false,
        keys: vec![],
        label: "Selected".into(),
        scope_label: "Your edit".into(),
    }
}

#[test]
fn inspection_clamps_only_the_temporary_picture_not_either_real_cursor() {
    let mut draft = draft();
    assert_eq!(inspection_frame(&draft.capture.target), ProjectFrame(19));
    assert_eq!(draft.capture.target.cursor, ProjectFrame(99));
    assert_eq!(draft.capture.source_cursor, 42);
    draft.capture.target.cursor = ProjectFrame(4);
    assert_eq!(inspection_frame(&draft.capture.target), ProjectFrame(10));
    assert_eq!(draft.capture.target.cursor, ProjectFrame(4));
}

#[test]
fn reverse_after_clamp_and_batched_nudges_keep_every_step_and_reject_overflow() {
    assert_eq!(nudge(500, -1, Some((-3, 5))), Ok(4));
    assert_eq!(nudge(-500, 1, Some((-3, 5))), Ok(-2));
    let mut amount = 0;
    for step in [1, 1, 10, -1, -1] {
        amount = nudge(amount, step, Some((-3, 5))).unwrap();
    }
    assert_eq!(amount, 3);
    assert!(nudge(i64::MAX, 1, None).is_err());
    assert!(nudge(i64::MIN, -1, None).is_err());
}

#[test]
fn invalid_text_retires_the_old_amount_and_late_failure_cannot_relabel_it() {
    let mut draft = draft();
    let old = draft.proposal.id();
    draft.pending = Some(old.clone());
    draft.issued = Some(old.clone());
    draft.observed = Some((
        old.clone(),
        draft.inspection,
        Ticket {
            transport: None,
            source: 1,
            request: 2,
        },
    ));
    draft.changed(Err("Enter whole frames".into()));
    assert!(!draft.can_apply());
    assert!(draft.observed.is_none());
    assert!(!draft.dirty);
    assert!(!draft.receive(ProposalUpdate {
        id: old,
        result: Err("old receipt error".into())
    }));
    assert_eq!(draft.error.as_deref(), Some("Enter whole frames"));
    assert!(draft.pending.is_none());
    draft.changed(Ok(-2));
    assert_eq!(draft.proposal.change, 3);
    assert_eq!(draft.proposal.delta_frames, -2);
    assert!(draft.dirty);
    assert!(draft.error.is_none());
    assert!(!draft.can_apply());
}

#[test]
fn unrelated_reply_cannot_consume_current_pending_or_enable_apply() {
    let mut draft = draft();
    let id = draft.proposal.id();
    draft.pending = Some(id.clone());
    let mut other = id.clone();
    other.draft += 1;
    assert!(!draft.receive(ProposalUpdate {
        id: other,
        result: Err("other draft".into())
    }));
    assert_eq!(draft.pending, Some(id));
    assert!(draft.error.is_none());
    assert!(!draft.can_apply());
}
