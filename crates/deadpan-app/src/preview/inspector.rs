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
                        HoldVideo::Accepted { .. } => "Accepted media",
                        HoldVideo::Generated { .. } => "Accepted AI",
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
                if recipe.picture_context.is_some() {
                    fields.push(("Captured view", "Framing preserved".into()));
                }
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
                    "Enter opens this group's child beats. Backspace returns without changing the edit.",
                )
            }
            NodeKind::Retime {
                purpose: deadpan_core::RetimePurpose::Partition,
                mapping,
                ..
            } => {
                fields.push((
                    "From full beat",
                    format!("{}–{} f", mapping.start().0, mapping.end().0),
                ));
                (
                    "Fragment",
                    "F",
                    None,
                    "This fragment keeps the full beat's original picture and sound timing. Split again to make a smaller cut.",
                )
            }
            NodeKind::Retime {
                pitch,
                mapping,
                duration,
                ..
            } => {
                fields.push((
                    "Speed",
                    format!("{}/{}×", mapping.duration().frames(), duration.frames()),
                ));
                fields.push((
                    "Pitch",
                    match pitch {
                        PitchPolicy::Preserve => "Preserve",
                        PitchPolicy::FollowSpeed => "Tape (follows speed)",
                    }
                    .into(),
                ));
                (
                    "Retime",
                    "R",
                    Some((
                        "Change speed…",
                        format!(
                            "retime {}/{} pitch={}",
                            mapping.duration().frames(),
                            duration.frames(),
                            crate::navigation::retime::pitch_name(*pitch)
                        ),
                    )),
                    "Speed is relative to this Retime's input span. Changing it preserves its input selection. :wrap-retime adds another speed stage.",
                )
            }
        };
        if let Some(framing) = &node.framing {
            let description = match &framing.value {
                deadpan_core::FramingValue::Static { pose } => {
                    let scale = pose.scale.numerator() as f64 / pose.scale.denominator() as f64;
                    format!("Static · {scale:.3}×")
                }
                deadpan_core::FramingValue::Envelope { .. } => "Whole-beat motion".into(),
            };
            fields.push(("Framing", description));
        }
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
            audio_treatments: Default::default(),
            label: "Pause".into(),
            framing: None,
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Hold {
                recipe: HoldRecipe {
                    picture_context: None,
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
            audio_treatments: Default::default(),
            label: "Again".into(),
            framing: None,
            audio_editorial_edges: Default::default(),
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

    #[test]
    fn retime_entry_uses_the_exact_retained_input_speed_and_current_pitch() {
        let mut node = BeatNode {
            audio_treatments: Default::default(),
            label: "Delivery".into(),
            framing: None,
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: NodeId::new("child").unwrap(),
                duration: FrameDuration::new(3).unwrap(),
                mapping: deadpan_core::FrameRange::new(
                    deadpan_core::ProjectFrame(5),
                    deadpan_core::ProjectFrame(15),
                )
                .unwrap(),
                pitch: PitchPolicy::FollowSpeed,
                purpose: deadpan_core::RetimePurpose::Edit,
            },
        };
        let inspector = Inspector::describe(&node, 22, 3);
        let command = inspector.parameter.unwrap().1;
        assert_eq!(command, "retime 10/3 pitch=tape");
        assert!(inspector.fields.contains(&("Speed", "10/3×".into())));
        assert_eq!(
            crate::navigation::command::parse(&command),
            Ok(crate::navigation::command::Entry::Action(
                crate::navigation::Action::Edit(crate::navigation::BeatEdit::Retime(
                    crate::navigation::retime::RetimeInput {
                        speed: deadpan_core::ExactRatio::new(10, 3).unwrap(),
                        pitch: PitchPolicy::FollowSpeed,
                        wrap: false,
                    }
                ))
            ))
        );
        if let NodeKind::Retime { purpose, .. } = &mut node.kind {
            *purpose = deadpan_core::RetimePurpose::Partition;
        }
        let fragment = Inspector::describe(&node, 22, 3);
        assert_eq!(fragment.kind, "Fragment");
        assert!(
            fragment.parameter.is_none(),
            "partition clocks are never edited in place"
        );
    }
}
