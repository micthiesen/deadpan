//! Pure inspection identity and exact audio context for the active Trim draft.

use std::{collections::BTreeMap, ops::Range};

use deadpan_core::{
    AudioSample, FrameDuration, FrameRange, FrameRate, MAX_DOCUMENT_DEPTH, NodeId, NodeKind,
    ProjectDocument, ProjectFrame, RevisionId, SourceTrimControl, SourceTrimEdge,
    SourceTrimRightDisposition,
};
use deadpan_playback::{ContentIdentity, Window};

use crate::project::trim::{Prepared, ProposalId};
use crate::worker::{EditJunctionIdentity, EditJunctionInput, JunctionRole, JunctionSide};

use super::super::playback::AuditionContext;

pub(super) struct Inspection {
    pub identity: EditJunctionIdentity,
    pub input: EditJunctionInput,
    pub duration: FrameDuration,
    /// Exact B(c), independent of which adjacent picture is present.
    pub boundary_sample: AudioSample,
    pub samples: Range<AudioSample>,
    /// The actual incoming structural beat in the displayed document.
    pub incoming_label: String,
    /// Separately describes captured B in the candidate, including when Before
    /// is displayed. It never stands in for the actual incoming owner label.
    pub right_label: String,
}

impl Inspection {
    pub(super) fn playback_window(&self, looping: bool) -> Result<Window, String> {
        if self.samples.start >= self.samples.end {
            return Err("Trim context contains no audio samples.".into());
        }
        Window::new(self.samples.start, self.samples.end, looping)
            .map_err(|error| error.to_string())
    }
}

/// Build only when input, comparison, inspection or context changes. The helper
/// validates the retained candidate and walks authored durations for labels; it
/// performs no decoding, media I/O, playback changes or source qualification.
pub(super) fn build(
    id: &ProposalId,
    prepared: &Prepared,
    side: JunctionSide,
    control: SourceTrimControl,
    slip_edge: SourceTrimEdge,
    inspection: u64,
    context: AuditionContext,
) -> Result<Inspection, String> {
    validate_prepared(id, prepared)?;
    let role = role(control, slip_edge);
    let geometry = &prepared.resolution.geometry;
    let (document, duration) = match (side, &prepared.snapshot) {
        (JunctionSide::Proposed, Some(snapshot)) => {
            (&*snapshot.document, geometry.project_duration_after)
        }
        _ => (&*prepared.base.document, geometry.project_duration_before),
    };
    let boundary = boundary(
        side,
        role,
        prepared.target.range,
        prepared.result.target_output,
    );
    let content = proposal_content(
        id,
        prepared.accepted.is_zero(),
        prepared.snapshot.as_ref().map(|snapshot| &snapshot.content),
        prepared
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.document.revision_id()),
    )?;
    let identity = identity(
        id,
        content.0,
        content.1,
        inspection,
        Coordinates {
            side,
            role,
            boundary,
            duration,
        },
    )?;
    let rate = document.presentation_basis().frame_rate;
    let (boundary_sample, samples) = context_window(rate, duration, boundary, context)?;
    // One duration inventory preserves nested ordinary scope and skips only empty
    // structure. Opaque Retime/Repeat nodes remain named as structural beats.
    let durations = document.durations().map_err(|error| error.to_string())?;
    if durations.get(document.root()) != Some(&duration) {
        return Err("Trim inspection duration differs from its admitted document.".into());
    }
    let incoming_label = incoming_label(document, &durations, identity.incoming, prepared, side)?;
    let right_label = right_label(prepared);
    Ok(Inspection {
        identity,
        input: EditJunctionInput {
            base: prepared.base.clone(),
            snapshot: prepared.snapshot.clone(),
        },
        duration,
        boundary_sample,
        samples,
        incoming_label,
        right_label,
    })
}

fn validate_prepared(id: &ProposalId, prepared: &Prepared) -> Result<(), String> {
    if id.session == 0
        || id.draft == 0
        || id.change == 0
        || id.session != prepared.base.session
        || &id.project != prepared.base.document.project_id()
        || &id.base_revision != prepared.base.document.revision_id()
        || id.session != prepared.target.session
        || id.project != prepared.target.project
        || id.base_revision != prepared.target.base_revision
    {
        return Err("Trim inspection belongs to another captured proposal.".into());
    }
    prepared.target.validate(&prepared.base)?;
    let geometry = &prepared.resolution.geometry;
    if geometry.intent != prepared.accepted
        || geometry.parent != prepared.target.parent
        || geometry.target.target != prepared.target.node
        || geometry.target.output_before != prepared.target.range
        || geometry.target.output_after != prepared.result.target_output
        || geometry.project_duration_before != prepared.base.plan.duration()
    {
        return Err("Trim inspection differs from its accepted target geometry.".into());
    }
    match (&prepared.snapshot, prepared.accepted.is_zero()) {
        (None, true) => {
            if geometry.project_duration_after != geometry.project_duration_before
                || prepared.result.target_output != prepared.target.range
                || prepared.result.target != prepared.target.node
            {
                return Err("Zero Trim inspection must preserve the exact entry.".into());
            }
        }
        (Some(snapshot), false) => {
            if snapshot.session != id.session
                || snapshot.document.project_id() != &id.project
                || snapshot.document.revision_id() == &id.base_revision
                || snapshot.document.presentation_basis()
                    != prepared.base.document.presentation_basis()
                || !snapshot
                    .document
                    .nodes()
                    .contains_key(&prepared.result.target)
            {
                return Err("Trim inspection candidate differs from its admitted base.".into());
            }
            if snapshot
                .document
                .duration()
                .map_err(|error| error.to_string())?
                != geometry.project_duration_after
            {
                return Err("Trim proposed duration differs from its accepted geometry.".into());
            }
            snapshot
                .validate_original_proposal()
                .map_err(|error| error.to_string())?;
            snapshot
                .validate_proposed_base(prepared.base.session, &prepared.base.document)
                .map_err(|error| error.to_string())?;
        }
        _ => {
            return Err(
                "Trim inspection requires an explicit zero or exact proposed snapshot.".into(),
            );
        }
    }
    Ok(())
}

fn proposal_content(
    id: &ProposalId,
    zero: bool,
    content: Option<&ContentIdentity>,
    revision: Option<&RevisionId>,
) -> Result<(ContentIdentity, Option<RevisionId>), String> {
    match (zero, content, revision) {
        (true, None, None) => Ok((ContentIdentity::Committed, None)),
        (false, Some(content), Some(revision))
            if *content
                == (ContentIdentity::Proposed {
                    base_revision: id.base_revision.clone(),
                    draft: id.draft,
                    change: id.change,
                })
                && revision != &id.base_revision =>
        {
            Ok((content.clone(), Some(revision.clone())))
        }
        _ => Err("Trim inspection snapshot does not match its complete proposal identity.".into()),
    }
}

pub(super) const fn role(control: SourceTrimControl, slip_edge: SourceTrimEdge) -> JunctionRole {
    match control {
        SourceTrimControl::In => JunctionRole::In,
        SourceTrimControl::Out => JunctionRole::Out,
        SourceTrimControl::Roll => JunctionRole::Roll,
        SourceTrimControl::Slip => match slip_edge {
            SourceTrimEdge::In => JunctionRole::SlipIn,
            SourceTrimEdge::Out => JunctionRole::SlipOut,
        },
    }
}

fn boundary(
    side: JunctionSide,
    role: JunctionRole,
    before: FrameRange,
    proposed: FrameRange,
) -> ProjectFrame {
    let range = match side {
        JunctionSide::Before => before,
        JunctionSide::Proposed => proposed,
    };
    match role {
        JunctionRole::In | JunctionRole::SlipIn => range.start(),
        JunctionRole::Out | JunctionRole::SlipOut | JunctionRole::Roll => range.end(),
    }
}

#[derive(Clone, Copy)]
struct Coordinates {
    side: JunctionSide,
    role: JunctionRole,
    boundary: ProjectFrame,
    duration: FrameDuration,
}

fn identity(
    id: &ProposalId,
    content: ContentIdentity,
    proposal_revision: Option<RevisionId>,
    inspection: u64,
    coordinates: Coordinates,
) -> Result<EditJunctionIdentity, String> {
    let Coordinates {
        side,
        role,
        boundary,
        duration,
    } = coordinates;
    if id.session == 0 || id.draft == 0 || id.change == 0 || inspection == 0 {
        return Err("Trim inspection identities must be nonzero.".into());
    }
    if boundary.0 < 0 || boundary.0 > duration.frames() {
        return Err("Trim inspection boundary is outside its displayed edit.".into());
    }
    Ok(EditJunctionIdentity {
        session: id.session,
        project: id.project.clone(),
        base_revision: id.base_revision.clone(),
        draft: id.draft,
        change: id.change,
        content,
        proposal_revision,
        inspection,
        side,
        role,
        boundary,
        outgoing: (boundary.0 > 0).then(|| ProjectFrame(boundary.0 - 1)),
        incoming: (boundary.0 < duration.frames()).then_some(boundary),
    })
}

fn context_window(
    rate: FrameRate,
    duration: FrameDuration,
    boundary: ProjectFrame,
    context: AuditionContext,
) -> Result<(AudioSample, Range<AudioSample>), String> {
    if boundary.0 < 0
        || boundary.0 > duration.frames()
        || context.lead.0 < 0
        || context.follow.0 < 0
    {
        return Err("Trim audition needs a valid boundary and nonnegative context.".into());
    }
    // Convert absolute coordinates independently. Never multiply a rounded frame
    // width or shift the Before window to synthesize Proposed sample boundaries.
    let at = rate
        .audio_boundary(boundary)
        .map_err(|error| error.to_string())?;
    let end = rate
        .audio_boundary(ProjectFrame(duration.frames()))
        .map_err(|error| error.to_string())?;
    let start = AudioSample(at.0 - context.lead.0.min(at.0));
    let finish = AudioSample(at.0 + context.follow.0.min(end.0 - at.0));
    // An empty audition context must not block picture inspection or Apply.
    // playback_window rejects it; waveform measurement reports it separately.
    Ok((at, start..finish))
}

fn incoming_label(
    document: &ProjectDocument,
    durations: &BTreeMap<NodeId, FrameDuration>,
    incoming: Option<ProjectFrame>,
    prepared: &Prepared,
    side: JunctionSide,
) -> Result<String, String> {
    let Some(frame) = incoming else {
        return Ok("No incoming frame".into());
    };
    let owner = owner_at_frame(document, durations, frame)?;
    let beat = document
        .nodes()
        .get(&owner)
        .ok_or("Trim incoming beat is missing.")?;
    let filler = side == JunctionSide::Proposed && prepared.result.fillers.contains(&owner);
    Ok(if filler {
        format!("Incoming: silent filler · {}", beat.label)
    } else {
        format!("Incoming: {}", beat.label)
    })
}

/// Resolve the structural beat at a real Edit frame through ordinary Sequences.
/// No endpoint clamping and no Repeat expansion or guessed provider identity.
fn owner_at_frame(
    document: &ProjectDocument,
    durations: &BTreeMap<NodeId, FrameDuration>,
    frame: ProjectFrame,
) -> Result<NodeId, String> {
    let total = durations
        .get(document.root())
        .ok_or("Trim root duration is missing.")?
        .frames();
    if frame.0 < 0 || frame.0 >= total {
        return Err("Trim incoming frame is outside its displayed edit.".into());
    }
    let mut current = document.root();
    let mut local = frame.0;
    for _ in 0..=MAX_DOCUMENT_DEPTH {
        let beat = document
            .nodes()
            .get(current)
            .ok_or("Trim incoming beat is missing.")?;
        let NodeKind::Sequence { children } = &beat.kind else {
            return Ok(current.clone());
        };
        let mut next = None;
        for child in children {
            let frames = durations
                .get(child)
                .ok_or("Trim child duration is missing.")?
                .frames();
            if local < frames {
                next = Some(child);
                break;
            }
            local = local
                .checked_sub(frames)
                .ok_or("Trim incoming clock overflowed.")?;
        }
        current = next.ok_or("Trim incoming frame has no structural owner.")?;
    }
    Err("Trim incoming scope exceeds the document depth bound.".into())
}

fn right_label(prepared: &Prepared) -> String {
    let Some(right) = &prepared.target.right else {
        return "Captured right beat: none · Roll unavailable".into();
    };
    let label = prepared
        .base
        .document
        .nodes()
        .get(right)
        .map_or("Captured right beat", |beat| beat.label.as_str());
    if prepared.resolution.geometry.right.is_none() {
        return format!("Captured right beat: {label} · Roll unavailable");
    }
    match prepared.result.right_disposition {
        SourceTrimRightDisposition::Unchanged => {
            format!("Proposed right beat: {label} · source window unchanged")
        }
        SourceTrimRightDisposition::Removed => format!("Proposed right beat: {label} · removed"),
        SourceTrimRightDisposition::Retained => {
            let padding = prepared
                .resolution
                .right_after
                .as_ref()
                .is_some_and(|right| right.visible_selection.is_none());
            format!(
                "Proposed right beat: {label} · {}",
                if padding {
                    "endpoint padding retained"
                } else {
                    "retained"
                }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{
        BeatNode, Command, CommandRequest, HoldAudio, HoldRecipe, HoldVideo, ProjectId, Subtree,
    };

    fn id() -> ProposalId {
        ProposalId {
            session: 3,
            project: ProjectId::new("trim-inspection").unwrap(),
            base_revision: RevisionId::new("entry").unwrap(),
            draft: 8,
            change: 13,
        }
    }
    fn range(start: i64, end: i64) -> FrameRange {
        FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
    }
    fn duration(frames: i64) -> FrameDuration {
        FrameDuration::new(frames).unwrap()
    }
    fn context(lead: i64, follow: i64) -> AuditionContext {
        AuditionContext {
            lead: AudioSample(lead),
            follow: AudioSample(follow),
        }
    }
    fn coordinates(side: JunctionSide, boundary: i64, frames: i64) -> Coordinates {
        Coordinates {
            side,
            role: JunctionRole::Out,
            boundary: ProjectFrame(boundary),
            duration: duration(frames),
        }
    }

    #[test]
    fn controls_select_junction_roles_without_changing_the_accepted_range() {
        let before = range(10, 20);
        let proposed = range(15, 25);
        for (control, edge, expected) in [
            (SourceTrimControl::In, SourceTrimEdge::Out, JunctionRole::In),
            (
                SourceTrimControl::Out,
                SourceTrimEdge::In,
                JunctionRole::Out,
            ),
            (
                SourceTrimControl::Slip,
                SourceTrimEdge::In,
                JunctionRole::SlipIn,
            ),
            (
                SourceTrimControl::Slip,
                SourceTrimEdge::Out,
                JunctionRole::SlipOut,
            ),
            (
                SourceTrimControl::Roll,
                SourceTrimEdge::In,
                JunctionRole::Roll,
            ),
        ] {
            assert_eq!(role(control, edge), expected);
            let is_in = matches!(expected, JunctionRole::In | JunctionRole::SlipIn);
            assert_eq!(
                boundary(JunctionSide::Before, expected, before, proposed),
                ProjectFrame(if is_in { 10 } else { 20 })
            );
            assert_eq!(
                boundary(JunctionSide::Proposed, expected, before, proposed),
                ProjectFrame(if is_in { 15 } else { 25 })
            );
        }
    }

    #[test]
    fn junction_exteriors_use_their_own_duration_and_never_clamp_a_missing_slot() {
        for (at, frames, outgoing, incoming) in [
            (0, 5, None, Some(ProjectFrame(0))),
            (3, 5, Some(ProjectFrame(2)), Some(ProjectFrame(3))),
            (5, 5, Some(ProjectFrame(4)), None),
            (0, 0, None, None),
        ] {
            let actual = identity(
                &id(),
                ContentIdentity::Committed,
                None,
                1,
                coordinates(JunctionSide::Proposed, at, frames),
            )
            .unwrap();
            assert_eq!(actual.outgoing, outgoing);
            assert_eq!(actual.incoming, incoming);
            assert_eq!(actual.boundary, ProjectFrame(at));
        }
        for (at, frames) in [(-1, 5), (6, 5)] {
            assert!(
                identity(
                    &id(),
                    ContentIdentity::Committed,
                    None,
                    1,
                    coordinates(JunctionSide::Before, at, frames)
                )
                .is_err()
            );
        }
        assert!(
            identity(
                &id(),
                ContentIdentity::Committed,
                None,
                0,
                coordinates(JunctionSide::Before, 0, 5)
            )
            .is_err()
        );
        for case in 0..3 {
            let mut invalid = id();
            match case {
                0 => invalid.session = 0,
                1 => invalid.draft = 0,
                _ => invalid.change = 0,
            }
            assert!(
                identity(
                    &invalid,
                    ContentIdentity::Committed,
                    None,
                    1,
                    coordinates(JunctionSide::Before, 0, 5)
                )
                .is_err()
            );
        }
        let before = identity(
            &id(),
            ContentIdentity::Committed,
            None,
            1,
            coordinates(JunctionSide::Before, 5, 5),
        )
        .unwrap();
        let proposed = identity(
            &id(),
            ContentIdentity::Committed,
            None,
            2,
            coordinates(JunctionSide::Proposed, 5, 7),
        )
        .unwrap();
        assert!(before.incoming.is_none());
        assert_eq!(proposed.incoming, Some(ProjectFrame(5)));
    }

    #[test]
    fn before_and_proposed_keep_the_same_complete_candidate_identity_and_actual_revision() {
        let id = id();
        let proposed = ContentIdentity::Proposed {
            base_revision: id.base_revision.clone(),
            draft: id.draft,
            change: id.change,
        };
        let revision = RevisionId::new("actual-private-candidate").unwrap();
        let (content, revision) =
            proposal_content(&id, false, Some(&proposed), Some(&revision)).unwrap();
        let before = identity(
            &id,
            content.clone(),
            revision.clone(),
            41,
            coordinates(JunctionSide::Before, 5, 5),
        )
        .unwrap();
        let after = identity(
            &id,
            content,
            revision.clone(),
            42,
            coordinates(JunctionSide::Proposed, 7, 7),
        )
        .unwrap();
        assert_eq!(before.content, proposed);
        assert_eq!(after.content, proposed);
        assert_eq!(before.proposal_revision, revision);
        assert_eq!(after.proposal_revision, revision);
        assert_eq!(before.base_revision, id.base_revision);
        assert_eq!(before.draft, 8);
        assert_eq!(before.change, 13);
        assert_ne!(before, after);
        let mut next = id.clone();
        next.change += 1;
        assert!(proposal_content(&next, false, Some(&proposed), revision.as_ref()).is_err());
        next = id.clone();
        next.draft += 1;
        assert!(proposal_content(&next, false, Some(&proposed), revision.as_ref()).is_err());
        assert!(proposal_content(&id, false, Some(&proposed), Some(&id.base_revision)).is_err());
    }

    #[test]
    fn zero_is_explicit_and_cannot_hide_a_missing_or_unexpected_candidate() {
        let id = id();
        let (content, revision) = proposal_content(&id, true, None, None).unwrap();
        assert_eq!(content, ContentIdentity::Committed);
        assert!(revision.is_none());
        for side in [JunctionSide::Before, JunctionSide::Proposed] {
            let actual = identity(
                &id,
                content.clone(),
                revision.clone(),
                1,
                coordinates(side, 2, 4),
            )
            .unwrap();
            assert_eq!(actual.draft, id.draft);
            assert_eq!(actual.change, id.change);
            assert_eq!(actual.side, side);
            assert!(actual.proposal_revision.is_none());
        }
        let proposed = ContentIdentity::Proposed {
            base_revision: id.base_revision.clone(),
            draft: id.draft,
            change: id.change,
        };
        let revision = RevisionId::new("unexpected").unwrap();
        assert!(proposal_content(&id, false, None, None).is_err());
        assert!(proposal_content(&id, true, Some(&proposed), Some(&revision)).is_err());
        assert!(proposal_content(&id, true, Some(&ContentIdentity::Committed), None).is_err());
        assert!(
            proposal_content(
                &id,
                false,
                Some(&ContentIdentity::Committed),
                Some(&revision)
            )
            .is_err()
        );
    }

    #[test]
    fn context_uses_exact_absolute_ntsc_cut_samples_and_each_sides_own_terminal() {
        let rate = FrameRate::new(30_000, 1001).unwrap();
        let (at, samples) =
            context_window(rate, duration(8), ProjectFrame(3), context(7, 11)).unwrap();
        assert_eq!(at, AudioSample(4805));
        assert_eq!(samples, AudioSample(4798)..AudioSample(4816));
        let (first, _) = context_window(rate, duration(8), ProjectFrame(1), context(1, 1)).unwrap();
        let (second, _) =
            context_window(rate, duration(8), ProjectFrame(2), context(1, 1)).unwrap();
        assert_eq!(first, AudioSample(1602));
        assert_eq!(second, AudioSample(3203));
        assert_ne!(second.0, first.0 * 2);
        let (before_at, before) =
            context_window(rate, duration(8), ProjectFrame(8), context(7, i64::MAX)).unwrap();
        let (after_at, after) =
            context_window(rate, duration(7), ProjectFrame(7), context(7, i64::MAX)).unwrap();
        assert_eq!(before_at, AudioSample(12813));
        assert_eq!(before, AudioSample(12806)..AudioSample(12813));
        assert_eq!(after_at, AudioSample(11211));
        assert_eq!(after, AudioSample(11204)..AudioSample(11211));
    }

    #[test]
    fn context_clips_before_arithmetic_and_keeps_sampleless_inspection_independent() {
        let rate = FrameRate::new(30, 1).unwrap();
        let (_, all) = context_window(
            rate,
            duration(3),
            ProjectFrame(1),
            context(i64::MAX, i64::MAX),
        )
        .unwrap();
        assert_eq!(all, AudioSample(0)..AudioSample(4800));
        let (_, first) =
            context_window(rate, duration(3), ProjectFrame(0), context(99, 7)).unwrap();
        assert_eq!(first, AudioSample(0)..AudioSample(7));
        for (frames, at, lead, follow) in [(3, 1, 0, 0), (0, 0, 7, 9), (3, 0, 7, 0), (3, 3, 0, 7)] {
            let (_, samples) = context_window(
                rate,
                duration(frames),
                ProjectFrame(at),
                context(lead, follow),
            )
            .unwrap();
            assert!(samples.is_empty());
            // Empty inspection is valid. Trim's audition method rejects it;
            // the shared non-looping Window type also represents ended audio.
        }
        for (frames, at, lead, follow) in
            [(3, -1, 1, 1), (3, 4, 1, 1), (3, 1, -1, 1), (3, 1, 1, -1)]
        {
            assert!(
                context_window(
                    rate,
                    duration(frames),
                    ProjectFrame(at),
                    context(lead, follow)
                )
                .is_err()
            );
        }
        let high_rate = FrameRate::new(1_000_000, 1).unwrap();
        let (_, samples) =
            context_window(high_rate, duration(1), ProjectFrame(0), context(1, 1)).unwrap();
        assert!(samples.is_empty());
        assert!(
            context_window(
                FrameRate::new(1, 1).unwrap(),
                duration(i64::MAX),
                ProjectFrame(0),
                context(1, 1)
            )
            .is_err()
        );
    }

    fn node(name: &str) -> NodeId {
        NodeId::new(name).unwrap()
    }
    fn hold(name: &str, frames: i64) -> BeatNode {
        BeatNode::hold(
            name,
            HoldRecipe {
                duration: duration(frames),
                picture_context: None,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        )
    }

    #[test]
    fn incoming_owner_uses_half_open_nested_sequence_positions_and_skips_empty_structure() {
        let initial = ProjectDocument::new_automatic(
            ProjectId::new("trim-owner-label").unwrap(),
            RevisionId::new("initial").unwrap(),
            node("root"),
        )
        .unwrap();
        let group = node("group");
        let request = CommandRequest {
            project_id: initial.project_id().clone(),
            expected_revision: initial.revision_id().clone(),
            new_revision: RevisionId::new("inserted").unwrap(),
            command: Command::Insert {
                parent: node("root"),
                index: 0,
                subtree: Subtree {
                    root: group.clone(),
                    nodes: BTreeMap::from([
                        (
                            group,
                            BeatNode::sequence(
                                "group",
                                vec![node("before"), node("empty"), node("nested")],
                            ),
                        ),
                        (node("before"), hold("before", 3)),
                        (node("empty"), BeatNode::sequence("empty", vec![])),
                        (
                            node("nested"),
                            BeatNode::sequence("nested", vec![node("filler"), node("after")]),
                        ),
                        (node("filler"), hold("filler", 2)),
                        (node("after"), hold("after", 4)),
                    ]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        };
        let document = deadpan_core::apply(&initial, &request)
            .unwrap()
            .forward
            .apply(&initial)
            .unwrap();
        let durations = document.durations().unwrap();
        for (frame, expected) in [
            (0, "before"),
            (2, "before"),
            (3, "filler"),
            (4, "filler"),
            (5, "after"),
            (8, "after"),
        ] {
            assert_eq!(
                owner_at_frame(&document, &durations, ProjectFrame(frame)).unwrap(),
                node(expected)
            );
        }
        assert!(owner_at_frame(&document, &durations, ProjectFrame(-1)).is_err());
        assert!(owner_at_frame(&document, &durations, ProjectFrame(9)).is_err());
    }
}
