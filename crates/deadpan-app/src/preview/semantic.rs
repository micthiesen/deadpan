//! Admit monotonically newer repeat receipts only in the visible session.

use crate::project::semantic::{RepeatableCut, RepeatableEdit, Snapshot};
use deadpan_core::{ProjectId, SemanticMotion, SemanticSelector};

impl super::DeadpanApp {
    pub(super) fn last_edit_uses_register(&self) -> bool {
        self.semantic
            .snapshot()
            .and_then(|snapshot| snapshot.edit.as_ref())
            .is_none_or(|edit| edit.uses_register())
    }

    pub(super) fn repeat_hint(&self) -> Option<String> {
        let edit = self
            .semantic
            .snapshot()?
            .edit_for(self.workspace.as_ref()?)
            .ok()?;
        let selected_repeat = matches!(edit.operation, RepeatableEdit::SetRepeatPlays { .. })
            && self.workspace.as_ref().is_some_and(|workspace| {
                self.selected_beat.as_ref().is_some_and(|selected| {
                    self.sequence_scope
                        .resolve(workspace)
                        .is_ok_and(|scope| scope.children.contains(selected))
                        && matches!(
                            workspace
                                .document
                                .nodes()
                                .get(selected)
                                .map(|node| &node.kind),
                            Some(deadpan_core::NodeKind::Repeat { .. })
                        )
                })
            });
        Some(repeat_instruction_hint(
            &edit.operation,
            self.edit_selection(),
            selected_repeat,
        ))
    }
}

fn repeat_instruction_hint(
    operation: &RepeatableEdit,
    selection: crate::navigation::EditSelection,
    selected_repeat: bool,
) -> String {
    let visual_hint = |range: String| match selection {
        crate::navigation::EditSelection::Range | crate::navigation::EditSelection::Object => {
            Some(range)
        }
        crate::navigation::EditSelection::Empty => Some("repeat unavailable: empty range".into()),
        crate::navigation::EditSelection::None => None,
    };
    let (action, selector) = match operation {
        RepeatableEdit::Cut(RepeatableCut::Frames(operation)) => {
            return visual_hint("repeat cut selection".into())
                .unwrap_or_else(|| format!("repeat cut {}f", operation.count()));
        }
        RepeatableEdit::Cut(RepeatableCut::Selector(selector)) => {
            if let Some(hint) = visual_hint("repeat cut selection".into()) {
                return hint;
            }
            ("repeat cut".to_owned(), selector)
        }
        RepeatableEdit::Repeat {
            selector,
            plays,
            escalation,
        } => {
            let escalating = if escalation.is_some() {
                " escalating"
            } else {
                ""
            };
            if let Some(hint) = visual_hint(format!("repeat selection ×{plays}{escalating}")) {
                return hint;
            }
            (format!("repeat ×{plays}{escalating}"), selector)
        }
        RepeatableEdit::Group { selector, label } => {
            if let Some(hint) = visual_hint(format!("group selection as {label:?}")) {
                return hint;
            }
            (format!("group as {label:?}"), selector)
        }
        RepeatableEdit::Ungroup => {
            return if selection != crate::navigation::EditSelection::None {
                "repeat unavailable: clear Visual range".into()
            } else {
                "ungroup selected neutral Sequence".into()
            };
        }
        RepeatableEdit::Explode => {
            return if selection != crate::navigation::EditSelection::None {
                "repeat unavailable: clear Visual range".into()
            } else if selected_repeat {
                "explode selected Repeat".into()
            } else {
                "select Repeat to explode".into()
            };
        }
        RepeatableEdit::Duplicate { selector } => {
            if let Some(hint) = visual_hint("duplicate selection".into()) {
                return hint;
            }
            ("duplicate".to_owned(), selector)
        }
        RepeatableEdit::Parameter(instruction) => {
            let text = applied_text(instruction).unwrap_or_else(|| "the last edit".into());
            let mut chars = text.chars();
            let first = chars
                .next()
                .map(|c| c.to_lowercase().to_string())
                .unwrap_or_default();
            return format!("repeat: {first}{}", chars.as_str());
        }
        RepeatableEdit::SetRepeatPlays { plays } => {
            return if selection != crate::navigation::EditSelection::None {
                "repeat unavailable: clear Visual range".into()
            } else if selected_repeat {
                format!("set Repeat to {plays} plays")
            } else {
                format!("select Repeat to set {plays} plays")
            };
        }
    };
    match selector {
        SemanticSelector::VisualSelection => format!("select range to {action}"),
        selector => format!("{action} {}", selector_text(selector)),
    }
}

/// What a non-Visual selector selects, as a short phrase.
pub(super) fn selector_text(selector: &SemanticSelector) -> String {
    match selector {
        SemanticSelector::SelectedBeat => "beat".into(),
        SemanticSelector::VisualSelection => "range".into(),
        SemanticSelector::TextObject { object } => object.noun().into(),
        SemanticSelector::Speech { object } => match object {
            deadpan_core::SpeechObject::InnerWord => "word",
            deadpan_core::SpeechObject::AroundWord => "word with pauses",
            deadpan_core::SpeechObject::InnerSentence => "sentence",
            deadpan_core::SpeechObject::AroundSentence => "sentence with pauses",
            deadpan_core::SpeechObject::InnerPause => "pause",
            deadpan_core::SpeechObject::AroundPause => "pause with edges",
            deadpan_core::SpeechObject::InnerShot => "shot",
            deadpan_core::SpeechObject::AroundShot => "shot with transitions",
        }
        .into(),
        SemanticSelector::Motion { motion } => {
            let direction = |forward: bool| if forward { "forward" } else { "backward" };
            match motion {
                SemanticMotion::Frames { forward, count } => {
                    format!("{count}f {}", direction(*forward))
                }
                SemanticMotion::Beats { forward, count } => {
                    format!("{count} beats {}", direction(*forward))
                }
                SemanticMotion::Scope { end } => {
                    format!("to group {}", if *end { "end" } else { "start" })
                }
                SemanticMotion::Words {
                    forward,
                    count,
                    end,
                } => match (forward, end) {
                    (true, true) => format!("to end of {count} words"),
                    (_, _) => format!("{count} words {}", direction(*forward)),
                },
                SemanticMotion::Sentences { forward, count } => {
                    format!("{count} sentences {}", direction(*forward))
                }
                SemanticMotion::Pauses { forward, count } => {
                    format!("{count} pauses {}", direction(*forward))
                }
                SemanticMotion::Shots { forward, count } => {
                    format!("{count} shots {}", direction(*forward))
                }
            }
        }
    }
}

fn pause_text(length: deadpan_core::PauseLength) -> String {
    match length {
        deadpan_core::PauseLength::Frames { frames } => format!("{frames}f"),
        deadpan_core::PauseLength::Milliseconds { milliseconds } => format!("{milliseconds}ms"),
    }
}

/// The footer message after an applied instruction, naming the Edit range
/// a ranged gain step, caption or cutaway acted on.
pub(super) fn applied_summary(
    instruction: &deadpan_core::SemanticInstruction,
    context: &deadpan_core::SemanticContext,
) -> Option<String> {
    use deadpan_core::SemanticInstruction as I;
    let text = applied_text(instruction)?;
    let Some(deadpan_core::SemanticVisualSelection::Time { anchor, head, .. }) =
        &context.visual_selection
    else {
        return Some(text);
    };
    let (start, end) = (anchor.0.min(head.0), anchor.0.max(head.0));
    Some(match instruction {
        I::SetAudio { .. } => format!("{text} over Edit {start}–{end}"),
        I::SetCaption { text: caption, .. } => {
            format!("Captioned Edit {start}–{end}: “{caption}”")
        }
        I::SetCutaway { register, .. } => format!(
            "Cutaway from register {} placed over Edit {start}–{end}",
            register.as_char()
        ),
        _ => text,
    })
}

/// The footer message after an applied editor operator.
pub(super) fn applied_text(instruction: &deadpan_core::SemanticInstruction) -> Option<String> {
    use deadpan_core::SemanticInstruction as I;
    let (verb, selector) = match instruction {
        I::Cut { selector, .. } => ("Cut", selector),
        I::Yank { selector, .. } => ("Copied", selector),
        I::Repeat {
            selector,
            plays,
            escalation,
        } => {
            let escalated = if escalation.is_some() {
                ", escalating"
            } else {
                ""
            };
            return (!matches!(selector, SemanticSelector::VisualSelection))
                .then(|| format!("Repeated {} ×{plays}{escalated}", selector_text(selector)));
        }
        I::Gag { recipe } => return Some(format!("Applied {}", recipe.name())),
        I::SetGag { recipe, parameters } if parameters.is_empty() => {
            return Some(format!("Gag set: {}", recipe.label()));
        }
        I::SetGag { recipe, parameters } => {
            let names: Vec<&str> = parameters
                .iter()
                .flat_map(|parameter| crate::navigation::gag::parameter_keys(*parameter))
                .copied()
                .collect();
            let values: Vec<String> = crate::navigation::gag::arguments(recipe)
                .into_iter()
                .filter(|(key, _)| names.contains(key))
                .map(|(key, value)| format!("{key} {value}"))
                .collect();
            return Some(format!("{} set: {}", recipe.name(), values.join(", ")));
        }
        I::SetAudioLag { offset } => {
            return Some(match offset.0 {
                0 => "Sound offset removed".to_owned(),
                samples => {
                    // 48 samples per millisecond, shown to the microsecond.
                    let micros = samples.unsigned_abs() * 1000 / 48;
                    let fraction = format!("{:03}", micros % 1000);
                    let fraction = fraction.trim_end_matches('0');
                    format!(
                        "Sound plays {}{}{fraction} ms {}",
                        micros / 1000,
                        if fraction.is_empty() { "" } else { "." },
                        if samples > 0 { "later" } else { "earlier" }
                    )
                }
            });
        }
        I::SetAudioEdges { side, policy } => {
            let side = match side {
                deadpan_core::EdgeSide::Start => "start",
                deadpan_core::EdgeSide::End => "end",
                deadpan_core::EdgeSide::Both => "start and end",
                deadpan_core::EdgeSide::Plays => "play seams",
                deadpan_core::EdgeSide::Gaps => "gap edges",
            };
            return Some(match policy {
                deadpan_core::AudioEdgePolicy::Hard => {
                    format!("Sound {side} cut hard, without the automatic fade")
                }
                deadpan_core::AudioEdgePolicy::Automatic => {
                    format!("Sound {side} use the automatic fade")
                }
            });
        }
        I::SetRepeat {
            plays,
            gaps,
            escalation,
        } => {
            let mut parts = Vec::new();
            if let Some(plays) = plays {
                parts.push(format!("{plays} plays"));
            }
            if let Some(gaps) = gaps {
                parts.push(if gaps.is_empty() {
                    "no gap".to_owned()
                } else {
                    let lengths: Vec<_> = gaps.iter().map(|gap| pause_text(*gap)).collect();
                    format!("gaps {}", lengths.join(", "))
                });
            }
            if let Some(escalation) = escalation {
                parts.push(
                    if escalation.gain_step == deadpan_core::GainDb::UNITY
                        && escalation.zoom.is_none()
                    {
                        "no escalation".to_owned()
                    } else {
                        "escalation".to_owned()
                    },
                );
            }
            return Some(format!("Repeat set: {}", parts.join("; ")));
        }
        I::SetRoomTone { .. } => return Some("Room tone applied".into()),
        I::Bleep { frequency_hz, .. } => {
            return Some(format!("Bleeped the range with a {frequency_hz} Hz tone"));
        }
        I::Lift { .. } => {
            return Some("Lifted the range: its time is now a silent black pause".into());
        }
        I::SetCaption { text, .. } => {
            return Some(format!("Captioned the beat: “{text}”"));
        }
        I::SetCutaway { register, .. } => {
            return Some(format!(
                "Cutaway from register {} placed",
                register.as_char()
            ));
        }
        I::InsertPause { .. } => return Some("Inserted a pause".into()),
        I::InsertAiPause { .. } => {
            return Some("Inserted an AI pause".into());
        }
        I::InsertReverse { bounce: false, .. } => {
            return Some("Inserted a reverse: the moment before the cursor plays backwards".into());
        }
        I::InsertReverse { bounce: true, .. } => {
            return Some("Inserted a ping-pong: the moment before the cursor bounces back".into());
        }
        I::Tail { effect, .. } => {
            return Some(format!("Added a {} tail", effect.name()));
        }
        I::SetFraming { .. } => return Some("Framed the beat".into()),
        I::Retime { speed, pitch, wrap } => {
            let pitch = match pitch {
                deadpan_core::PitchPolicy::Preserve => "preserve pitch".to_owned(),
                deadpan_core::PitchPolicy::FollowSpeed => "tape pitch".to_owned(),
                deadpan_core::PitchPolicy::Shift { semitones } => {
                    format!("pitch {semitones:+} semitones")
                }
            };
            let wrap = if *wrap { " · new Retime" } else { "" };
            return Some(format!(
                "Speed {}/{}× · {pitch}{wrap}",
                speed.numerator(),
                speed.denominator()
            ));
        }
        I::Pitch { semitones: 0 } => return Some("Pitch shift removed".into()),
        I::Pitch { semitones } => {
            return Some(format!("Pitch shifted {semitones:+} semitones"));
        }
        I::SetHoldDuration { length } => {
            return Some(format!("Pause length set to {}", pause_text(*length)));
        }
        I::SetAudio { change } => return Some(audio_change_text(*change)),
        I::RoleRepeat { role, plays, .. } => {
            return Some(match role {
                deadpan_core::MediaRole::Audio => {
                    format!("Repeated the range's sound ×{plays} over its beat; no time added")
                }
                _ => format!("Repeated the range's picture ×{plays} over its beat; no time added"),
            });
        }
        I::DeleteRole { role } => {
            return Some(match role {
                deadpan_core::MediaRole::Audio => {
                    "Deleted the range's sound; its picture and time stay".into()
                }
                _ => {
                    "Deleted the range's picture to the background; its sound and time stay".into()
                }
            });
        }
        I::SplitEdit { kind, length } => {
            return Some(match kind {
                deadpan_core::SplitEditKind::J => {
                    format!("J-cut: the next sound starts {} early", pause_text(*length))
                }
                deadpan_core::SplitEditKind::L => format!(
                    "L-cut: this sound runs on {} under the next picture",
                    pause_text(*length)
                ),
            });
        }
        I::Group { selector, label } => {
            return (!matches!(selector, SemanticSelector::VisualSelection))
                .then(|| format!("Grouped {} as {label:?}", selector_text(selector)));
        }
        I::Explode => {
            return Some("Exploded the Repeat into independent plays; Undo restores it".into());
        }
        I::Duplicate { selector } => ("Duplicated", selector),
        _ => return None,
    };
    (!matches!(selector, SemanticSelector::VisualSelection))
        .then(|| format!("{verb} {}", selector_text(selector)))
}

#[derive(Default)]
pub(super) struct Mirror {
    snapshot: Option<Snapshot>,
}

impl Mirror {
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    pub fn receive(&mut self, incoming: Option<Snapshot>, context: Option<(u64, &ProjectId)>) {
        let matches = |snapshot: &Snapshot| {
            context.is_some_and(|(session, project)| {
                snapshot.session == session && &snapshot.project == project
            })
        };
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| !matches(snapshot))
        {
            self.snapshot = None;
        }
        if let Some(incoming) = incoming
            && matches(&incoming)
            && self
                .snapshot
                .as_ref()
                .is_none_or(|previous| incoming.version > previous.version)
        {
            self.snapshot = Some(incoming);
        }
    }
}

/// What a recorded audio change does, with its exact value.
pub(super) fn audio_change_text(change: deadpan_core::AudioChange) -> String {
    use deadpan_core::AudioChange;
    match change {
        AudioChange::Trim { gain } => format!("Gain set to {} dB", crate::gain::format_db(gain)),
        AudioChange::Step { millidecibels } | AudioChange::RangeStep { millidecibels } => {
            let scope = if matches!(change, AudioChange::RangeStep { .. }) {
                "Range gain"
            } else {
                "Gain"
            };
            let sign = if millidecibels < 0 { "-" } else { "+" };
            let magnitude = millidecibels.unsigned_abs();
            let fraction = format!("{:03}", magnitude % 1000);
            let fraction = fraction.trim_end_matches('0');
            let point = if fraction.is_empty() { "" } else { "." };
            format!(
                "{scope} changed by {sign}{}{point}{fraction} dB",
                magnitude / 1000
            )
        }
        AudioChange::Saturation { drive: Some(drive) } => {
            format!("Saturation drive {} dB", crate::gain::format_db(drive))
        }
        AudioChange::Saturation { drive: None } => "Saturation removed".into(),
        AudioChange::Mute { muted: true } => "Beat muted".into(),
        AudioChange::Mute { muted: false } => "Beat unmuted".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::semantic::LastEdit;
    use deadpan_core::{FrameCut, RevisionId};

    #[test]
    fn group_repeat_teaches_retained_label_and_current_visual_scope() {
        use crate::navigation::EditSelection;
        let operation = RepeatableEdit::Group {
            selector: SemanticSelector::VisualSelection,
            label: "the answer".into(),
        };
        assert_eq!(
            repeat_instruction_hint(&operation, EditSelection::None, false),
            "select range to group as \"the answer\""
        );
        assert_eq!(
            repeat_instruction_hint(&operation, EditSelection::Range, false),
            "group selection as \"the answer\""
        );
        assert_eq!(
            repeat_instruction_hint(&operation, EditSelection::Empty, false),
            "repeat unavailable: empty range"
        );
        for selection in [EditSelection::Empty, EditSelection::Range] {
            assert_eq!(
                repeat_instruction_hint(&RepeatableEdit::Ungroup, selection, false),
                "repeat unavailable: clear Visual range"
            );
        }
        assert_eq!(
            repeat_instruction_hint(&RepeatableEdit::Ungroup, EditSelection::None, false),
            "ungroup selected neutral Sequence"
        );
    }

    #[test]
    fn repeat_count_hint_requires_an_explicit_repeat_and_no_visual_state() {
        use crate::navigation::EditSelection;
        let operation = RepeatableEdit::SetRepeatPlays {
            plays: std::num::NonZeroU32::new(3).unwrap(),
        };
        assert_eq!(
            repeat_instruction_hint(&operation, EditSelection::None, true),
            "set Repeat to 3 plays"
        );
        assert_eq!(
            repeat_instruction_hint(&operation, EditSelection::None, false),
            "select Repeat to set 3 plays"
        );
        for selection in [EditSelection::Empty, EditSelection::Range] {
            for selected_repeat in [false, true] {
                assert_eq!(
                    repeat_instruction_hint(&operation, selection, selected_repeat),
                    "repeat unavailable: clear Visual range"
                );
            }
        }
    }

    #[test]
    fn selector_and_frame_repeat_hints_keep_visual_override_teaching() {
        use crate::navigation::EditSelection;
        let cut = RepeatableEdit::Cut(RepeatableCut::Frames(FrameCut::new(5).unwrap()));
        assert_eq!(
            repeat_instruction_hint(&cut, EditSelection::None, false),
            "repeat cut 5f"
        );
        assert_eq!(
            repeat_instruction_hint(&cut, EditSelection::Range, false),
            "repeat cut selection"
        );
        assert_eq!(
            repeat_instruction_hint(&cut, EditSelection::Empty, false),
            "repeat unavailable: empty range"
        );
        let wrapped = RepeatableEdit::Repeat {
            selector: SemanticSelector::VisualSelection,
            plays: std::num::NonZeroU32::new(3).unwrap(),
            escalation: None,
        };
        assert_eq!(
            repeat_instruction_hint(&wrapped, EditSelection::None, true),
            "select range to repeat ×3"
        );
        assert_eq!(
            repeat_instruction_hint(&wrapped, EditSelection::Range, true),
            "repeat selection ×3"
        );
        assert_eq!(
            repeat_instruction_hint(&wrapped, EditSelection::Empty, true),
            "repeat unavailable: empty range"
        );
    }

    #[test]
    fn late_snapshots_cannot_restore_cleared_edits_or_cross_sessions() {
        let project = ProjectId::new("project").unwrap();
        let mut saved = Snapshot {
            session: 1,
            project: project.clone(),
            version: 2,
            head: Some(RevisionId::new("cut").unwrap()),
            edit: Some(LastEdit {
                operation: RepeatableEdit::Cut(RepeatableCut::Frames(FrameCut::new(5).unwrap())),
                register: None,
            }),
            error: None,
        };
        let mut mirror = Mirror::default();
        mirror.receive(Some(saved.clone()), Some((1, &project)));
        let mut cleared = saved.clone();
        cleared.version = 3;
        cleared.edit = None;
        mirror.receive(Some(cleared.clone()), Some((1, &project)));
        mirror.receive(Some(saved.clone()), Some((1, &project)));
        assert_eq!(mirror.snapshot(), Some(&cleared));
        mirror.receive(Some(saved.clone()), Some((2, &project)));
        assert!(mirror.snapshot().is_none());
        saved.session = 2;
        mirror.receive(Some(saved.clone()), Some((2, &project)));
        mirror.receive(None, None);
        assert!(mirror.snapshot().is_none());
        mirror.receive(Some(saved), None);
        assert!(mirror.snapshot().is_none());
    }
}
