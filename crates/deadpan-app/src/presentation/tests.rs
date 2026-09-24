use super::*;
use deadpan_core::{AssetId, ProjectFrame, SourceTimeBase, SourceTimestamp};
use deadpan_render::{
    FrameMetadata, Primaries, Rgba8Frame, Rotation, SampleAspectRatio, SourceColor, Transfer,
};

fn ticket(source: u64, request: u64) -> Ticket {
    Ticket {
        source,
        request,
        transport: None,
    }
}

#[test]
fn transport_stop_keeps_displayed_picture_but_revokes_unsubmitted_decode() {
    let (mut feed, _callback) = deadpan_output::channel().unwrap();
    let old_generation = feed.restart(0).unwrap();
    let mut state = Presentation::default();
    let mut first = ticket(1, 1);
    first.transport = Some(old_generation);
    state.request(first, &Work::Frame(SourceFrameId(4)));
    accept(&mut state, first, picture(4));
    state.presented();
    let mut pending = ticket(1, 2);
    pending.transport = Some(old_generation);
    state.request(pending, &Work::Frame(SourceFrameId(8)));
    accept(&mut state, pending, picture(8));
    assert!(state.needs_render());
    state.invalidate_pending();
    assert!(!state.needs_render());
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
    assert!(
        state
            .receive(Reply {
                ticket: pending,
                picture: Ok(picture(8))
            })
            .is_none()
    );
    let next_generation = feed.restart(900).unwrap();
    let current = Ticket {
        transport: Some(next_generation),
        ..pending
    };
    state.request(current, &Work::Frame(SourceFrameId(12)));
    assert!(
        state
            .receive(Reply {
                ticket: pending,
                picture: Err("late error".into())
            })
            .is_none()
    );
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
    accept(&mut state, current, picture(12));
    state.presented();
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(12)));
}

fn picture(id: u64) -> Picture {
    Picture {
        summary: None,
        id: SourceFrameId(id),
        frame: Some(
            Rgba8Frame::new(
                FrameMetadata {
                    width: 1,
                    height: 1,
                    row_stride_bytes: 4,
                    sample_aspect_ratio: SampleAspectRatio::new(1, 1).unwrap(),
                    rotation: Rotation::None,
                    color: SourceColor {
                        transfer: Transfer::Srgb,
                        primaries: Primaries::Rec709,
                    },
                    pts: SourceTimestamp {
                        ticks: i64::try_from(id).unwrap(),
                        time_base: SourceTimeBase::new(1, 30).unwrap(),
                    },
                },
                vec![24, 48, 72, 255],
            )
            .unwrap(),
        ),
        canvas: None,
    }
}

fn accept(state: &mut Presentation, ticket: Ticket, picture: Picture) {
    assert!(
        state
            .receive(Reply {
                ticket,
                picture: Ok(picture)
            })
            .unwrap()
            .is_ok()
    );
}

#[test]
fn accepted_picture_waiting_for_gpu_survives_a_new_request() {
    let mut state = Presentation::default();
    state.request(ticket(1, 1), &Work::Frame(SourceFrameId(4)));
    accept(&mut state, ticket(1, 1), picture(4));
    assert!(state.needs_render());
    assert!(!state.loading());
    assert_eq!(state.displayed_label(), None);

    state.request(ticket(1, 2), &Work::Frame(SourceFrameId(8)));
    assert!(
        state.needs_render(),
        "new decode must not erase pending GPU work"
    );
    assert!(state.loading());
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing source frame 5")
    );
    assert!(
        state.loading(),
        "presenting an older accepted picture does not finish the new decode"
    );
    assert!(!state.needs_render());
    accept(&mut state, ticket(1, 2), picture(8));
    assert!(state.needs_render());
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing source frame 5")
    );
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing source frame 9")
    );
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(8)));
}

#[test]
fn late_decode_and_error_cannot_replace_the_newest_request() {
    let mut state = Presentation::default();
    state.request(ticket(1, 1), &Work::Frame(SourceFrameId(4)));
    accept(&mut state, ticket(1, 1), picture(4));
    state.presented();
    state.request(ticket(1, 2), &Work::Frame(SourceFrameId(8)));
    state.request(ticket(1, 3), &Work::Frame(SourceFrameId(12)));
    for result in [Ok(picture(8)), Err("old decode failed".into())] {
        assert!(
            state
                .receive(Reply {
                    ticket: ticket(1, 2),
                    picture: result
                })
                .is_none()
        );
        assert!(state.loading());
        assert_eq!(state.picture().unwrap().id, SourceFrameId(4));
        assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
    }
    accept(&mut state, ticket(1, 3), picture(12));
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing source frame 13")
    );
}

#[test]
fn clearing_rejects_old_success_and_failure_even_after_reopening() {
    let mut state = Presentation::default();
    state.request(ticket(1, 1), &Work::Frame(SourceFrameId(4)));
    accept(&mut state, ticket(1, 1), picture(4));
    state.presented();
    state.clear();
    assert!(!state.has_displayed());
    assert!(!state.needs_render());
    assert!(!state.loading());
    assert!(
        state
            .receive(Reply {
                ticket: ticket(1, 1),
                picture: Ok(picture(4))
            })
            .is_none()
    );
    state.request(ticket(2, 2), &Work::Open("replacement.mp4".into()));
    assert!(
        state
            .receive(Reply {
                ticket: ticket(1, 1),
                picture: Err("old source".into())
            })
            .is_none()
    );
    assert!(state.loading());
    assert!(state.picture().is_none());
    accept(&mut state, ticket(2, 2), picture(0));
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing source frame 1")
    );
}

#[test]
fn current_decode_failure_removes_the_old_picture_and_label() {
    let mut state = Presentation::default();
    state.request(ticket(1, 1), &Work::Frame(SourceFrameId(4)));
    accept(&mut state, ticket(1, 1), picture(4));
    state.presented();
    state.request(ticket(1, 2), &Work::Frame(SourceFrameId(8)));
    assert!(
        state
            .receive(Reply {
                ticket: ticket(1, 2),
                picture: Err("missing media".into())
            })
            .unwrap()
            .is_err()
    );
    assert!(!state.has_displayed());
    assert!(state.picture().is_none());
    assert_eq!(state.displayed_label(), None);
    assert!(!state.loading());
    assert!(!state.needs_render());
}

#[test]
fn render_failure_preserves_the_last_displayed_identity_until_a_new_request() {
    let mut state = Presentation::default();
    state.request(ticket(1, 1), &Work::Frame(SourceFrameId(4)));
    accept(&mut state, ticket(1, 1), picture(4));
    state.presented();
    state.request(ticket(1, 2), &Work::Frame(SourceFrameId(8)));
    accept(&mut state, ticket(1, 2), picture(8));
    state.render_failed("GPU error".into());
    assert_eq!(state.error(), Some("GPU error"));
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
    assert!(!state.can_render());
    assert!(
        !state.needs_render(),
        "failed presentation must not show an endless progress spinner"
    );
    state.request(ticket(1, 3), &Work::Frame(SourceFrameId(12)));
    assert_eq!(state.error(), None);
    assert!(state.can_render());
    assert!(state.needs_render());
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
}

fn project_request(
    frame: i64,
    serial: u64,
    revision: &str,
    empty_sequence: bool,
) -> RequestedPicture {
    RequestedPicture {
        ticket: ticket(1, serial),
        location: Location::Project {
            session: 1,
            project: ProjectId::new("project").unwrap(),
            revision: RevisionId::new(revision).unwrap(),
            view: ProjectView::Sequence {
                frame: ProjectFrame(frame),
            },
            empty_sequence,
        },
    }
}

#[test]
fn successful_decode_clears_an_older_gpu_failure_while_waiting() {
    let mut state = Presentation::default();
    state.request(ticket(1, 1), &Work::Frame(SourceFrameId(4)));
    accept(&mut state, ticket(1, 1), picture(4));
    state.request(ticket(1, 2), &Work::Frame(SourceFrameId(8)));
    state.render_failed("older accepted picture failed to render".into());
    assert!(state.loading());
    assert!(state.error().is_some());
    assert!(!state.can_render());
    assert!(
        state
            .receive(Reply {
                ticket: ticket(1, 1),
                picture: Err("stale decoder error".into())
            })
            .is_none()
    );
    assert_eq!(
        state.error(),
        Some("older accepted picture failed to render")
    );
    accept(&mut state, ticket(1, 2), picture(8));
    assert_eq!(state.error(), None);
    assert!(state.can_render());
    assert!(state.needs_render());
    state.presented();
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(8)));
}

#[test]
fn repeated_original_picture_has_distinct_sequence_and_revision_identity() {
    let mut state = Presentation {
        requested: Some(project_request(20, 1, "r1", false)),
        ..Default::default()
    };
    accept(&mut state, ticket(1, 1), picture(4));
    state.presented();
    state.requested = Some(project_request(35, 2, "r1", false));
    accept(&mut state, ticket(1, 2), picture(4));
    assert!(state.needs_render());
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing sequence frame 21")
    );
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing sequence frame 36")
    );
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
    state.requested = Some(project_request(35, 3, "r2", false));
    assert!(
        state
            .receive(Reply {
                ticket: ticket(1, 2),
                picture: Ok(picture(4))
            })
            .is_none()
    );
    accept(&mut state, ticket(1, 3), picture(4));
    assert!(
        state.needs_render(),
        "same image at the same position in a new revision is distinct"
    );
}

#[test]
fn background_and_empty_sequence_do_not_invent_source_frame_identity() {
    let mut state = Presentation::default();
    for (serial, empty) in [(1, false), (2, true)] {
        state.requested = Some(project_request(0, serial, "r1", empty));
        accept(
            &mut state,
            ticket(1, serial),
            Picture {
                summary: None,
                id: SourceFrameId(0),
                frame: None,
                canvas: Some((1920, 1080)),
            },
        );
        assert!(state.needs_render());
        state.presented();
        assert!(state.has_displayed());
        assert_eq!(state.displayed_source_frame(), None);
        assert_eq!(
            state.displayed_label().as_deref(),
            if empty {
                None
            } else {
                Some("Showing sequence frame 1")
            }
        );
    }
}

#[test]
fn source_caption_uses_the_source_view_even_inside_a_project() {
    let request = RequestedPicture {
        ticket: ticket(1, 1),
        location: Location::Project {
            session: 1,
            project: ProjectId::new("project").unwrap(),
            revision: RevisionId::new("revision").unwrap(),
            view: ProjectView::Source {
                asset: AssetId::new("asset").unwrap(),
                frame: SourceFrameId(119),
            },
            empty_sequence: false,
        },
    };
    assert_eq!(request.label().as_deref(), Some("Showing source frame 120"));
}
