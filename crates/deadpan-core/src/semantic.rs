//! Reusable editing intent, resolved against the host's current selection.

mod planner;
mod program;

pub use planner::*;
pub use program::*;

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::{
    DocumentError, EditError, EditErrorCode, FrameRange, NodeId, NodeKind, ProjectDocument,
    ProjectFrame,
};

/// Cut linked picture and sound from the current Edit cursor by a requested
/// number of project frames. The count survives clamping at a Sequence's end;
/// resolved timestamps, scope, project and revision are never recorded here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameCut {
    count: NonZeroU32,
}

impl FrameCut {
    pub fn new(count: u32) -> Result<Self, EditError> {
        let count = NonZeroU32::new(count).ok_or_else(|| {
            EditError::new(
                EditErrorCode::InvalidDuration,
                "a frame cut requires a positive requested count",
            )
        })?;
        Ok(Self { count })
    }

    pub const fn count(&self) -> u32 {
        self.count.get()
    }

    /// Resolve anew inside the named ordinary Sequence on the absolute Edit
    /// clock. The host supplies the current scope and cursor, checks their
    /// project/revision identity, and binds any resulting command to this exact
    /// document revision. This query grants no mutation or media authority.
    pub fn resolve(
        &self,
        document: &ProjectDocument,
        parent: &NodeId,
        cursor: ProjectFrame,
    ) -> Result<FrameRange, EditError> {
        let start = document.source_splice_boundary(parent, 0)?;
        let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
            unreachable!("source_splice_boundary admitted an ordinary Sequence")
        };
        let end = document.source_splice_boundary(parent, children.len())?;
        if cursor < start || cursor >= end {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "a frame cut requires the Edit cursor inside its current Sequence",
            ));
        }
        let requested_end = cursor
            .0
            .checked_add(i64::from(self.count()))
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::TimingOverflow,
                    "the frame cut exceeds the project frame range",
                )
            })?;
        let range = FrameRange::new(cursor, ProjectFrame(requested_end.min(end.0)))
            .map_err(DocumentError::from)?;
        // Preserve the ordinary deletion contract, including unsupported
        // partial composite endpoints and the temporary Split node budget.
        document
            .range_deletion(parent, range)
            .map(|edit| edit.range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BeatNode, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, IterationOrder, PitchPolicy,
        PlayOverrides, ProjectId, RetimePurpose, RevisionId,
    };

    fn id(name: &str) -> NodeId {
        NodeId::new(name).unwrap()
    }

    fn revision(name: &str) -> RevisionId {
        RevisionId::new(name).unwrap()
    }

    fn range(start: i64, end: i64) -> FrameRange {
        FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
    }

    fn hold(frames: i64) -> BeatNode {
        BeatNode::hold(
            "Held",
            HoldRecipe {
                duration: FrameDuration::new(frames).unwrap(),
                picture_context: None,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        )
    }

    fn tree(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
        let mut document = ProjectDocument::new(
            ProjectId::new("semantic").unwrap(),
            revision("initial"),
            crate::basis::default_basis(),
            id("root"),
        )
        .unwrap();
        document.nodes.insert(
            id("root"),
            BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
        );
        document
            .nodes
            .extend(nodes.into_iter().map(|(name, node)| (id(name), node)));
        document.validate().unwrap();
        document
    }

    #[test]
    fn moving_the_cursor_resolves_again_without_changing_the_requested_count() {
        let document = tree(&["held"], vec![("held", hold(20))]);
        let intent = FrameCut::new(7).unwrap();
        assert_eq!(
            intent
                .resolve(&document, &id("root"), ProjectFrame(18))
                .unwrap(),
            range(18, 20)
        );
        assert_eq!(intent.count(), 7);
        assert_eq!(
            intent
                .resolve(&document, &id("root"), ProjectFrame(3))
                .unwrap(),
            range(3, 10)
        );
        assert_eq!(serde_json::to_string(&intent).unwrap(), r#"{"count":7}"#);
    }

    #[test]
    fn nested_scope_uses_absolute_bounds_and_stops_before_its_siblings() {
        let document = tree(
            &["prefix", "outer", "tail"],
            vec![
                ("prefix", hold(3)),
                (
                    "outer",
                    BeatNode::sequence("Outer", vec![id("lead"), id("inner")]),
                ),
                ("lead", hold(4)),
                ("inner", BeatNode::sequence("Inner", vec![id("held")])),
                ("held", hold(5)),
                ("tail", hold(10)),
            ],
        );
        let intent = FrameCut::new(9).unwrap();
        assert_eq!(
            intent
                .resolve(&document, &id("inner"), ProjectFrame(8))
                .unwrap(),
            range(8, 12)
        );
        for cursor in [-1, 0, 6, 12, 13, 22] {
            assert_eq!(
                intent
                    .resolve(&document, &id("inner"), ProjectFrame(cursor))
                    .unwrap_err()
                    .code,
                EditErrorCode::SelectionUnavailable
            );
        }
        assert_eq!(
            intent
                .resolve(&document, &id("inner"), ProjectFrame(7))
                .unwrap(),
            range(7, 12)
        );
    }

    #[test]
    fn empty_missing_and_nonsequence_scopes_refuse_without_fallback() {
        let intent = FrameCut::new(1).unwrap();
        let empty = tree(&[], vec![]);
        assert!(
            intent
                .resolve(&empty, &id("root"), ProjectFrame(0))
                .is_err()
        );
        let document = tree(&["held"], vec![("held", hold(5))]);
        for parent in ["missing", "held"] {
            assert!(
                intent
                    .resolve(&document, &id(parent), ProjectFrame(0))
                    .is_err()
            );
        }
    }

    #[test]
    fn partial_composite_endpoint_refuses_but_a_complete_composite_can_be_cut() {
        let document = tree(
            &["group"],
            vec![
                ("group", BeatNode::sequence("Group", vec![id("held")])),
                ("held", hold(5)),
            ],
        );
        for (count, cursor) in [(1, 0), (4, 1)] {
            assert!(
                FrameCut::new(count)
                    .unwrap()
                    .resolve(&document, &id("root"), ProjectFrame(cursor))
                    .is_err()
            );
        }
        assert_eq!(
            FrameCut::new(5)
                .unwrap()
                .resolve(&document, &id("root"), ProjectFrame(0))
                .unwrap(),
            range(0, 5)
        );
    }

    #[test]
    fn repeat_retime_and_override_ancestry_require_explicit_occurrence_resolution() {
        let mut repeat = BeatNode::sequence("Repeat", vec![]);
        repeat.kind = NodeKind::Repeat {
            child: id("group"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
        };
        let mut retime = BeatNode::sequence("Retime", vec![]);
        retime.kind = NodeKind::Retime {
            child: id("group"),
            duration: FrameDuration::new(10).unwrap(),
            mapping: range(0, 5),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        };
        for ancestor in [repeat, retime] {
            let mut document = tree(
                &["ancestor"],
                vec![
                    ("ancestor", ancestor),
                    ("group", BeatNode::sequence("Group", vec![id("held")])),
                    ("held", hold(5)),
                ],
            );
            let intent = FrameCut::new(1).unwrap();
            assert!(
                intent
                    .resolve(&document, &id("group"), ProjectFrame(0))
                    .is_err()
            );
            if let NodeKind::Repeat { iterations, .. } = &document.nodes[&id("ancestor")].kind {
                let mut overrides = PlayOverrides::default();
                overrides.insert(iterations.at(0).unwrap(), id("override"));
                document.overrides.insert(id("ancestor"), overrides);
                document.nodes.insert(
                    id("override"),
                    BeatNode::sequence("Override", vec![id("override-held")]),
                );
                document.nodes.insert(id("override-held"), hold(3));
                document.validate().unwrap();
                assert!(
                    intent
                        .resolve(&document, &id("override"), ProjectFrame(0))
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn resolution_uses_the_supplied_snapshot_and_does_not_grant_revision_authority() {
        let before = tree(&["held"], vec![("held", hold(10))]);
        let mut after = before.clone();
        after.revision_id = revision("later");
        after.nodes.insert(id("held"), hold(6));
        after.validate().unwrap();
        let intent = FrameCut::new(5).unwrap();
        assert_eq!(
            intent
                .resolve(&before, &id("root"), ProjectFrame(4))
                .unwrap(),
            range(4, 9)
        );
        assert_eq!(
            intent
                .resolve(&after, &id("root"), ProjectFrame(4))
                .unwrap(),
            range(4, 6)
        );
        assert_eq!(intent.count(), 5);
    }

    #[test]
    fn arithmetic_overflow_refuses_instead_of_turning_into_a_clamped_selection() {
        let document = tree(&["held"], vec![("held", hold(i64::MAX))]);
        assert_eq!(
            FrameCut::new(2)
                .unwrap()
                .resolve(&document, &id("root"), ProjectFrame(i64::MAX - 1))
                .unwrap_err()
                .code,
            EditErrorCode::TimingOverflow
        );
    }

    #[test]
    fn wire_format_retains_only_a_strict_positive_u32_count() {
        assert_eq!(
            FrameCut::new(0).unwrap_err().code,
            EditErrorCode::InvalidDuration
        );
        for count in [1, 12, u32::MAX] {
            let intent = FrameCut::new(count).unwrap();
            assert_eq!(
                serde_json::from_str::<FrameCut>(&serde_json::to_string(&intent).unwrap()).unwrap(),
                intent
            );
        }
        for invalid in [
            r#"{}"#,
            r#"{"count":0}"#,
            r#"{"count":-1}"#,
            r#"{"count":4294967296}"#,
            r#"{"count":1.0}"#,
            r#"{"count":"1"}"#,
            r#"{"count":null}"#,
            r#"{"count":1,"cursor":0}"#,
            r#"{"count":1,"count":2}"#,
        ] {
            assert!(
                serde_json::from_str::<FrameCut>(invalid).is_err(),
                "{invalid}"
            );
        }
    }
}
