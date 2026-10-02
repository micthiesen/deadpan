use super::*;
use crate::project::trim::{Acknowledged, EventOutcome, Proposal};
use deadpan_core::{FrameRange, NodeId, ProjectId, RevisionId};

fn draft(control: SourceTrimControl, text: &str) -> Draft {
    let target = Target {
        session: 1,
        project: ProjectId::new("amount-entry").unwrap(),
        base_revision: RevisionId::new("entry").unwrap(),
        scope: SequenceScope::default(),
        parent: NodeId::new("root").unwrap(),
        node: NodeId::new("a").unwrap(),
        right: None,
        range: FrameRange::new(ProjectFrame(0), ProjectFrame(10)).unwrap(),
        cursor: ProjectFrame(3),
    };
    Draft {
        capture: Capture {
            target: target.clone(),
            source_cursor: 11,
            selected_source: None,
            pane: Pane::Viewer,
            view: View::Sequence,
            selection: edit_range::Selection::default(),
        },
        input: input::Input::new(target, 7),
        control,
        slip_edge: SourceTrimEdge::In,
        side: JunctionSide::Proposed,
        inspection_serial: 1,
        inspection: None,
        applying: None,
        apply_requested: None,
        waveform: waveform::Display::default(),
        position: None,
        looping: false,
        amount: text.into(),
        amount_control: control,
        amount_dirty: true,
        accept_amount: false,
        amount_events: None,
        focus_pending: false,
        keys: Vec::new(),
        label: "A".into(),
        scope_label: "Your edit".into(),
        error: None,
    }
}

fn set(control: SourceTrimControl, frames: i64) -> Event {
    Event::SetAmount { control, frames }
}

#[test]
fn captured_revision_continuity_rejects_foreign_and_never_reused_identities() {
    let mut capture = draft(SourceTrimControl::In, "+0f").capture;
    let project = capture.target.project.clone();
    let revision = capture.target.base_revision.clone();
    assert!(capture.matches_revision(1, &project, &revision));
    assert!(!capture.matches_revision(2, &project, &revision));
    assert!(!capture.matches_revision(1, &ProjectId::new("foreign").unwrap(), &revision));
    assert!(!capture.matches_revision(1, &project, &RevisionId::new("after-undo").unwrap()));
    capture.target.session = 0;
    assert!(!capture.matches_revision(0, &project, &revision));
}

fn acknowledge(draft: &mut Draft, request: &Proposal, accepted: SourceTrimIntent) {
    assert_eq!(request.events.len(), 1);
    assert!(draft.input.receive(ProposalUpdate {
        id: request.id(),
        acknowledgment: Some(Acknowledged {
            accepted,
            events: vec![EventOutcome {
                event: request.events[0],
                accepted,
                adjustment: None,
                error: None,
            }],
        }),
        // Input acceptance remains authoritative when candidate preparation
        // fails. No picture, GPU or service admission is fabricated here.
        result: Err("candidate preparation unavailable".into()),
    }));
    // Production receive_trim performs this synchronization after Input::receive.
    draft.sync_amount();
}

#[test]
fn finishing_after_clamped_ack_shows_accepted_value_and_preserves_feedback() {
    let mut draft = draft(SourceTrimControl::Slip, "+1000f");
    assert!(draft.input.accept_text(set(SourceTrimControl::Slip, 1000)));
    let request = draft.input.request().unwrap();
    let accepted = SourceTrimIntent {
        slip_frames: 96,
        ..Default::default()
    };
    acknowledge(&mut draft, &request, accepted);
    assert_eq!(
        draft.amount, "+1000f",
        "native text stays intact while dirty"
    );
    let feedback = draft.input.feedback.clone();
    let candidate_error = draft.input.error.clone();

    assert!(draft.finish_amount_entry());
    assert_eq!(draft.amount, "+96f");
    assert_eq!(draft.amount_control, SourceTrimControl::Slip);
    assert!(!draft.amount_dirty);
    assert_eq!(draft.input.accepted, accepted);
    assert_eq!(draft.input.feedback, feedback);
    assert_eq!(
        draft.input.feedback[0].event,
        set(SourceTrimControl::Slip, 1000)
    );
    assert_eq!(draft.input.error, candidate_error);
    assert!(
        draft.input.ready().is_none(),
        "accepting text cannot admit a candidate"
    );
}

#[test]
fn finishing_rebinds_active_control_without_rewriting_pending_prefix() {
    let mut draft = draft(SourceTrimControl::In, "+2f");
    assert!(draft.input.accept_text(set(SourceTrimControl::In, 2)));
    let first = draft.input.request().unwrap();
    // Selecting Out while the native buffer is dirty must not reinterpret the
    // already dispatched In intent. Finishing then binds the new field to Out.
    draft.control = SourceTrimControl::Out;
    draft.sync_amount();
    assert_eq!(draft.amount_control, SourceTrimControl::In);
    assert_eq!(draft.amount, "+2f");

    assert!(draft.finish_amount_entry());
    assert_eq!(draft.amount_control, SourceTrimControl::Out);
    assert_eq!(draft.amount, "+0f");
    assert_eq!(draft.input.abandon_ids(), vec![first.id()]);
    assert_eq!(draft.input.waiting(), 1);
    assert!(draft.input.request().is_none());

    assert!(draft.input.accept_text(set(draft.amount_control, 7)));
    assert_eq!(draft.input.waiting(), 2);
    assert!(draft.input.request().is_none());
    acknowledge(
        &mut draft,
        &first,
        SourceTrimIntent {
            in_frames: 2,
            ..Default::default()
        },
    );
    let second = draft.input.request().unwrap();
    assert_eq!(first.events, vec![set(SourceTrimControl::In, 2)]);
    assert_eq!(second.previous_change, Some(first.change));
    assert_eq!(second.events, vec![set(SourceTrimControl::Out, 7)]);
    assert!(second.change > first.change);
}

#[test]
fn finishing_before_ack_preserves_requested_amount_and_later_syncs_accepted_value() {
    let mut draft = draft(SourceTrimControl::Slip, "+1000f");
    assert!(draft.input.accept_text(set(SourceTrimControl::Slip, 1000)));
    let request = draft.input.request().unwrap();

    assert!(draft.finish_amount_entry());
    assert_eq!(
        draft.amount, "+0f",
        "field shows the currently accepted tuple"
    );
    assert_eq!(draft.input.waiting(), 1);
    assert!(draft.input.request().is_none());
    assert_eq!(request.events, vec![set(SourceTrimControl::Slip, 1000)]);
    acknowledge(
        &mut draft,
        &request,
        SourceTrimIntent {
            slip_frames: 96,
            ..Default::default()
        },
    );
    assert_eq!(draft.amount, "+96f");
    assert_eq!(draft.amount_control, SourceTrimControl::Slip);
    assert!(!draft.amount_dirty);
}

#[test]
fn invalid_or_retained_invalid_text_cannot_finish_or_rebind_the_field() {
    for (text, error) in [
        ("+f", None),
        (
            "+7f",
            Some("invalid field must be corrected through native input"),
        ),
    ] {
        let mut draft = draft(SourceTrimControl::In, text);
        draft.control = SourceTrimControl::Out;
        if let Some(error) = error {
            draft.input.set_text_error(error.into());
        }
        let text_error = draft.input.text_error.clone();
        let accepted = draft.input.accepted;

        assert!(!draft.finish_amount_entry());
        assert_eq!(draft.amount, text);
        assert_eq!(draft.amount_control, SourceTrimControl::In);
        assert!(draft.amount_dirty);
        assert_eq!(draft.input.text_error, text_error);
        assert_eq!(draft.input.accepted, accepted);
        assert_eq!(draft.input.waiting(), 0);
    }
}
