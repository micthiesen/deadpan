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
                #[cfg(feature = "ui-harness")]
                timing: None,
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
                #[cfg(feature = "ui-harness")]
                timing: None,
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
        framing: Vec::new(),
        framing_gap: false,
        follow_point: None,
        picture_context: None,
    }
}

fn accept(state: &mut Presentation, ticket: Ticket, picture: Picture) {
    assert!(
        state
            .receive(Reply {
                #[cfg(feature = "ui-harness")]
                timing: None,
                ticket,
                picture: Ok(picture)
            })
            .unwrap()
            .is_ok()
    );
}

fn camera_picture() -> (Picture, deadpan_core::InstancePath) {
    let mut value = picture(4);
    let scope = deadpan_core::InstancePath {
        node: deadpan_core::NodeId::new("pause").unwrap(),
        repeats: Vec::new(),
    };
    value.canvas = Some((1920, 1080));
    value.framing.push(deadpan_plan::PictureFraming {
        instance: scope.clone(),
        local_position: deadpan_core::ExactRatio::new(1, 2).unwrap(),
        duration: deadpan_core::FrameDuration::new(11).unwrap(),
        pose: None,
        escalation: false,
    });
    (value, scope)
}

#[test]
fn camera_geometry_redraws_same_bytes_and_only_submission_advances_display() {
    let mut state = Presentation {
        requested: Some(project_request(20, 1, "r1", false)),
        ..Default::default()
    };
    let (value, scope) = camera_picture();
    accept(&mut state, ticket(1, 1), value);
    let revision = RevisionId::new("r1").unwrap();
    assert_eq!(
        state.stable_sequence_ticket(1, &revision, ProjectFrame(20)),
        None
    );
    state.presented();
    assert_eq!(
        state.stable_sequence_ticket(1, &revision, ProjectFrame(20)),
        Some(ticket(1, 1))
    );
    let bytes = state.picture().unwrap().frame.as_ref().unwrap() as *const Rgba8Frame;
    let pose = deadpan_core::FramingPose {
        center_x: deadpan_core::ExactRatio::new(58, 100).unwrap(),
        center_y: deadpan_core::ExactRatio::new(46, 100).unwrap(),
        scale: deadpan_core::ExactRatio::new(135, 100).unwrap(),
    };
    state
        .set_framing_pose(ticket(1, 1), &scope, Some(pose))
        .unwrap();
    assert!(state.needs_render());
    assert_eq!(state.displayed.as_ref().unwrap().geometry_revision, 0);
    assert_eq!(
        state.picture().unwrap().frame.as_ref().unwrap() as *const Rgba8Frame,
        bytes
    );
    state.render_failed("GPU temporarily unavailable".into());
    assert_eq!(state.displayed.as_ref().unwrap().geometry_revision, 0);
    // Escape restores the captured entry operation and can recover a failed draw.
    state.set_framing_pose(ticket(1, 1), &scope, None).unwrap();
    assert!(state.needs_render());
    assert!(state.can_render());
    state.presented();
    assert_eq!(state.displayed.as_ref().unwrap().geometry_revision, 2);
    assert!(!state.needs_render());
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
}

#[test]
fn camera_ticket_cannot_modify_retained_picture_after_a_new_request() {
    let mut state = Presentation {
        requested: Some(project_request(20, 1, "r1", false)),
        ..Default::default()
    };
    let (value, scope) = camera_picture();
    accept(&mut state, ticket(1, 1), value);
    state.presented();
    state.requested = Some(project_request(20, 2, "r2", false));
    state.loading = true;
    assert!(
        state
            .set_framing_pose(ticket(1, 1), &scope, Some(Default::default()))
            .is_err()
    );
    assert_eq!(state.picture().unwrap().framing[0].pose, None);
    assert_eq!(
        state.stable_sequence_ticket(1, &RevisionId::new("r1").unwrap(), ProjectFrame(20)),
        None
    );
    let (mut current, _) = camera_picture();
    current.framing[0].pose = Some(Default::default());
    accept(&mut state, ticket(1, 2), current);
    assert!(state.set_framing_pose(ticket(1, 1), &scope, None).is_err());
    assert_eq!(
        state.picture().unwrap().framing[0].pose,
        Some(Default::default())
    );
    state.presented();
    assert_eq!(
        state.stable_sequence_ticket(2, &RevisionId::new("r2").unwrap(), ProjectFrame(20)),
        None
    );
    assert_eq!(
        state.stable_sequence_ticket(1, &RevisionId::new("r2").unwrap(), ProjectFrame(21)),
        None
    );
    assert_eq!(
        state.stable_sequence_ticket(1, &RevisionId::new("r2").unwrap(), ProjectFrame(20)),
        Some(ticket(1, 2))
    );
    state.invalidate_pending();
    assert!(state.set_framing_pose(ticket(1, 2), &scope, None).is_err());
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
                    #[cfg(feature = "ui-harness")]
                    timing: None,
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
                #[cfg(feature = "ui-harness")]
                timing: None,
                ticket: ticket(1, 1),
                picture: Ok(picture(4))
            })
            .is_none()
    );
    state.request(ticket(2, 2), &Work::Open("replacement.mp4".into()));
    assert!(
        state
            .receive(Reply {
                #[cfg(feature = "ui-harness")]
                timing: None,
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
                #[cfg(feature = "ui-harness")]
                timing: None,
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
                #[cfg(feature = "ui-harness")]
                timing: None,
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
                #[cfg(feature = "ui-harness")]
                timing: None,
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
                framing: Vec::new(),
                framing_gap: false,
                follow_point: None,
                picture_context: None,
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

#[test]
fn proposed_content_identity_never_becomes_a_committed_display_or_camera_target() {
    let make = |change| RequestedPicture {
        ticket: ticket(1, change),
        location: Location::Proposed {
            session: 1,
            project: ProjectId::new("project").unwrap(),
            revision: RevisionId::new("proposal").unwrap(),
            content: deadpan_playback::ContentIdentity::Proposed {
                base_revision: RevisionId::new("base").unwrap(),
                draft: 7,
                change,
            },
            view: ProjectView::Sequence {
                frame: ProjectFrame(12),
            },
        },
    };
    let mut state = Presentation {
        requested: Some(make(1)),
        ..Default::default()
    };
    accept(&mut state, ticket(1, 1), picture(4));
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing proposed edit frame 13")
    );
    assert_eq!(
        state.stable_sequence_ticket(1, &RevisionId::new("proposal").unwrap(), ProjectFrame(12)),
        None
    );
    state.requested = Some(make(2));
    accept(&mut state, ticket(1, 2), picture(4));
    assert!(
        state.needs_render(),
        "a new proposed change keeps its own submitted identity even with identical pixels"
    );
    state.presented();
    state.requested = Some(project_request(12, 3, "proposal", false));
    accept(&mut state, ticket(1, 3), picture(4));
    assert!(
        state.needs_render(),
        "matching public revision and pixels cannot collapse proposed and committed domains"
    );
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing proposed edit frame 13")
    );
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing sequence frame 13")
    );
}

fn copied_request(frame: i64, serial: u64, start: i64) -> RequestedPicture {
    RequestedPicture {
        ticket: ticket(1, serial),
        location: Location::Copied {
            source: CopiedViewId {
                copy: crate::project::slice::CopyId {
                    session: 1,
                    project: ProjectId::new("copied-presentation").unwrap(),
                    source_revision: RevisionId::new("historical-source").unwrap(),
                    request: 10,
                    persisted_version: None,
                },
                parent: deadpan_core::NodeId::new("historical-owner").unwrap(),
                range: deadpan_core::FrameRange::new(ProjectFrame(start), ProjectFrame(start + 3))
                    .unwrap(),
            },
            frame: ProjectFrame(frame),
        },
    }
}

#[test]
fn copied_identical_images_advance_only_the_submitted_historical_clock_and_cannot_enter_camera() {
    let mut state = Presentation {
        requested: Some(copied_request(0, 1, 120)),
        ..Default::default()
    };
    accept(&mut state, ticket(1, 1), picture(20));
    assert_eq!(state.displayed_label(), None);
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing copied Edit frame 121")
    );
    assert_eq!(
        state.stable_sequence_ticket(
            1,
            &RevisionId::new("historical-source").unwrap(),
            ProjectFrame(120)
        ),
        None
    );
    state.requested = Some(copied_request(1, 2, 120));
    accept(&mut state, ticket(1, 2), picture(20));
    assert!(
        state.needs_render(),
        "same source image still occupies a different owner clock"
    );
    state.render_failed("replacement allocation failed".into());
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing copied Edit frame 121")
    );
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(20)));
    accept(&mut state, ticket(1, 2), picture(20));
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing copied Edit frame 122")
    );
    assert_eq!(state.error(), None);
    assert_eq!(
        state.stable_sequence_ticket(
            1,
            &RevisionId::new("historical-source").unwrap(),
            ProjectFrame(121)
        ),
        None
    );
}

#[test]
fn refined_copied_identity_rejects_old_success_and_failure_and_accepts_explicit_background() {
    let mut state = Presentation {
        requested: Some(copied_request(0, 1, 20)),
        ..Default::default()
    };
    accept(&mut state, ticket(1, 1), picture(20));
    state.presented();
    state.requested = Some(copied_request(0, 2, 123));
    for result in [Ok(picture(20)), Err("superseded source failure".into())] {
        assert!(
            state
                .receive(Reply {
                    #[cfg(feature = "ui-harness")]
                    timing: None,
                    ticket: ticket(1, 1),
                    picture: result,
                })
                .is_none()
        );
        assert_eq!(
            state.displayed_label().as_deref(),
            Some("Showing copied Edit frame 21")
        );
    }
    let mut background = picture(0);
    background.frame = None;
    background.canvas = Some((101, 61));
    accept(&mut state, ticket(1, 2), background);
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing copied Edit frame 21")
    );
    state.presented();
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing copied Edit frame 124")
    );
    assert_eq!(state.displayed_source_frame(), None);
    assert_eq!(
        state.stable_sequence_ticket(
            1,
            &RevisionId::new("historical-source").unwrap(),
            ProjectFrame(123)
        ),
        None
    );
}

#[test]
fn proposed_apply_gate_requires_exact_current_submitted_picture_and_recovers_after_failure() {
    let project = ProjectId::new("slip-project").unwrap();
    let revision = RevisionId::new("candidate").unwrap();
    let content = |change| deadpan_playback::ContentIdentity::Proposed {
        base_revision: RevisionId::new("base").unwrap(),
        draft: 8,
        change,
    };
    let requested = |change| RequestedPicture {
        ticket: ticket(9, change),
        location: Location::Proposed {
            session: 7,
            project: project.clone(),
            revision: revision.clone(),
            content: content(change),
            view: ProjectView::Sequence {
                frame: ProjectFrame(12),
            },
        },
    };
    let ready = |state: &Presentation, change| {
        state.stable_proposed_ticket(7, &project, &revision, &content(change), ProjectFrame(12))
    };
    let mut state = Presentation {
        requested: Some(requested(1)),
        ..Default::default()
    };
    accept(&mut state, ticket(9, 1), picture(4));
    assert!(ready(&state, 1).is_none(), "decode is not display");
    state.presented();
    assert_eq!(ready(&state, 1), Some(ticket(9, 1)));
    assert!(
        state
            .stable_sequence_ticket(7, &revision, ProjectFrame(12))
            .is_none()
    );
    assert!(
        state
            .stable_proposed_ticket(
                7,
                &ProjectId::new("other").unwrap(),
                &revision,
                &content(1),
                ProjectFrame(12)
            )
            .is_none()
    );
    assert!(
        state
            .stable_proposed_ticket(8, &project, &revision, &content(1), ProjectFrame(12))
            .is_none()
    );
    assert!(
        state
            .stable_proposed_ticket(7, &project, &revision, &content(1), ProjectFrame(13))
            .is_none()
    );
    assert!(ready(&state, 2).is_none());
    state.render_failed("resize failed".into());
    assert!(ready(&state, 1).is_none());
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
    state.requested = Some(requested(2));
    accept(&mut state, ticket(9, 2), picture(5));
    assert!(ready(&state, 2).is_none());
    state.presented();
    assert_eq!(ready(&state, 2), Some(ticket(9, 2)));
    state.invalidate_pending();
    assert!(
        ready(&state, 2).is_none(),
        "cancel revokes Apply but retains displayed image"
    );
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(5)));
}

#[test]
fn stopped_slip_decode_failure_retains_display_and_geometry_without_reviving_apply() {
    let project = ProjectId::new("slip-project").unwrap();
    let revision = RevisionId::new("slip-candidate").unwrap();
    let content = |change| deadpan_playback::ContentIdentity::Proposed {
        base_revision: RevisionId::new("base").unwrap(),
        draft: 8,
        change,
    };
    let request = |change, frame| RequestedPicture {
        ticket: ticket(9, change),
        location: Location::Proposed {
            session: 7,
            project: project.clone(),
            revision: revision.clone(),
            content: content(change),
            view: ProjectView::Sequence {
                frame: ProjectFrame(frame),
            },
        },
    };
    let ready = |state: &Presentation, change, frame| {
        state.stable_proposed_ticket(
            7,
            &project,
            &revision,
            &content(change),
            ProjectFrame(frame),
        )
    };
    let mut state = Presentation {
        requested: Some(request(1, 12)),
        ..Default::default()
    };
    let mut first = picture(4);
    first.canvas = Some((101, 61));
    accept(&mut state, ticket(9, 1), first);
    state.decoded.as_mut().unwrap().geometry_revision = 3;
    state.presented();
    assert_eq!(ready(&state, 1, 12), Some(ticket(9, 1)));
    let old_label = state.displayed_label();
    state.requested = Some(request(2, 13));
    state.loading = true;
    let failed = state.receive_retaining_display(Reply {
        #[cfg(feature = "ui-harness")]
        timing: None,
        ticket: ticket(9, 2),
        picture: Err("current Slip decode failed".into()),
    });
    assert!(matches!(failed, Some(Err(error)) if error == "current Slip decode failed"));
    assert_eq!(state.requested.as_ref(), Some(&request(2, 13)));
    assert_eq!(state.error(), Some("current Slip decode failed"));
    assert!(!state.loading());
    assert!(state.picture().is_none());
    assert!(!state.needs_render());
    assert!(state.has_displayed());
    assert_eq!(state.displayed_label(), old_label);
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
    assert_eq!(state.canvas(), Some((101, 61)));
    assert_eq!(state.displayed.as_ref().unwrap().geometry_revision, 3);
    assert!(ready(&state, 1, 12).is_none());
    assert!(ready(&state, 2, 13).is_none());
    for picture in [Ok(picture(90)), Err("stale Slip failure".into())] {
        assert!(
            state
                .receive_retaining_display(Reply {
                    #[cfg(feature = "ui-harness")]
                    timing: None,
                    ticket: ticket(9, 1),
                    picture,
                })
                .is_none()
        );
    }
    assert_eq!(state.error(), Some("current Slip decode failed"));
    assert_eq!(state.displayed_label(), old_label);
    state.requested = Some(request(3, 13));
    let mut next = picture(5);
    next.canvas = Some((121, 71));
    assert!(
        state
            .receive_retaining_display(Reply {
                #[cfg(feature = "ui-harness")]
                timing: None,
                ticket: ticket(9, 3),
                picture: Ok(next),
            })
            .unwrap()
            .is_ok()
    );
    assert_eq!(state.error(), None);
    assert!(state.needs_render());
    assert_eq!(state.displayed_label(), old_label);
    assert_eq!(state.displayed.as_ref().unwrap().canvas, Some((101, 61)));
    assert!(ready(&state, 3, 13).is_none());
    state.presented();
    assert_eq!(ready(&state, 3, 13), Some(ticket(9, 3)));
    assert_eq!(state.canvas(), Some((121, 71)));
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(5)));
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing proposed edit frame 14")
    );
}

#[test]
fn slip_lifecycle_rejects_old_replies_before_replacement_dispatch() {
    let project = ProjectId::new("slip-project").unwrap();
    let revision = RevisionId::new("slip-candidate").unwrap();
    let content = deadpan_playback::ContentIdentity::Proposed {
        base_revision: RevisionId::new("base").unwrap(),
        draft: 8,
        change: 2,
    };
    let mut state = Presentation::default();
    let displayed = ticket(9, 1);
    state.request(displayed, &Work::Frame(SourceFrameId(4)));
    let mut previous = picture(4);
    previous.canvas = Some((101, 61));
    accept(&mut state, displayed, previous);
    state.decoded.as_mut().unwrap().geometry_revision = 3;
    state.presented();
    let old_label = state.displayed_label();
    let ordinary = ticket(9, 2);
    state.requested = Some(RequestedPicture {
        ticket: ordinary,
        location: Location::Project {
            session: 7,
            project: project.clone(),
            revision: RevisionId::new("base").unwrap(),
            view: ProjectView::Sequence {
                frame: ProjectFrame(19),
            },
            empty_sequence: false,
        },
    });
    state.loading = true;
    // Opening Slip discards the current layout before its first proposal can
    // dispatch. Revoke the previous ordinary request before the next receive.
    state.invalidate_pending();
    for picture in [Ok(picture(80)), Err("pre-Slip decode failure".into())] {
        assert!(
            state
                .receive_retaining_display(Reply {
                    #[cfg(feature = "ui-harness")]
                    timing: None,
                    ticket: ordinary,
                    picture,
                })
                .is_none()
        );
        assert!(state.requested.is_none());
        assert!(state.picture().is_none());
        assert!(!state.loading());
        assert!(!state.needs_render());
        assert!(state.error().is_none());
        assert_eq!(state.displayed_label(), old_label);
        assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
        assert_eq!(state.canvas(), Some((101, 61)));
        assert_eq!(state.displayed.as_ref().unwrap().geometry_revision, 3);
        assert!(
            state
                .stable_proposed_ticket(7, &project, &revision, &content, ProjectFrame(13))
                .is_none()
        );
    }

    let pending = ticket(9, 3);
    state.requested = Some(RequestedPicture {
        ticket: pending,
        location: Location::Proposed {
            session: 7,
            project: project.clone(),
            revision: revision.clone(),
            content: content.clone(),
            view: ProjectView::Sequence {
                frame: ProjectFrame(13),
            },
        },
    });
    state.loading = true;

    // Cancel, saved commit, and a stale workspace all revoke the proposal
    // before the final layout pass dispatches its replacement. A reply already
    // ready at this boundary must not become the next decoded/displayed image.
    state.invalidate_pending();
    for keep_slip_open in [false, true] {
        for result in [Ok(picture(90)), Err("retired Slip failure".into())] {
            let reply = Reply {
                #[cfg(feature = "ui-harness")]
                timing: None,
                ticket: pending,
                picture: result,
            };
            let accepted = if keep_slip_open {
                state.receive_retaining_display(reply)
            } else {
                state.receive(reply)
            };
            assert!(accepted.is_none());
            assert!(state.requested.is_none());
            assert!(state.picture().is_none());
            assert!(!state.loading());
            assert!(!state.needs_render());
            assert!(state.error().is_none());
            assert_eq!(state.displayed_label(), old_label);
            assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
            assert_eq!(state.canvas(), Some((101, 61)));
            assert_eq!(state.displayed.as_ref().unwrap().geometry_revision, 3);
            assert!(
                state
                    .stable_proposed_ticket(7, &project, &revision, &content, ProjectFrame(13),)
                    .is_none()
            );
        }
    }

    // An amount edit (including invalid text), Before toggle, or inspection
    // move can retire a reply that has decoded but has not reached the GPU.
    // The same immediate invalidation must prevent the layout retry from
    // promoting that retired candidate while the next request is deferred.
    let decoded = ticket(9, 4);
    state.requested = Some(RequestedPicture {
        ticket: decoded,
        location: Location::Proposed {
            session: 7,
            project: project.clone(),
            revision: revision.clone(),
            content: content.clone(),
            view: ProjectView::Sequence {
                frame: ProjectFrame(13),
            },
        },
    });
    accept(&mut state, decoded, picture(91));
    assert!(state.needs_render());
    assert_eq!(state.picture().unwrap().id, SourceFrameId(91));
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
    state.invalidate_pending();
    for picture in [Ok(picture(92)), Err("retired intent failure".into())] {
        assert!(
            state
                .receive_retaining_display(Reply {
                    #[cfg(feature = "ui-harness")]
                    timing: None,
                    ticket: decoded,
                    picture,
                })
                .is_none()
        );
        assert!(state.picture().is_none());
        assert!(!state.needs_render());
        assert!(state.error().is_none());
        assert_eq!(state.displayed_label(), old_label);
        assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(4)));
        assert_eq!(state.canvas(), Some((101, 61)));
        assert_eq!(state.displayed.as_ref().unwrap().geometry_revision, 3);
        assert!(
            state
                .stable_proposed_ticket(7, &project, &revision, &content, ProjectFrame(13))
                .is_none()
        );
    }

    let current = ticket(9, 5);
    state.requested = Some(RequestedPicture {
        ticket: current,
        location: Location::Project {
            session: 7,
            project,
            revision: RevisionId::new("new-head").unwrap(),
            view: ProjectView::Sequence {
                frame: ProjectFrame(19),
            },
            empty_sequence: false,
        },
    });
    accept(&mut state, current, picture(5));
    assert_eq!(state.displayed_label(), old_label);
    state.presented();
    assert_eq!(state.displayed_source_frame(), Some(SourceFrameId(5)));
    assert_eq!(
        state.displayed_label().as_deref(),
        Some("Showing sequence frame 20")
    );
    assert_eq!(
        state.stable_sequence_ticket(7, &RevisionId::new("new-head").unwrap(), ProjectFrame(19),),
        Some(current)
    );
}

#[test]
fn camera_sets_several_operations_atomically_or_not_at_all() {
    let mut state = Presentation {
        requested: Some(project_request(20, 1, "r1", false)),
        ..Default::default()
    };
    let (mut value, inner) = camera_picture();
    let outer = deadpan_core::InstancePath {
        node: deadpan_core::NodeId::new("group").unwrap(),
        repeats: Vec::new(),
    };
    let mut layer = value.framing[0].clone();
    layer.instance = outer.clone();
    value.framing.push(layer);
    accept(&mut state, ticket(1, 1), value);
    state.presented();
    let pose = deadpan_core::FramingPose {
        scale: deadpan_core::ExactRatio::integer(2),
        ..Default::default()
    };
    let missing = deadpan_core::InstancePath {
        node: deadpan_core::NodeId::new("absent").unwrap(),
        repeats: Vec::new(),
    };
    // One absent scope refuses the whole batch without changing the picture.
    assert!(
        state
            .set_framing_poses(
                ticket(1, 1),
                &[(inner.clone(), Some(pose)), (missing, Some(pose))]
            )
            .is_err()
    );
    assert!(!state.needs_render());
    assert_eq!(state.picture().unwrap().framing[0].pose, None);
    state
        .set_framing_poses(
            ticket(1, 1),
            &[(inner.clone(), Some(pose)), (outer.clone(), Some(pose))],
        )
        .unwrap();
    assert!(state.needs_render());
    let framing = &state.picture().unwrap().framing;
    assert_eq!((framing[0].pose, framing[1].pose), (Some(pose), Some(pose)));
}
