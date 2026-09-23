//! Selection facts and existing parameter entry, with no authored-state mutation.

use deadpan_core::{
    BeatNode, HoldAudio, HoldVideo, LinkRelation, NodeKind, PitchPolicy, SourceVideo,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Inspector {
    pub label: String,
    pub kind: &'static str,
    pub glyph: &'static str,
    pub duration: String,
    pub range: String,
    pub fields: Vec<(&'static str, String)>,
    pub parameter: Option<(&'static str, String)>,
    pub note: &'static str,
}

impl Inspector {
    pub fn describe(node: &BeatNode, start: u64, frames: u64) -> Self {
        let mut fields = Vec::new();
        let (kind, glyph, parameter, note) = match &node.kind {
            NodeKind::Hold { recipe } => {
                fields.push((
                    "Picture",
                    match &recipe.video {
                        HoldVideo::Background => "Background",
                        HoldVideo::Freeze { .. } => "Freeze",
                        HoldVideo::Accepted { .. } | HoldVideo::Generated { .. } => {
                            "Accepted media"
                        }
                    }
                    .into(),
                ));
                fields.push((
                    "Sound",
                    match &recipe.audio {
                        HoldAudio::Silence => "Silence",
                        HoldAudio::RoomTone { .. } => "Room tone",
                        HoldAudio::Tail { .. } => "Permitted tail",
                    }
                    .into(),
                ));
                let note = if matches!(recipe.video, HoldVideo::Generated { .. }) {
                    "Extending beyond the available generated footage restores the captured fallback."
                } else {
                    "Change this Hold's exact duration in project frames. This does not insert another Hold."
                };
                (
                    "Hold",
                    "H",
                    Some((
                        "Change duration…",
                        format!("hold-duration {}f", recipe.duration.frames()),
                    )),
                    note,
                )
            }
            NodeKind::Repeat {
                iterations, gap, ..
            } => {
                fields.push(("Total plays", iterations.len().to_string()));
                fields.push((
                    "Between plays",
                    gap.as_ref().map_or_else(
                        || "No gap".into(),
                        |gap| format!("{} f", gap.duration.frames()),
                    ),
                ));
                (
                    "Repeat",
                    "↻",
                    Some(("Set total plays…", format!("repeat {}", iterations.len()))),
                    "Set total plays to change this Repeat. Wrap repeat adds a new enclosing Repeat.",
                )
            }
            NodeKind::Source { source } => {
                fields.push((
                    "Picture",
                    match source.video {
                        SourceVideo::Stream { .. } => "Original video",
                        SourceVideo::Still { .. } => "Still image",
                        SourceVideo::Blank => "Background",
                    }
                    .into(),
                ));
                fields.push((
                    "Sound",
                    if source.audio.is_some() {
                        "Original audio"
                    } else {
                        "None"
                    }
                    .into(),
                ));
                fields.push((
                    "Link",
                    match source.link {
                        LinkRelation::Linked => "Linked",
                        LinkRelation::Independent => "Independent",
                    }
                    .into(),
                ));
                (
                    "Source",
                    "S",
                    None,
                    "This beat references retained source media. Structural edits preserve the original.",
                )
            }
            NodeKind::Sequence { children } => {
                fields.push(("Child beats", children.len().to_string()));
                (
                    "Sequence",
                    "≡",
                    None,
                    "This group is selected as one root beat. Nested navigation is not available yet.",
                )
            }
            NodeKind::Retime { pitch, .. } => {
                fields.push((
                    "Pitch",
                    match pitch {
                        PitchPolicy::Preserve => "Preserve",
                        PitchPolicy::FollowSpeed => "Follow speed",
                    }
                    .into(),
                ));
                (
                    "Retime",
                    "R",
                    None,
                    "The complete Retime is selected. Its existing timing remains structural.",
                )
            }
        };
        Self {
            label: node.label.clone(),
            kind,
            glyph,
            duration: format!("{frames} f"),
            range: start
                .checked_add(frames)
                .map_or_else(|| "Unavailable".into(), |end| format!("{start}–{end}")),
            fields,
            parameter,
            note,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{FrameDuration, HoldRecipe, IterationOrder, NodeId, RevisionId};

    #[test]
    fn hold_facts_and_duration_entry_preserve_actual_policy_and_frame_units() {
        let node = BeatNode {
            label: "Pause".into(),
            audio_edges: Default::default(),
            kind: NodeKind::Hold {
                recipe: HoldRecipe {
                    duration: FrameDuration::new(11).unwrap(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            },
        };
        let inspector = Inspector::describe(&node, 132, 11);
        assert_eq!(inspector.duration, "11 f");
        assert_eq!(inspector.range, "132–143");
        assert_eq!(
            inspector.fields,
            vec![
                ("Picture", "Background".into()),
                ("Sound", "Silence".into())
            ]
        );
        assert_eq!(inspector.parameter.unwrap().1, "hold-duration 11f");
    }

    #[test]
    fn repeat_inspection_is_compact_and_edits_the_existing_repeat() {
        let node = BeatNode {
            label: "Again".into(),
            audio_edges: Default::default(),
            kind: NodeKind::Repeat {
                child: NodeId::new("child").unwrap(),
                iterations: IterationOrder::new(RevisionId::new("allocation").unwrap(), u32::MAX)
                    .unwrap(),
                gap: None,
            },
        };
        let inspector = Inspector::describe(&node, 143, 72);
        assert_eq!(
            inspector.fields,
            vec![
                ("Total plays", u32::MAX.to_string()),
                ("Between plays", "No gap".into())
            ]
        );
        assert_eq!(inspector.parameter.unwrap().1, "repeat 4294967295");
        assert_eq!(inspector.range, "143–215");
    }

    #[test]
    fn empty_group_has_truthful_zero_duration_without_invented_parameter_controls() {
        let inspector = Inspector::describe(&BeatNode::sequence("Empty", Vec::new()), 4, 0);
        assert_eq!(inspector.duration, "0 f");
        assert_eq!(inspector.range, "4–4");
        assert_eq!(inspector.fields, vec![("Child beats", "0".into())]);
        assert!(inspector.parameter.is_none());
    }
}
