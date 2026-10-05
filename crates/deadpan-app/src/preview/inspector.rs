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
                        HoldVideo::Reverse { .. } => "Reversed",
                        HoldVideo::Play { .. } => "Original",
                    }
                    .into(),
                ));
                fields.push((
                    "Sound",
                    match &recipe.audio {
                        HoldAudio::Silence => "Silence".into(),
                        HoldAudio::RoomTone { .. } => "Room tone".into(),
                        HoldAudio::Reverse { .. } => "Reversed".into(),
                        HoldAudio::Tone {
                            frequency_hz,
                            level,
                        } => format!(
                            "Bleep {frequency_hz} Hz {:.0} dB",
                            f64::from(level.millidecibels()) / 1000.0
                        ),
                        HoldAudio::Tail {
                            maximum, effect, ..
                        } => format!(
                            "{} tail {} f",
                            match effect {
                                deadpan_core::TailEffect::Reverb => "Reverb",
                                deadpan_core::TailEffect::Delay => "Delay",
                            },
                            maximum.frames()
                        ),
                    },
                ));
                if matches!(recipe.audio, HoldAudio::Tail { .. }) {
                    // The tail is fed live by what is heard before it, so
                    // edits there change it.
                    fields.push(("Tail of", "Live 2 s before".into()));
                }
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
                iterations,
                gap,
                escalation,
                ..
            } => {
                fields.push(("Total plays", iterations.len().to_string()));
                fields.push((
                    "Between plays",
                    gap.as_ref().map_or_else(
                        || "No gap".into(),
                        |gap| format!("{} f", gap.duration.frames()),
                    ),
                ));
                fields.extend(crate::navigation::escalation::EscalationInput::fields(
                    escalation.as_ref(),
                ));
                (
                    "Repeat",
                    "↻",
                    Some((
                        "Plays, gaps or escalation…",
                        format!("repeat {}", iterations.len()),
                    )),
                    "Set total plays, a held gap between plays with gap=120ms (gap-step=-40ms shortens each later gap), or escalate each play with gain-step=3dB zoom-step=0.08. Wrap repeat adds a new enclosing Repeat.",
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
                let link = match source.link {
                    LinkRelation::Linked => "Linked",
                    LinkRelation::Independent => "Independent",
                };
                fields.push((
                    "Link",
                    match source.audio_offset.0 {
                        0 => link.into(),
                        samples => format!(
                            "{link} · sound {:.1} ms {}",
                            samples.unsigned_abs() as f64 / 48.0,
                            if samples > 0 { "late" } else { "early" }
                        ),
                    },
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
                    "Open this group's child beats, or return to its parent without changing the edit.",
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
                        PitchPolicy::Preserve => "Preserve".into(),
                        PitchPolicy::FollowSpeed => "Tape (follows speed)".into(),
                        PitchPolicy::Shift { semitones } => {
                            format!("Shifted {semitones:+} semitones")
                        }
                    },
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
                deadpan_core::FramingValue::Envelope { envelope } => {
                    // The scale range keeps a ranged punch-in visible in the
                    // narrow value column; the status line gives endpoints.
                    let scales: Vec<f64> = std::iter::once(envelope.initial.scale)
                        .chain(envelope.segments.iter().map(|segment| segment.pose.scale))
                        .map(|ratio| ratio.numerator() as f64 / ratio.denominator() as f64)
                        .collect();
                    let low = scales.iter().copied().fold(f64::INFINITY, f64::min);
                    let high = scales.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    format!("Path · {low:.2}–{high:.2}×")
                }
                deadpan_core::FramingValue::Follow { target, scale, .. } => {
                    let scale = scale.numerator() as f64 / scale.denominator() as f64;
                    format!("Follows {target} · {scale:.3}×")
                }
            };
            fields.push(("Framing", description));
        }
        if let Some(saturation) = node.audio_treatments.saturation() {
            let gain_first = node.audio_treatments.order().first()
                == Some(&deadpan_core::AudioTreatmentStage::ClipGain);
            fields.push((
                "Saturation",
                format!(
                    "{} dB drive{}",
                    crate::gain::format_db(saturation.drive()),
                    if node.audio_treatments.clip_gain().is_none() {
                        ""
                    } else if gain_first {
                        " · after gain"
                    } else {
                        " · before gain"
                    }
                ),
            ));
        }
        for (label, removed) in [("Cutaways", false), ("Picture removed", true)] {
            let ranges: Vec<_> = node
                .cutaways
                .iter()
                .filter(|cutaway| cutaway.removed == removed)
                .map(|cutaway| format!("{}–{}", cutaway.range.start().0, cutaway.range.end().0))
                .collect();
            if !ranges.is_empty() {
                fields.push((label, ranges.join(", ")));
            }
        }
        if !node.captions.is_empty() {
            fields.push((
                "Captions",
                node.captions
                    .iter()
                    .map(|caption| {
                        format!(
                            "“{}” {}–{}",
                            caption.text,
                            caption.range.start().0,
                            caption.range.end().0
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
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
            cutaways: Vec::new(),
            captions: Vec::new(),
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
        let mut driven = node.clone();
        driven.audio_treatments = deadpan_core::AudioTreatments::default()
            .with_saturation(Some(
                deadpan_core::Saturation::new(deadpan_core::GainDb::new(12_500).unwrap()).unwrap(),
            ))
            .unwrap();
        assert_eq!(
            Inspector::describe(&driven, 0, 11).fields.last(),
            Some(&("Saturation", "12.5 dB drive".into()))
        );
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
                escalation: None,
            },
            cutaways: Vec::new(),
            captions: Vec::new(),
        };
        let inspector = Inspector::describe(&node, 143, 72);
        assert_eq!(
            inspector.fields,
            vec![
                ("Total plays", u32::MAX.to_string()),
                ("Between plays", "No gap".into()),
                ("Escalation", "None".into())
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
            cutaways: Vec::new(),
            captions: Vec::new(),
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
