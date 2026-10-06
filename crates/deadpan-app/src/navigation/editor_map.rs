//! Immutable editor grammars shared by routing, labels, and shortcut auditing.

use std::sync::{Arc, LazyLock};

use super::binding_trie::{Binding, Prefix, Trie};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stroke {
    Key(Key, bool),
    /// Egui has no `@` key identity. Only paired native text proves this symbol.
    At,
}

impl Stroke {
    pub fn label(self) -> String {
        let Self::Key(key, shift) = self else {
            return "@".into();
        };
        if self == Self::Key(Key::Quote, true) {
            return "\"".into();
        }
        if let Some(letter) = mark_letter(key, shift) {
            return letter.to_string();
        }
        let name = match key {
            Key::Comma => ",",
            Key::Period => ".",
            Key::Quote => "'",
            Key::ArrowLeft => "Left",
            Key::ArrowRight => "Right",
            Key::ArrowUp => "Up",
            Key::ArrowDown => "Down",
            Key::Plus => "+",
            Key::Minus => "-",
            Key::Colon => ":",
            Key::Slash => "/",
            Key::Questionmark => "?",
            Key::OpenBracket => "[",
            Key::CloseBracket => "]",
            Key::Escape => "Esc",
            _ => key.name(),
        };
        if shift {
            format!("Shift+{name}")
        } else {
            name.into()
        }
    }
}

/// Stable semantic IDs. Configurations replace paths, never action policies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingId {
    FramePrevious,
    FrameNext,
    BeatPrevious,
    BeatNext,
    WordNext,
    WordPrevious,
    WordEnd,
    SentenceNext,
    SentencePrevious,
    PauseNext,
    PausePrevious,
    ShotNext,
    ShotPrevious,
    PlayNext,
    PlayPrevious,
    First,
    Last,
    Undo,
    Playback,
    Audition,
    EnterGroup,
    LeaveGroup,
    Group,
    Ungroup,
    Visual,
    InnerGroup,
    AroundGroup,
    InnerWord,
    AroundWord,
    InnerSentence,
    AroundSentence,
    InnerPause,
    AroundPause,
    InnerShot,
    AroundShot,
    Copy,
    CopyBeat,
    CutOperator,
    YankOperator,
    PasteAfter,
    PasteBefore,
    Split,
    CutFrames,
    RepeatLast,
    CutBeat,
    CutRange,
    Repeat,
    RepeatOperator,
    RepeatRange,
    EscalatingRepeat,
    GenerateAi,
    Hold,
    Insert,
    PlaceSound,
    GainUp,
    GainDown,
    Mute,
    CutawayPicker,
    Tail,
    Bleep,
    Camera,
    PunchIn,
    Creep,
    Trim,
    MarkSet,
    MarkJump,
    RegisterSelect,
    MacroRecord,
    MacroExecute,
    Command,
    Search,
    SearchNext,
    SearchPrevious,
    Help,
    PaneNext,
    PanePrevious,
    Escape,
}

impl BindingId {
    pub const ALL: [Self; 77] = [
        Self::FramePrevious,
        Self::FrameNext,
        Self::BeatPrevious,
        Self::BeatNext,
        Self::WordNext,
        Self::WordPrevious,
        Self::WordEnd,
        Self::SentenceNext,
        Self::SentencePrevious,
        Self::PauseNext,
        Self::PausePrevious,
        Self::ShotNext,
        Self::ShotPrevious,
        Self::PlayNext,
        Self::PlayPrevious,
        Self::First,
        Self::Last,
        Self::Undo,
        Self::Playback,
        Self::Audition,
        Self::EnterGroup,
        Self::LeaveGroup,
        Self::Group,
        Self::Ungroup,
        Self::Visual,
        Self::InnerGroup,
        Self::AroundGroup,
        Self::InnerWord,
        Self::AroundWord,
        Self::InnerSentence,
        Self::AroundSentence,
        Self::InnerPause,
        Self::AroundPause,
        Self::InnerShot,
        Self::AroundShot,
        Self::Copy,
        Self::CopyBeat,
        Self::CutOperator,
        Self::YankOperator,
        Self::PasteAfter,
        Self::PasteBefore,
        Self::Split,
        Self::CutFrames,
        Self::RepeatLast,
        Self::CutBeat,
        Self::CutRange,
        Self::Repeat,
        Self::RepeatOperator,
        Self::RepeatRange,
        Self::EscalatingRepeat,
        Self::GenerateAi,
        Self::Hold,
        Self::Insert,
        Self::PlaceSound,
        Self::GainUp,
        Self::GainDown,
        Self::Mute,
        Self::CutawayPicker,
        Self::Tail,
        Self::Bleep,
        Self::Camera,
        Self::PunchIn,
        Self::Creep,
        Self::Trim,
        Self::MarkSet,
        Self::MarkJump,
        Self::RegisterSelect,
        Self::MacroRecord,
        Self::MacroExecute,
        Self::Command,
        Self::Search,
        Self::SearchNext,
        Self::SearchPrevious,
        Self::Help,
        Self::PaneNext,
        Self::PanePrevious,
        Self::Escape,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FramePrevious => "frame.previous",
            Self::FrameNext => "frame.next",
            Self::BeatPrevious => "beat.previous",
            Self::BeatNext => "beat.next",
            Self::WordNext => "word.next",
            Self::WordPrevious => "word.previous",
            Self::WordEnd => "word.end",
            Self::SentenceNext => "sentence.next",
            Self::SentencePrevious => "sentence.previous",
            Self::PauseNext => "pause.next",
            Self::PausePrevious => "pause.previous",
            Self::ShotNext => "shot.next",
            Self::ShotPrevious => "shot.previous",
            Self::PlayNext => "play.next",
            Self::PlayPrevious => "play.previous",
            Self::First => "first",
            Self::Last => "last",
            Self::Undo => "undo",
            Self::Playback => "playback",
            Self::Audition => "audition",
            Self::EnterGroup => "group.enter",
            Self::Group => "group.create",
            Self::Ungroup => "group.ungroup",
            Self::LeaveGroup => "group.leave",
            Self::Visual => "visual",
            Self::InnerGroup => "object.inner_group",
            Self::AroundGroup => "object.around_group",
            Self::InnerWord => "object.inner_word",
            Self::AroundWord => "object.around_word",
            Self::InnerSentence => "object.inner_sentence",
            Self::AroundSentence => "object.around_sentence",
            Self::InnerPause => "object.inner_pause",
            Self::AroundPause => "object.around_pause",
            Self::InnerShot => "object.inner_shot",
            Self::AroundShot => "object.around_shot",
            Self::Copy => "copy",
            Self::CopyBeat => "copy.beat",
            Self::CutOperator => "cut.operator",
            Self::YankOperator => "yank.operator",
            Self::PasteAfter => "paste.after",
            Self::PasteBefore => "paste.before",
            Self::Split => "split",
            Self::CutFrames => "cut.frames",
            Self::RepeatLast => "edit.repeat-last",
            Self::CutBeat => "cut.beat",
            Self::CutRange => "cut.range",
            Self::Repeat => "repeat",
            Self::RepeatOperator => "repeat.operator",
            Self::RepeatRange => "repeat.range",
            Self::EscalatingRepeat => "repeat.escalating",
            Self::GenerateAi => "ai.generate",
            Self::Hold => "hold",
            Self::Insert => "insert",
            Self::PlaceSound => "sound.place",
            Self::GainUp => "gain.up",
            Self::GainDown => "gain.down",
            Self::Mute => "gain.mute",
            Self::CutawayPicker => "cutaway.pick",
            Self::Tail => "tail",
            Self::Bleep => "bleep",
            Self::Camera => "camera",
            Self::PunchIn => "punch_in",
            Self::Creep => "creep",
            Self::Trim => "trim",
            Self::MarkSet => "mark.set",
            Self::MarkJump => "mark.jump",
            Self::RegisterSelect => "register.select",
            Self::MacroRecord => "macro.record",
            Self::MacroExecute => "macro.execute",
            Self::Command => "command",
            Self::Search => "search",
            Self::Help => "help",
            Self::SearchNext => "search.next",
            Self::SearchPrevious => "search.previous",
            Self::PaneNext => "pane.next",
            Self::PanePrevious => "pane.previous",
            Self::Escape => "escape",
        }
    }
    pub(super) fn fixed(self) -> bool {
        matches!(self, Self::Escape | Self::PaneNext | Self::PanePrevious)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum KeyMode {
    Logical,
    Physical,
}

#[derive(Clone, Copy)]
enum CountPolicy {
    Ignore,
    Motion,
    Frames,
    Repeat,
    LegacyRepeat,
    DeleteBeat,
    Operator,
    Hold,
    Gain,
    Macro,
    Refuse(&'static str),
}

#[derive(Clone, Copy, Default)]
pub(super) struct Counts {
    pub leading: Option<u32>,
    pub motion: Option<u32>,
}

#[derive(Clone, Copy)]
pub(super) struct Rule {
    action: Action,
    count: CountPolicy,
    short: &'static str,
    pub repeatable: bool,
    pub interrupt: bool,
}

impl Rule {
    pub fn resolve(&self, count: Option<u32>) -> Action {
        self.resolve_counts(Counts {
            leading: count,
            motion: None,
        })
    }

    pub fn resolve_counts(&self, counts: Counts) -> Action {
        if matches!(self.count, CountPolicy::Operator | CountPolicy::Repeat)
            && counts.leading.is_some()
            && counts.motion.is_some()
        {
            return Action::Invalid(
                "Use one count before the operator or before its motion, not both.",
            );
        }
        let count = counts.motion.or(counts.leading);
        match self.count {
            CountPolicy::Repeat => {
                let Action::Repeat { selector, .. } = self.action else {
                    unreachable!()
                };
                use deadpan_core::{SemanticMotion as M, SemanticSelector as S};
                let Some(plays) = std::num::NonZeroU32::new(counts.leading.unwrap_or(2)) else {
                    return Action::Invalid("An edit count must be positive; no edit was made.");
                };
                let Some(distance) = std::num::NonZeroU32::new(counts.motion.unwrap_or(1)) else {
                    return Action::Invalid("A motion count must be positive; no edit was made.");
                };
                let selector = match selector {
                    S::Motion {
                        motion: M::Frames { forward, .. },
                    } => S::Motion {
                        motion: M::Frames {
                            forward,
                            count: distance,
                        },
                    },
                    S::Motion {
                        motion: M::Beats { forward, .. },
                    } => S::Motion {
                        motion: M::Beats {
                            forward,
                            count: distance,
                        },
                    },
                    S::Motion {
                        motion: M::Words { forward, end, .. },
                    } => S::Motion {
                        motion: M::Words {
                            forward,
                            count: distance,
                            end,
                        },
                    },
                    S::Motion {
                        motion: M::Sentences { forward, .. },
                    } => S::Motion {
                        motion: M::Sentences {
                            forward,
                            count: distance,
                        },
                    },
                    S::Motion {
                        motion: M::Pauses { forward, .. },
                    } => S::Motion {
                        motion: M::Pauses {
                            forward,
                            count: distance,
                        },
                    },
                    S::Motion {
                        motion: M::Shots { forward, .. },
                    } => S::Motion {
                        motion: M::Shots {
                            forward,
                            count: distance,
                        },
                    },
                    _ if counts.motion.is_some() => {
                        return Action::Invalid(
                            "A motion count requires frame, beat, word, sentence, pause or shot motion; put total plays before Repeat.",
                        );
                    }
                    selector => selector,
                };
                Action::Repeat { selector, plays }
            }
            CountPolicy::Operator => {
                let Action::Operator { cut, selector } = self.action else {
                    unreachable!()
                };
                use deadpan_core::{SemanticMotion as M, SemanticSelector as S};
                let count = match count {
                    Some(0) => {
                        return Action::Invalid(
                            "An operator count must be positive; no action was taken.",
                        );
                    }
                    value => value,
                };
                let selector = match selector {
                    S::TextObject { .. } if count.is_some_and(|count| count > 1) => {
                        return Action::Invalid(
                            "A group object selects one group; larger counts are not supported.",
                        );
                    }
                    S::Speech { .. } if count.is_some_and(|count| count > 1) => {
                        return Action::Invalid(
                            "A word, sentence, pause or shot object selects one; use a motion such as d3w for more.",
                        );
                    }
                    S::SelectedBeat if count.is_some_and(|count| count > 1) => {
                        return Action::Invalid(
                            "A whole-beat operator selects one beat; larger counts are not supported.",
                        );
                    }
                    S::Motion {
                        motion: M::Scope { .. },
                    } if count.is_some() => {
                        return Action::Invalid(
                            "A group-boundary operator does not accept a count.",
                        );
                    }
                    S::Motion {
                        motion: M::Frames { forward, .. },
                    } => S::Motion {
                        motion: M::Frames {
                            forward,
                            count: std::num::NonZeroU32::new(count.unwrap_or(1)).unwrap(),
                        },
                    },
                    S::Motion {
                        motion: M::Beats { forward, .. },
                    } => S::Motion {
                        motion: M::Beats {
                            forward,
                            count: std::num::NonZeroU32::new(count.unwrap_or(1)).unwrap(),
                        },
                    },
                    S::Motion {
                        motion: M::Words { forward, end, .. },
                    } => S::Motion {
                        motion: M::Words {
                            forward,
                            count: std::num::NonZeroU32::new(count.unwrap_or(1)).unwrap(),
                            end,
                        },
                    },
                    S::Motion {
                        motion: M::Sentences { forward, .. },
                    } => S::Motion {
                        motion: M::Sentences {
                            forward,
                            count: std::num::NonZeroU32::new(count.unwrap_or(1)).unwrap(),
                        },
                    },
                    S::Motion {
                        motion: M::Pauses { forward, .. },
                    } => S::Motion {
                        motion: M::Pauses {
                            forward,
                            count: std::num::NonZeroU32::new(count.unwrap_or(1)).unwrap(),
                        },
                    },
                    S::Motion {
                        motion: M::Shots { forward, .. },
                    } => S::Motion {
                        motion: M::Shots {
                            forward,
                            count: std::num::NonZeroU32::new(count.unwrap_or(1)).unwrap(),
                        },
                    },
                    selector => selector,
                };
                Action::Operator { cut, selector }
            }
            CountPolicy::Ignore => self.action,
            CountPolicy::Motion => match self.action {
                Action::Step { forward, .. } => Action::Step {
                    forward,
                    count: count.unwrap_or(1).max(1),
                },
                Action::Beat { forward, .. } => Action::Beat {
                    forward,
                    count: count.unwrap_or(1).max(1),
                },
                Action::Word { forward, end, .. } => Action::Word {
                    forward,
                    end,
                    count: count.unwrap_or(1).max(1),
                },
                Action::Sentence { forward, .. } => Action::Sentence {
                    forward,
                    count: count.unwrap_or(1).max(1),
                },
                Action::Pause { forward, .. } => Action::Pause {
                    forward,
                    count: count.unwrap_or(1).max(1),
                },
                Action::Shot { forward, .. } => Action::Shot {
                    forward,
                    count: count.unwrap_or(1).max(1),
                },
                Action::Play { forward, .. } => Action::Play {
                    forward,
                    count: count.unwrap_or(1).max(1),
                },
                _ => unreachable!("motion declarations carry a motion action"),
            },
            CountPolicy::Frames => match count {
                Some(0) => Action::Invalid("A frame cut count must be positive; no edit was made."),
                value => Action::DeleteFrames(value.unwrap_or(1)),
            },
            CountPolicy::Macro => match (self.action, count) {
                (_, Some(0)) => {
                    Action::Invalid("A macro count must be positive; no edit was made.")
                }
                (Action::MacroExecute { register, .. }, count) => Action::MacroExecute {
                    register,
                    count: count.unwrap_or(1),
                },
                _ => unreachable!("macro declarations carry a macro execution"),
            },
            CountPolicy::LegacyRepeat => match count {
                Some(0) => Action::Invalid("An edit count must be positive; no edit was made."),
                value => Action::Edit(BeatEdit::WrapRepeat(value.unwrap_or(2))),
            },
            CountPolicy::DeleteBeat => match count {
                Some(0) => Action::Invalid("An edit count must be positive; no edit was made."),
                None | Some(1) => self.action,
                Some(_) => Action::Invalid(
                    "Whole-beat cut deletes one selected beat. Counted deletion is not available.",
                ),
            },
            CountPolicy::Hold => Action::Edit(BeatEdit::InsertHold(
                duration::DurationInput::half_seconds(count.unwrap_or(1)),
            )),
            CountPolicy::Gain => {
                let Action::GainStep(step) = self.action else {
                    unreachable!("gain declarations carry a gain action")
                };
                if count == Some(0) {
                    return Action::Invalid("A gain count must be positive; no edit was made.");
                }
                match i32::try_from(count.unwrap_or(1))
                    .ok()
                    .and_then(|value| value.checked_mul(step))
                {
                    Some(value) => Action::GainStep(value),
                    None => {
                        Action::Invalid("Gain step exceeds the supported range; no edit was made.")
                    }
                }
            }
            CountPolicy::Refuse(message) => {
                if count.is_some() {
                    Action::Invalid(message)
                } else {
                    self.action
                }
            }
        }
    }
}

impl Rule {
    pub fn id(&self) -> BindingId {
        use BindingId as I;
        match self.action {
            Action::Repeat { selector, .. } => match selector {
                deadpan_core::SemanticSelector::SelectedBeat => I::Repeat,
                deadpan_core::SemanticSelector::VisualSelection => I::RepeatRange,
                deadpan_core::SemanticSelector::Motion { .. }
                | deadpan_core::SemanticSelector::TextObject { .. }
                | deadpan_core::SemanticSelector::Speech { .. } => I::RepeatOperator,
            },
            Action::Operator { cut, selector } => match selector {
                deadpan_core::SemanticSelector::SelectedBeat => {
                    if cut {
                        I::CutBeat
                    } else {
                        I::CopyBeat
                    }
                }
                _ => {
                    if cut {
                        I::CutOperator
                    } else {
                        I::YankOperator
                    }
                }
            },
            Action::Step { forward: false, .. } => I::FramePrevious,
            Action::Step { forward: true, .. } => I::FrameNext,
            Action::Beat { forward: false, .. } => I::BeatPrevious,
            Action::Beat { forward: true, .. } => I::BeatNext,
            Action::Word {
                forward: true,
                end: false,
                ..
            } => I::WordNext,
            Action::Word { forward: false, .. } => I::WordPrevious,
            Action::Word { end: true, .. } => I::WordEnd,
            Action::Sentence { forward: true, .. } => I::SentenceNext,
            Action::Sentence { forward: false, .. } => I::SentencePrevious,
            Action::Pause { forward: true, .. } => I::PauseNext,
            Action::Pause { forward: false, .. } => I::PausePrevious,
            Action::Shot { forward: true, .. } => I::ShotNext,
            Action::Shot { forward: false, .. } => I::ShotPrevious,
            Action::Play { forward: true, .. } => I::PlayNext,
            Action::Play { forward: false, .. } => I::PlayPrevious,
            Action::SelectSpeech(deadpan_core::SpeechObject::InnerWord) => I::InnerWord,
            Action::SelectSpeech(deadpan_core::SpeechObject::AroundWord) => I::AroundWord,
            Action::SelectSpeech(deadpan_core::SpeechObject::InnerSentence) => I::InnerSentence,
            Action::SelectSpeech(deadpan_core::SpeechObject::AroundSentence) => I::AroundSentence,
            Action::SelectSpeech(deadpan_core::SpeechObject::InnerPause) => I::InnerPause,
            Action::SelectSpeech(deadpan_core::SpeechObject::AroundPause) => I::AroundPause,
            Action::SelectSpeech(deadpan_core::SpeechObject::InnerShot) => I::InnerShot,
            Action::SelectSpeech(deadpan_core::SpeechObject::AroundShot) => I::AroundShot,
            Action::First => I::First,
            Action::Last => I::Last,
            Action::Undo => I::Undo,
            Action::Playback => I::Playback,
            Action::Audition => I::Audition,
            Action::EnterGroup => I::EnterGroup,
            Action::LeaveGroup => I::LeaveGroup,
            Action::Group => I::Group,
            Action::Ungroup => I::Ungroup,
            Action::VisualMoment => I::Visual,
            Action::SelectObject(deadpan_core::SemanticTextObject::InnerGroup) => I::InnerGroup,
            Action::SelectObject(deadpan_core::SemanticTextObject::AroundGroup) => I::AroundGroup,
            Action::CopyMoment => I::Copy,
            Action::PasteMoment { before: false } => I::PasteAfter,
            Action::PasteMoment { before: true } => I::PasteBefore,
            Action::Edit(BeatEdit::Split) => I::Split,
            Action::DeleteFrames(_) => I::CutFrames,
            Action::RepeatLast => I::RepeatLast,
            Action::Edit(BeatEdit::Delete) => I::CutBeat,
            Action::DeleteSelection => I::CutRange,
            Action::Edit(BeatEdit::WrapRepeat(_)) => I::Repeat,
            Action::Edit(BeatEdit::InsertHold(_)) => I::Hold,
            Action::EscalatingRepeat => I::EscalatingRepeat,
            Action::Ai(AiAction::Generate { variants: 1 }) => I::GenerateAi,
            Action::Insert => I::Insert,
            Action::Sound(SoundAction::Place) => I::PlaceSound,
            Action::GainStep(step) if step > 0 => I::GainUp,
            Action::GainStep(_) => I::GainDown,
            Action::Mute => I::Mute,
            Action::CutawayPicker => I::CutawayPicker,
            Action::TailPicker => I::Tail,
            Action::Bleep { .. } => I::Bleep,
            Action::Framing(FramingAction::EnterCamera) => I::Camera,
            Action::Framing(FramingAction::PunchIn) => I::PunchIn,
            Action::Framing(FramingAction::Creep) => I::Creep,
            Action::Trim => I::Trim,
            Action::SetMark(_) => I::MarkSet,
            Action::JumpMark(_) => I::MarkJump,
            Action::SelectRegister(_) => I::RegisterSelect,
            Action::MacroRecord(_) => I::MacroRecord,
            Action::MacroExecute { .. } => I::MacroExecute,
            Action::Command => I::Command,
            Action::Search => I::Search,
            Action::Help => I::Help,
            Action::SearchStep { forward: true } => I::SearchNext,
            Action::SearchStep { forward: false } => I::SearchPrevious,
            Action::Pane { reverse: false } => I::PaneNext,
            Action::Pane { reverse: true } => I::PanePrevious,
            Action::Escape => I::Escape,
            _ => unreachable!("editor grammar only declares configurable actions"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrefixKind {
    Operator { cut: bool },
    Repeat,
    Mark(MarkPrefix),
    Register,
    MacroRecord,
    MacroExecute,
}

impl PrefixKind {
    fn for_binding(id: BindingId) -> Option<Self> {
        match id {
            BindingId::CutOperator => Some(Self::Operator { cut: true }),
            BindingId::YankOperator => Some(Self::Operator { cut: false }),
            BindingId::RepeatOperator => Some(Self::Repeat),
            BindingId::MarkSet => Some(Self::Mark(MarkPrefix::Set)),
            BindingId::MarkJump => Some(Self::Mark(MarkPrefix::Jump)),
            BindingId::RegisterSelect => Some(Self::Register),
            BindingId::MacroRecord => Some(Self::MacroRecord),
            BindingId::MacroExecute => Some(Self::MacroExecute),
            _ => None,
        }
    }

    pub fn count_refusal(self) -> &'static str {
        match self {
            Self::Operator { .. } => "An operator count must be positive; no action was taken.",
            Self::Repeat => "A Repeat play count must be positive; no edit was made.",
            Self::Mark(_) => "Use mark commands without a count.",
            Self::Register => "Select a register without a count; put the count after its name.",
            Self::MacroRecord => "Record a macro without a count.",
            Self::MacroExecute => "A macro count must be positive; no edit was made.",
        }
    }

    pub fn allows_count(self, count: Option<u32>) -> bool {
        (self == Self::MacroExecute || self.is_operator()) && count != Some(0)
    }

    pub fn is_operator(self) -> bool {
        matches!(self, Self::Operator { .. } | Self::Repeat)
    }

    fn letter_action(self, letter: char) -> Action {
        match self {
            Self::Operator { .. } | Self::Repeat => {
                unreachable!("operators compile selector paths")
            }
            Self::Mark(MarkPrefix::Set) => Action::SetMark(letter),
            Self::Mark(MarkPrefix::Jump) => Action::JumpMark(letter),
            Self::Register => Action::SelectRegister(letter.to_ascii_lowercase()),
            Self::MacroRecord => Action::MacroRecord(letter.to_ascii_lowercase()),
            Self::MacroExecute => Action::MacroExecute {
                register: letter.to_ascii_lowercase(),
                count: 1,
            },
        }
    }
}

type EditorTrie = Trie<Stroke, Rule, PrefixKind>;

pub(super) struct Definition {
    pub id: BindingId,
    pub paths: Vec<Vec<Stroke>>,
    rule: Rule,
    overridden: bool,
}

pub(super) struct Compiled {
    normal: EditorTrie,
    visual: EditorTrie,
    original: EditorTrie,
    definitions: Vec<Definition>,
    pub mode: KeyMode,
}

pub(super) static SHIPPED: LazyLock<Arc<Compiled>> = LazyLock::new(|| {
    Arc::new(Compiled::compile(KeyMode::Logical, Vec::new()).expect("shipped grammar is valid"))
});

pub(super) fn path_label(path: &[Stroke]) -> String {
    let labels: Vec<_> = path.iter().map(|stroke| stroke.label()).collect();
    if labels.iter().all(|label| label.chars().count() == 1) {
        labels.concat()
    } else {
        labels.join(" ")
    }
}

impl Compiled {
    pub fn compile(
        mode: KeyMode,
        overrides: Vec<(BindingId, Vec<Vec<Stroke>>)>,
    ) -> Result<Self, String> {
        keymap_config::reservations::validate()?;
        let mut definitions: Vec<Definition> = Vec::new();
        for binding in shipped(false).into_iter().chain(shipped(true)) {
            let id = binding.value.id();
            let mut path = binding.path;
            if PrefixKind::for_binding(id).is_some() {
                path.pop();
            }
            if mode == KeyMode::Physical {
                for stroke in &mut path {
                    *stroke = physical_default(*stroke);
                }
            }
            if let Some(definition) = definitions
                .iter_mut()
                .find(|definition| definition.id == id)
            {
                if !definition.paths.contains(&path) {
                    definition.paths.push(path);
                }
            } else {
                definitions.push(Definition {
                    id,
                    paths: vec![path],
                    rule: binding.value,
                    overridden: false,
                });
            }
        }
        // Ungroup has a command alias and an optional configured path. No
        // unmodified default is assigned by the product key vocabulary.
        definitions.push(Definition {
            id: BindingId::Ungroup,
            paths: Vec::new(),
            rule: Rule {
                action: Action::Ungroup,
                count: CountPolicy::Refuse("Ungroup once, without a count."),
                short: "ungroup a neutral Sequence",
                repeatable: false,
                interrupt: false,
            },
            overridden: false,
        });
        // The primary labels are the familiar editor paths, with native aliases following.
        for definition in &mut definitions {
            if definition.id == BindingId::First || definition.id == BindingId::Last {
                definition.paths.reverse();
            }
        }
        for (id, paths) in overrides {
            let definition = definitions
                .iter_mut()
                .find(|definition| definition.id == id)
                .expect("known ID");
            if id.fixed() && definition.paths != paths {
                return Err(format!("{} is a fixed native key", id.as_str()));
            }
            definition.paths = paths;
            definition.overridden = true;
        }
        let normal = compile_mode(&definitions, false, RoutingDomain::Edit)?;
        let visual = compile_mode(&definitions, true, RoutingDomain::Edit)?;
        let original = compile_mode(&definitions, false, RoutingDomain::Original)?;
        Ok(Self {
            normal,
            visual,
            original,
            definitions,
            mode,
        })
    }

    pub fn map(&self, selection: EditSelection, domain: RoutingDomain) -> &EditorTrie {
        if domain != RoutingDomain::Edit {
            &self.original
        } else if selection == EditSelection::None {
            &self.normal
        } else {
            &self.visual
        }
    }
    pub fn rule(
        &self,
        path: &[Stroke],
        selection: EditSelection,
        domain: RoutingDomain,
    ) -> Option<&Rule> {
        self.map(selection, domain)
            .resolve(path)?
            .terminal()
            .map(|binding| &binding.value)
    }
    pub fn prefix(
        &self,
        path: &[Stroke],
        selection: EditSelection,
        domain: RoutingDomain,
    ) -> Option<PrefixKind> {
        self.map(selection, domain)
            .resolve(path)?
            .prefix()
            .map(|prefix| prefix.value)
    }
    pub fn has_descendant(
        &self,
        path: &[Stroke],
        selection: EditSelection,
        id: BindingId,
        domain: RoutingDomain,
    ) -> bool {
        if path.is_empty() {
            return false;
        }
        self.definitions.iter().any(|definition| {
            definition.id == id
                && enabled(id, selection != EditSelection::None, domain)
                && definition
                    .paths
                    .iter()
                    .any(|candidate| candidate.len() > path.len() && candidate.starts_with(path))
        })
    }
    pub fn operator_pending(
        &self,
        path: &[Stroke],
        selection: EditSelection,
        domain: RoutingDomain,
    ) -> bool {
        let Some(node) = self.map(selection, domain).resolve(path) else {
            return false;
        };
        let mut pending: Vec<_> = node.children().map(|(_, child)| child).collect();
        while let Some(node) = pending.pop() {
            if node
                .terminal()
                .is_some_and(|binding| matches!(binding.value.action, Action::Operator { .. }))
            {
                return true;
            }
            pending.extend(node.children().map(|(_, child)| child));
        }
        false
    }
    pub fn repeat_pending_scope(
        &self,
        path: &[Stroke],
        selection: EditSelection,
        domain: RoutingDomain,
    ) -> Option<RepeatPendingScope> {
        let node = self.map(selection, domain).resolve(path)?;
        let mut pending: Vec<_> = node.children().map(|(_, child)| child).collect();
        let mut result = None;
        while let Some(node) = pending.pop() {
            if let Some(binding) = node.terminal()
                && let Action::Repeat { selector, .. } = binding.value.action
            {
                let scope = match selector {
                    deadpan_core::SemanticSelector::SelectedBeat => {
                        RepeatPendingScope::SelectedBeat
                    }
                    deadpan_core::SemanticSelector::VisualSelection => {
                        RepeatPendingScope::VisualSelection
                    }
                    deadpan_core::SemanticSelector::Motion { .. } => RepeatPendingScope::Motion,
                    deadpan_core::SemanticSelector::TextObject { .. }
                    | deadpan_core::SemanticSelector::Speech { .. } => {
                        RepeatPendingScope::TextObject
                    }
                };
                match result {
                    Some(previous) if previous != scope => return Some(RepeatPendingScope::Mixed),
                    _ => result = Some(scope),
                }
            }
            pending.extend(node.children().map(|(_, child)| child));
        }
        result
    }
    fn stroke_label(&self, stroke: Stroke) -> String {
        if self.mode == KeyMode::Physical
            && let Stroke::Key(Key::Plus, shift) = stroke
        {
            if shift {
                "Shift+NumpadAdd".into()
            } else {
                "NumpadAdd".into()
            }
        } else {
            stroke.label()
        }
    }
    pub fn path_label(&self, path: &[Stroke]) -> String {
        let labels: Vec<_> = path
            .iter()
            .map(|stroke| self.stroke_label(*stroke))
            .collect();
        if labels.iter().all(|label| label.chars().count() == 1) {
            labels.concat()
        } else {
            labels.join(" ")
        }
    }
    pub fn labels(&self, id: BindingId) -> Vec<String> {
        self.definitions
            .iter()
            .find(|definition| definition.id == id)
            .expect("complete map")
            .paths
            .iter()
            .map(|path| self.path_label(path))
            .collect()
    }
    pub fn hint(
        &self,
        path: &[Stroke],
        selection: EditSelection,
        counts: Counts,
        recording: bool,
        domain: RoutingDomain,
    ) -> Option<String> {
        self.teaching(path, selection, counts, false, recording, domain)
    }
    pub fn next_keys(
        &self,
        path: &[Stroke],
        selection: EditSelection,
        counts: Counts,
        recording: bool,
        domain: RoutingDomain,
    ) -> Option<String> {
        self.teaching(path, selection, counts, true, recording, domain)
    }
    fn teaching(
        &self,
        path: &[Stroke],
        selection: EditSelection,
        counts: Counts,
        compact: bool,
        recording: bool,
        domain: RoutingDomain,
    ) -> Option<String> {
        let node = self.map(selection, domain).resolve(path)?;
        if let Some(prefix) = node.prefix().filter(|prefix| !prefix.value.is_operator()) {
            if prefix.value == PrefixKind::Register {
                return Some(if compact {
                    "a–z / A–Z · \" · Esc".into()
                } else {
                    "a–z / A–Z selects a register · \" selects the unnamed register · Esc cancels"
                        .into()
                });
            }
            if compact {
                return Some("a–z / A–Z · Esc".into());
            }
            let verb = match prefix.value {
                PrefixKind::Operator { .. } | PrefixKind::Repeat => {
                    unreachable!("operators use selector teaching")
                }
                PrefixKind::Mark(MarkPrefix::Set) => "saves this position",
                PrefixKind::Mark(MarkPrefix::Jump) => "jumps to that mark",
                PrefixKind::Register => unreachable!("register teaching handled above"),
                PrefixKind::MacroRecord => "records a semantic macro in that named register",
                PrefixKind::MacroExecute => "executes that named macro",
            };
            let repetitions = if prefix.value == PrefixKind::MacroExecute {
                format!(" {} time(s)", counts.leading.unwrap_or(1))
            } else {
                String::new()
            };
            return Some(format!("a–z / A–Z {verb}{repetitions} · Esc cancels"));
        }
        let mut parts = Vec::new();
        if node
            .prefix()
            .is_some_and(|prefix| prefix.value == PrefixKind::Repeat)
            && counts.leading.is_none()
            && counts.motion.is_none()
        {
            parts.push(
                if compact {
                    "1–9"
                } else {
                    "1–9 starts a motion count"
                }
                .into(),
            );
        }
        let mut refusal = None;
        for (stroke, child) in node.children() {
            let mut pending = vec![child];
            let mut available = false;
            while let Some(candidate) = pending.pop() {
                if let Some(binding) = candidate.terminal() {
                    match binding.value.resolve_counts(counts) {
                        Action::Invalid(message) => {
                            refusal = refusal.or(Some(message));
                        }
                        _ => available = true,
                    }
                }
                pending.extend(candidate.children().map(|(_, next)| next));
            }
            if available {
                let short = if recording
                    && child
                        .prefix()
                        .is_some_and(|prefix| prefix.value == PrefixKind::MacroRecord)
                {
                    "stops macro recording".into()
                } else {
                    child.terminal().map_or_else(
                        || "…".into(),
                        |binding| {
                            if counts.leading.is_some() && binding.value.id() == BindingId::Hold {
                                "inserts the counted pause".into()
                            } else if let Action::Repeat { selector, plays } =
                                binding.value.resolve_counts(counts)
                            {
                                repeat_description(selector, plays)
                            } else {
                                binding.value.short.into()
                            }
                        },
                    )
                };
                parts.push(if compact {
                    self.stroke_label(*stroke)
                } else {
                    format!("{} {short}", self.stroke_label(*stroke))
                });
            }
        }
        if parts.is_empty() {
            return refusal.map(str::to_owned);
        }
        parts.push(if compact { "Esc" } else { "Esc cancels" }.into());
        Some(parts.join(" · "))
    }
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn prefix_paths(&self) -> Vec<Vec<Stroke>> {
        let mut prefixes = Vec::new();
        for trie in [&self.normal, &self.visual, &self.original] {
            for path in branch_paths(trie) {
                if !prefixes.contains(&path) {
                    prefixes.push(path);
                }
            }
        }
        prefixes
    }
}

fn repeat_description(
    selector: deadpan_core::SemanticSelector,
    plays: std::num::NonZeroU32,
) -> String {
    use deadpan_core::{SemanticMotion as M, SemanticSelector as S};
    let selected = match selector {
        S::SelectedBeat => "this whole beat".into(),
        S::VisualSelection => "the selected range".into(),
        S::TextObject {
            object: deadpan_core::SemanticTextObject::InnerGroup,
        } => "group contents".into(),
        S::TextObject {
            object: deadpan_core::SemanticTextObject::AroundGroup,
        } => "the whole group".into(),
        S::Motion {
            motion: M::Scope { end },
        } => if end {
            "to the group end"
        } else {
            "to the group start"
        }
        .into(),
        S::Motion {
            motion: M::Frames { forward, count },
        } => format!(
            "{count} frame(s) {}",
            if forward { "forward" } else { "backward" }
        ),
        S::Motion {
            motion: M::Beats { forward, count },
        } => format!(
            "{count} beat boundary/boundaries {}",
            if forward { "forward" } else { "backward" }
        ),
        S::Motion {
            motion:
                M::Words {
                    forward,
                    count,
                    end,
                },
        } => match (forward, end) {
            (true, true) => format!("to the end of {count} word(s)"),
            (true, false) => format!("{count} word(s) forward"),
            (false, _) => format!("{count} word(s) backward"),
        },
        S::Motion {
            motion: M::Sentences { forward, count },
        } => format!(
            "{count} sentence(s) {}",
            if forward { "forward" } else { "backward" }
        ),
        S::Motion {
            motion: M::Pauses { forward, count },
        } => format!(
            "{count} pause(s) {}",
            if forward { "forward" } else { "backward" }
        ),
        S::Motion {
            motion: M::Shots { forward, count },
        } => format!(
            "{count} shot(s) {}",
            if forward { "forward" } else { "backward" }
        ),
        S::Speech { object } => speech_object_text(object).into(),
    };
    format!("repeats {selected}, {plays} total plays")
}

fn speech_object_text(object: deadpan_core::SpeechObject) -> &'static str {
    match object {
        deadpan_core::SpeechObject::InnerWord => "the word",
        deadpan_core::SpeechObject::AroundWord => "the word with its pauses",
        deadpan_core::SpeechObject::InnerSentence => "the sentence",
        deadpan_core::SpeechObject::AroundSentence => "the sentence with its pauses",
        deadpan_core::SpeechObject::InnerPause => "the pause",
        deadpan_core::SpeechObject::AroundPause => "the pause with its edges",
        deadpan_core::SpeechObject::InnerShot => "the shot",
        deadpan_core::SpeechObject::AroundShot => "the shot with its transitions",
    }
}

fn enabled(id: BindingId, visual: bool, domain: RoutingDomain) -> bool {
    if matches!(
        id,
        BindingId::InnerGroup
            | BindingId::AroundGroup
            | BindingId::InnerWord
            | BindingId::AroundWord
            | BindingId::InnerSentence
            | BindingId::AroundSentence
            | BindingId::InnerPause
            | BindingId::AroundPause
            | BindingId::InnerShot
            | BindingId::AroundShot
    ) {
        return domain == RoutingDomain::Edit && visual;
    }
    if matches!(
        id,
        BindingId::CopyBeat
            | BindingId::CutOperator
            | BindingId::YankOperator
            | BindingId::RepeatOperator
    ) {
        return domain == RoutingDomain::Edit && !visual;
    }
    if id == BindingId::Copy {
        return domain != RoutingDomain::Edit || visual;
    }
    if id == BindingId::RepeatRange {
        return domain == RoutingDomain::Edit && visual;
    }
    if id == BindingId::EscalatingRepeat {
        return domain == RoutingDomain::Edit;
    }
    if id == BindingId::GenerateAi {
        return domain == RoutingDomain::Edit && !visual;
    }
    if matches!(
        id,
        BindingId::Mute | BindingId::CutawayPicker | BindingId::Tail | BindingId::Bleep
    ) {
        return domain == RoutingDomain::Edit;
    }
    !matches!(
        (id, visual),
        (BindingId::CutBeat | BindingId::Repeat, true) | (BindingId::CutRange, false)
    )
}

fn compile_mode(
    definitions: &[Definition],
    visual: bool,
    domain: RoutingDomain,
) -> Result<EditorTrie, String> {
    let mut bindings = Vec::new();
    let mut prefixes = Vec::new();
    for definition in definitions
        .iter()
        .filter(|definition| enabled(definition.id, visual, domain))
    {
        for path in &definition.paths {
            if let Some(kind) = PrefixKind::for_binding(definition.id) {
                prefixes.push(Prefix {
                    path: path.clone(),
                    label: path_label(path),
                    value: kind,
                });
                if kind.is_operator() {
                    for motion in definitions {
                        let selector = match motion.rule.action {
                            Action::SelectObject(object) => {
                                deadpan_core::SemanticSelector::TextObject { object }
                            }
                            Action::SelectSpeech(object) => {
                                deadpan_core::SemanticSelector::Speech { object }
                            }
                            Action::Word { forward, end, .. } => {
                                deadpan_core::SemanticSelector::Motion {
                                    motion: deadpan_core::SemanticMotion::Words {
                                        forward,
                                        count: std::num::NonZeroU32::new(1).unwrap(),
                                        end,
                                    },
                                }
                            }
                            Action::Sentence { forward, .. } => {
                                deadpan_core::SemanticSelector::Motion {
                                    motion: deadpan_core::SemanticMotion::Sentences {
                                        forward,
                                        count: std::num::NonZeroU32::new(1).unwrap(),
                                    },
                                }
                            }
                            Action::Pause { forward, .. } => {
                                deadpan_core::SemanticSelector::Motion {
                                    motion: deadpan_core::SemanticMotion::Pauses {
                                        forward,
                                        count: std::num::NonZeroU32::new(1).unwrap(),
                                    },
                                }
                            }
                            Action::Shot { forward, .. } => {
                                deadpan_core::SemanticSelector::Motion {
                                    motion: deadpan_core::SemanticMotion::Shots {
                                        forward,
                                        count: std::num::NonZeroU32::new(1).unwrap(),
                                    },
                                }
                            }
                            Action::Step { forward, .. } => {
                                deadpan_core::SemanticSelector::Motion {
                                    motion: deadpan_core::SemanticMotion::Frames {
                                        forward,
                                        count: std::num::NonZeroU32::new(1).unwrap(),
                                    },
                                }
                            }
                            Action::Beat { forward, .. } => {
                                deadpan_core::SemanticSelector::Motion {
                                    motion: deadpan_core::SemanticMotion::Beats {
                                        forward,
                                        count: std::num::NonZeroU32::new(1).unwrap(),
                                    },
                                }
                            }
                            Action::First => deadpan_core::SemanticSelector::Motion {
                                motion: deadpan_core::SemanticMotion::Scope { end: false },
                            },
                            Action::Last => deadpan_core::SemanticSelector::Motion {
                                motion: deadpan_core::SemanticMotion::Scope { end: true },
                            },
                            _ => continue,
                        };
                        for suffix in &motion.paths {
                            let mut expanded = path.clone();
                            expanded.extend(suffix);
                            bindings.push(Binding {
                                label: path_label(&expanded),
                                path: expanded,
                                value: Rule {
                                    action: match kind {
                                        PrefixKind::Operator { cut } => {
                                            Action::Operator { cut, selector }
                                        }
                                        PrefixKind::Repeat => Action::Repeat {
                                            selector,
                                            plays: std::num::NonZeroU32::new(2).unwrap(),
                                        },
                                        _ => unreachable!(),
                                    },
                                    count: if kind == PrefixKind::Repeat {
                                        CountPolicy::Repeat
                                    } else {
                                        CountPolicy::Operator
                                    },
                                    short: match kind {
                                        PrefixKind::Operator { cut } => match selector {
                                            deadpan_core::SemanticSelector::Motion { motion } => {
                                                operator_description(cut, motion)
                                            }
                                            deadpan_core::SemanticSelector::TextObject {
                                                object,
                                            } => match (cut, object) {
                                                (
                                                    true,
                                                    deadpan_core::SemanticTextObject::InnerGroup,
                                                ) => "cuts group contents",
                                                (
                                                    false,
                                                    deadpan_core::SemanticTextObject::InnerGroup,
                                                ) => "copies group contents",
                                                (
                                                    true,
                                                    deadpan_core::SemanticTextObject::AroundGroup,
                                                ) => "cuts the whole group",
                                                (
                                                    false,
                                                    deadpan_core::SemanticTextObject::AroundGroup,
                                                ) => "copies the whole group",
                                            },
                                            deadpan_core::SemanticSelector::Speech { object } => {
                                                speech_operator_description(cut, object)
                                            }
                                            _ => unreachable!(),
                                        },
                                        PrefixKind::Repeat => "repeat with a motion",
                                        _ => unreachable!(),
                                    },
                                    repeatable: false,
                                    interrupt: false,
                                },
                            });
                        }
                    }
                    continue;
                }
                for key in LETTERS {
                    for shift in [false, true] {
                        let letter = mark_letter(key, shift).expect("ASCII letter");
                        let mut expanded = path.clone();
                        expanded.push(Stroke::Key(key, shift));
                        let mut rule = definition.rule;
                        rule.action = kind.letter_action(letter);
                        bindings.push(Binding {
                            label: path_label(&expanded),
                            path: expanded,
                            value: rule,
                        });
                    }
                }
                if kind == PrefixKind::Register {
                    let mut expanded = path.clone();
                    expanded.push(Stroke::Key(Key::Quote, true));
                    let mut rule = definition.rule;
                    rule.action = Action::SelectRegister('"');
                    bindings.push(Binding {
                        label: path_label(&expanded),
                        path: expanded,
                        value: rule,
                    });
                }
            } else {
                let mut rule = definition.rule;
                if domain != RoutingDomain::Edit && definition.id == BindingId::CutBeat {
                    rule.action = Action::Edit(BeatEdit::Delete);
                    rule.count = CountPolicy::DeleteBeat;
                }
                if domain != RoutingDomain::Edit && definition.id == BindingId::Repeat {
                    rule.action = Action::Edit(BeatEdit::WrapRepeat(2));
                    rule.count = CountPolicy::LegacyRepeat;
                }
                if definition.id == BindingId::Last {
                    rule.interrupt =
                        definition.overridden || path.as_slice() == [Stroke::Key(Key::G, true)];
                }
                bindings.push(Binding {
                    path: path.clone(),
                    label: path_label(path),
                    value: rule,
                });
            }
        }
    }
    for binding in &bindings {
        for stroke in &binding.path {
            let Stroke::Key(key, shift) = *stroke else {
                continue;
            };
            let modifiers = if shift {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            };
            if keymap_config::reservations::reserved(key, modifiers) {
                return Err("A command path conflicts with a reserved Kestrel position".into());
            }
        }
    }
    // One pending count cannot change meaning by entering another operator
    // family. Shared unannotated ancestors and aliases of one family are fine.
    for (index, prefix) in prefixes.iter().enumerate() {
        if !prefix.value.is_operator() {
            continue;
        }
        for prior in &prefixes[..index] {
            if prior.value.is_operator()
                && prior.value != prefix.value
                && (prior.path.starts_with(&prefix.path) || prefix.path.starts_with(&prior.path))
            {
                return Err(format!(
                    "Operator prefixes {} and {} overlap; use separate paths for Repeat, Cut, and Yank",
                    prior.label, prefix.label,
                ));
            }
        }
    }
    // Bound structural branches as well as explicit argument-family annotations.
    // This also bounds startup conflict auditing and generated pending hints.
    let mut branches: Vec<Vec<Stroke>> = Vec::new();
    for binding in &bindings {
        for end in 1..binding.path.len() {
            let branch = &binding.path[..end];
            if !branches.iter().any(|prior| prior == branch) {
                branches.push(branch.to_vec());
            }
            if branches.len() > super::binding_trie::MAX_PREFIXES {
                return Err("Editor grammar exceeds 128 pending branches".into());
            }
        }
    }
    Trie::compile(bindings, prefixes).map_err(|error| error.to_string())
}

fn speech_operator_description(cut: bool, object: deadpan_core::SpeechObject) -> &'static str {
    use deadpan_core::SpeechObject as O;
    match (cut, object) {
        (true, O::InnerWord) => "cuts the word",
        (false, O::InnerWord) => "copies the word",
        (true, O::AroundWord) => "cuts the word with its pauses",
        (false, O::AroundWord) => "copies the word with its pauses",
        (true, O::InnerSentence) => "cuts the sentence",
        (false, O::InnerSentence) => "copies the sentence",
        (true, O::AroundSentence) => "cuts the sentence with its pauses",
        (false, O::AroundSentence) => "copies the sentence with its pauses",
        (true, O::InnerPause) => "cuts the pause",
        (false, O::InnerPause) => "copies the pause",
        (true, O::AroundPause) => "cuts the pause with its edges",
        (false, O::AroundPause) => "copies the pause with its edges",
        (true, O::InnerShot) => "cuts the shot",
        (false, O::InnerShot) => "copies the shot",
        (true, O::AroundShot) => "cuts the shot with its transitions",
        (false, O::AroundShot) => "copies the shot with its transitions",
    }
}

fn operator_description(cut: bool, motion: deadpan_core::SemanticMotion) -> &'static str {
    use deadpan_core::SemanticMotion as M;
    match (cut, motion) {
        (true, M::Frames { forward: false, .. }) => "cuts backward in frames",
        (true, M::Frames { forward: true, .. }) => "cuts forward in frames",
        (false, M::Frames { forward: false, .. }) => "copies backward in frames",
        (false, M::Frames { forward: true, .. }) => "copies forward in frames",
        (true, M::Beats { forward: false, .. }) => "cuts toward previous beats",
        (true, M::Beats { forward: true, .. }) => "cuts toward next beats",
        (false, M::Beats { forward: false, .. }) => "copies toward previous beats",
        (false, M::Beats { forward: true, .. }) => "copies toward next beats",
        (true, M::Scope { end: false }) => "cuts to group start",
        (true, M::Scope { end: true }) => "cuts to group end",
        (false, M::Scope { end: false }) => "copies to group start",
        (false, M::Scope { end: true }) => "copies to group end",
        (true, M::Words { end: true, .. }) => "cuts to the word end",
        (false, M::Words { end: true, .. }) => "copies to the word end",
        (true, M::Words { forward: true, .. }) => "cuts to the next word",
        (false, M::Words { forward: true, .. }) => "copies to the next word",
        (true, M::Words { forward: false, .. }) => "cuts back to a word start",
        (false, M::Words { forward: false, .. }) => "copies back to a word start",
        (true, M::Sentences { forward: true, .. }) => "cuts to the next sentence",
        (false, M::Sentences { forward: true, .. }) => "copies to the next sentence",
        (true, M::Sentences { forward: false, .. }) => "cuts back to a sentence start",
        (false, M::Sentences { forward: false, .. }) => "copies back to a sentence start",
        (true, M::Pauses { forward: true, .. }) => "cuts to the next pause",
        (false, M::Pauses { forward: true, .. }) => "copies to the next pause",
        (true, M::Pauses { forward: false, .. }) => "cuts back to a pause start",
        (false, M::Pauses { forward: false, .. }) => "copies back to a pause start",
        (true, M::Shots { forward: true, .. }) => "cuts to the next shot",
        (false, M::Shots { forward: true, .. }) => "copies to the next shot",
        (true, M::Shots { forward: false, .. }) => "cuts back to a shot start",
        (false, M::Shots { forward: false, .. }) => "copies back to a shot start",
    }
}

fn physical_default(stroke: Stroke) -> Stroke {
    match stroke {
        Stroke::At => Stroke::Key(Key::Num2, true),
        Stroke::Key(Key::Colon, _) => Stroke::Key(Key::Semicolon, true),
        Stroke::Key(Key::Questionmark, _) => Stroke::Key(Key::Slash, true),
        Stroke::Key(Key::Plus, _) => Stroke::Key(Key::Equals, true),
        _ => stroke,
    }
}

#[cfg(test)]
pub(super) fn map(selection: EditSelection) -> &'static EditorTrie {
    SHIPPED.map(selection, RoutingDomain::Edit)
}
#[cfg(test)]
pub(super) fn rule(path: &[Stroke], selection: EditSelection) -> Option<&'static Rule> {
    SHIPPED.rule(path, selection, RoutingDomain::Edit)
}
#[cfg(any(test, feature = "ui-harness"))]
fn branch_paths(trie: &EditorTrie) -> Vec<Vec<Stroke>> {
    let mut branches = Vec::new();
    let mut pending = vec![Vec::new()];
    while let Some(path) = pending.pop() {
        let node = trie.resolve(&path).expect("path came from this trie");
        if node.children().next().is_some() {
            branches.push(path.clone());
        }
        for (stroke, _) in node.children() {
            let mut child = path.clone();
            child.push(*stroke);
            pending.push(child);
        }
    }
    branches
}

#[cfg(test)]
mod tests;
fn speech_select_text(object: deadpan_core::SpeechObject) -> &'static str {
    match object {
        deadpan_core::SpeechObject::InnerWord => "selects the word",
        deadpan_core::SpeechObject::AroundWord => "selects the word with its pauses",
        deadpan_core::SpeechObject::InnerSentence => "selects the sentence",
        deadpan_core::SpeechObject::AroundSentence => "selects the sentence with its pauses",
        deadpan_core::SpeechObject::InnerPause => "selects the pause",
        deadpan_core::SpeechObject::AroundPause => "selects the pause with its edges",
        deadpan_core::SpeechObject::InnerShot => "selects the shot",
        deadpan_core::SpeechObject::AroundShot => "selects the shot with its transitions",
    }
}

fn shipped(visual: bool) -> Vec<Binding<Stroke, Rule>> {
    let mut bindings = Vec::new();
    let mut add = |path: &[Stroke], action, count, short, repeatable| {
        bindings.push(Binding {
            path: path.to_vec(),
            label: path.iter().map(|stroke| stroke.label()).collect::<String>(),
            value: Rule {
                action,
                count,
                short,
                repeatable,
                interrupt: matches!(
                    action,
                    Action::Playback
                        | Action::Audition
                        | Action::EnterGroup
                        | Action::LeaveGroup
                        | Action::Command
                        | Action::Search
                        | Action::Help
                ),
            },
        });
    };
    use CountPolicy as C;
    let plain = |key| Stroke::Key(key, false);
    for (key, object, short) in [
        (
            Key::I,
            deadpan_core::SemanticTextObject::InnerGroup,
            "selects group contents",
        ),
        (
            Key::A,
            deadpan_core::SemanticTextObject::AroundGroup,
            "selects the whole group",
        ),
    ] {
        add(
            &[plain(key), plain(Key::G)],
            Action::SelectObject(object),
            C::Refuse("Select one group object without a count."),
            short,
            false,
        );
    }
    for (key, forward) in [
        (Key::H, false),
        (Key::ArrowLeft, false),
        (Key::L, true),
        (Key::ArrowRight, true),
    ] {
        add(
            &[plain(key)],
            Action::Step { forward, count: 1 },
            C::Motion,
            "move by frames",
            true,
        );
    }
    for (key, object, short) in [
        (Key::I, Key::W, deadpan_core::SpeechObject::InnerWord),
        (Key::A, Key::W, deadpan_core::SpeechObject::AroundWord),
        (Key::I, Key::S, deadpan_core::SpeechObject::InnerSentence),
        (Key::A, Key::S, deadpan_core::SpeechObject::AroundSentence),
        (Key::I, Key::P, deadpan_core::SpeechObject::InnerPause),
        (Key::A, Key::P, deadpan_core::SpeechObject::AroundPause),
    ]
    .map(|(prefix, key, object)| ([prefix, key], object, speech_select_text(object)))
    {
        add(
            &[plain(key[0]), plain(key[1])],
            Action::SelectSpeech(object),
            C::Refuse("Select one word, sentence, pause or shot object without a count."),
            short,
            false,
        );
    }
    for (prefix, object) in [
        (Key::I, deadpan_core::SpeechObject::InnerShot),
        (Key::A, deadpan_core::SpeechObject::AroundShot),
    ] {
        add(
            &[plain(prefix), Stroke::Key(Key::S, true)],
            Action::SelectSpeech(object),
            C::Refuse("Select one word, sentence, pause or shot object without a count."),
            speech_select_text(object),
            false,
        );
    }
    for (stroke, action, short) in [
        (
            plain(Key::W),
            Action::Word {
                forward: true,
                end: false,
                count: 1,
            },
            "next word",
        ),
        (
            plain(Key::B),
            Action::Word {
                forward: false,
                end: false,
                count: 1,
            },
            "previous word",
        ),
        (
            plain(Key::E),
            Action::Word {
                forward: true,
                end: true,
                count: 1,
            },
            "word end",
        ),
        (
            Stroke::Key(Key::W, true),
            Action::Sentence {
                forward: true,
                count: 1,
            },
            "next sentence",
        ),
        (
            Stroke::Key(Key::B, true),
            Action::Sentence {
                forward: false,
                count: 1,
            },
            "previous sentence",
        ),
    ] {
        add(&[stroke], action, C::Motion, short, true);
    }
    for (key, forward, short) in [
        (Key::CloseBracket, true, "next pause"),
        (Key::OpenBracket, false, "previous pause"),
    ] {
        add(
            &[plain(key), plain(Key::P)],
            Action::Pause { forward, count: 1 },
            C::Motion,
            short,
            true,
        );
    }
    for (key, forward, short) in [
        (Key::CloseBracket, true, "next shot"),
        (Key::OpenBracket, false, "previous shot"),
    ] {
        add(
            &[plain(key), plain(Key::S)],
            Action::Shot { forward, count: 1 },
            C::Motion,
            short,
            true,
        );
    }
    // Repeat plays: All plays, then play 1..N of the nearest open Repeat.
    // Navigation only; operators never compose with it.
    for (key, forward, short) in [
        (Key::CloseBracket, true, "next Repeat play"),
        (Key::OpenBracket, false, "previous Repeat play"),
    ] {
        add(
            &[plain(key), plain(Key::R)],
            Action::Play { forward, count: 1 },
            C::Motion,
            short,
            true,
        );
    }
    for (key, forward) in [
        (Key::J, true),
        (Key::ArrowDown, true),
        (Key::K, false),
        (Key::ArrowUp, false),
    ] {
        add(
            &[plain(key)],
            Action::Beat { forward, count: 1 },
            C::Motion,
            "select beat",
            true,
        );
    }
    for (key, shift, action, short) in [
        (Key::Home, false, Action::First, "go to the start"),
        (Key::End, false, Action::Last, "go to the end"),
        (Key::G, true, Action::Last, "go to the end"),
        (Key::U, false, Action::Undo, "undo"),
        (Key::Space, false, Action::Playback, "play / pause"),
        (Key::Space, true, Action::Audition, "loop selection"),
        (Key::Enter, false, Action::EnterGroup, "enter group"),
        (Key::Backspace, false, Action::LeaveGroup, "leave group"),
        (Key::Escape, false, Action::Escape, "cancel"),
        (
            Key::Tab,
            false,
            Action::Pane { reverse: false },
            "next pane",
        ),
        (
            Key::Tab,
            true,
            Action::Pane { reverse: true },
            "previous pane",
        ),
        (Key::Colon, false, Action::Command, "command"),
        (Key::Slash, false, Action::Search, "search"),
        (
            Key::N,
            false,
            Action::SearchStep { forward: true },
            "next match",
        ),
        (
            Key::N,
            true,
            Action::SearchStep { forward: false },
            "previous match",
        ),
        (Key::Questionmark, false, Action::Help, "keys"),
    ] {
        add(&[Stroke::Key(key, shift)], action, C::Ignore, short, false);
    }
    add(
        &[plain(Key::G), plain(Key::G)],
        Action::First,
        C::Ignore,
        "goes to the start",
        false,
    );
    add(
        &[plain(Key::R), plain(Key::R)],
        Action::Repeat {
            selector: deadpan_core::SemanticSelector::SelectedBeat,
            plays: std::num::NonZeroU32::new(2).unwrap(),
        },
        C::Repeat,
        "completes the Repeat",
        false,
    );
    if visual {
        add(
            &[plain(Key::R)],
            Action::Repeat {
                selector: deadpan_core::SemanticSelector::VisualSelection,
                plays: std::num::NonZeroU32::new(2).unwrap(),
            },
            C::Repeat,
            "repeat selected range",
            false,
        );
        add(
            &[plain(Key::D)],
            Action::DeleteSelection,
            C::Refuse("Cut the selected range once, without a count."),
            "cut selected range",
            false,
        );
    } else {
        // Placeholder suffix is replaced by the configured motion paths.
        add(
            &[plain(Key::R), plain(Key::H)],
            Action::Repeat {
                selector: deadpan_core::SemanticSelector::Motion {
                    motion: deadpan_core::SemanticMotion::Frames {
                        forward: false,
                        count: std::num::NonZeroU32::new(1).unwrap(),
                    },
                },
                plays: std::num::NonZeroU32::new(2).unwrap(),
            },
            C::Repeat,
            "repeat with a motion",
            false,
        );
        add(
            &[plain(Key::D), plain(Key::D)],
            Action::Operator {
                cut: true,
                selector: deadpan_core::SemanticSelector::SelectedBeat,
            },
            C::Operator,
            "cuts this whole beat",
            false,
        );
        add(
            &[plain(Key::Y), plain(Key::Y)],
            Action::Operator {
                cut: false,
                selector: deadpan_core::SemanticSelector::SelectedBeat,
            },
            C::Operator,
            "copies this whole beat",
            false,
        );
        for (key, cut) in [(Key::D, true), (Key::Y, false)] {
            // The last placeholder is removed while collecting definitions;
            // compile_mode composes each operator prefix with configured motions.
            add(
                &[plain(key), plain(Key::H)],
                Action::Operator {
                    cut,
                    selector: deadpan_core::SemanticSelector::Motion {
                        motion: deadpan_core::SemanticMotion::Frames {
                            forward: false,
                            count: std::num::NonZeroU32::new(1).unwrap(),
                        },
                    },
                },
                C::Operator,
                if cut {
                    "cut with a motion"
                } else {
                    "copy with a motion"
                },
                false,
            );
        }
    }
    add(
        &[plain(Key::X)],
        Action::DeleteFrames(1),
        C::Frames,
        "cut frames",
        false,
    );
    add(
        &[plain(Key::Period)],
        Action::RepeatLast,
        C::Refuse(
            "Repeat the last edit once, without a count; its requested selector and parameters are retained.",
        ),
        "repeat the last cut, Repeat or group edit at the current Visual range, beat or motion",
        false,
    );
    for (key, action, short) in [
        (Key::V, Action::VisualMoment, "select time"),
        (Key::Y, Action::CopyMoment, "copy"),
        (Key::P, Action::PasteMoment { before: false }, "paste after"),
    ] {
        add(
            &[plain(key)],
            action,
            C::Refuse(
                "Use selection, copy and paste without a count. Counted motion moves the range boundary.",
            ),
            short,
            false,
        );
    }
    add(
        &[Stroke::Key(Key::P, true)],
        Action::PasteMoment { before: true },
        C::Refuse("Paste once, without a count or pending command."),
        "paste before",
        false,
    );
    add(
        &[plain(Key::S)],
        Action::Edit(BeatEdit::Split),
        C::Refuse("Split uses the current boundary. Move with a count first."),
        "split",
        false,
    );
    for (key, step) in [(Key::Plus, 3000), (Key::Minus, -3000)] {
        add(
            &[plain(key)],
            Action::GainStep(step),
            C::Gain,
            "change gain",
            false,
        );
    }
    for (key, action, count, short) in [
        (
            Key::G,
            Action::Group,
            C::Refuse("Name one group, without a count."),
            "name a group",
        ),
        (
            Key::I,
            Action::Insert,
            C::Refuse("Reuse inserts once, without a count."),
            "reuse Original",
        ),
        (
            Key::S,
            Action::Sound(SoundAction::Place),
            C::Refuse("Place one sound, without a count."),
            "place sound",
        ),
        (
            Key::H,
            Action::Edit(BeatEdit::InsertHold(duration::DurationInput::half_seconds(
                1,
            ))),
            C::Hold,
            "pause",
        ),
        (
            Key::E,
            Action::EscalatingRepeat,
            C::Refuse(
                "Escalate once, without a count; :repeat N gain-step= zoom-step= adjusts it.",
            ),
            "escalating repeat",
        ),
        (
            Key::A,
            Action::Ai(AiAction::Generate { variants: 1 }),
            C::Refuse("Generate AI pictures once, without a count."),
            "AI pictures",
        ),
        (
            Key::F,
            Action::Framing(FramingAction::EnterCamera),
            C::Refuse("Use framing actions without a count."),
            "Camera",
        ),
        (
            Key::V,
            Action::Trim,
            C::Refuse("Open Trim once, without a count."),
            "Trim",
        ),
        (
            Key::Z,
            Action::Framing(FramingAction::PunchIn),
            C::Refuse("Use framing actions without a count."),
            "punch in",
        ),
        (
            Key::C,
            Action::Framing(FramingAction::Creep),
            C::Refuse("Use framing actions without a count."),
            "creep",
        ),
        (
            Key::M,
            Action::Mute,
            C::Refuse("Mute once, without a count."),
            "mute",
        ),
        (
            Key::R,
            Action::CutawayPicker,
            C::Refuse("Pick one cutaway register, without a count."),
            "reaction cutaway",
        ),
        (
            Key::T,
            Action::TailPicker,
            C::Refuse("Add one tail, without a count; :tail sets its length."),
            "reverb tail",
        ),
        (
            Key::B,
            Action::Bleep {
                frequency_hz: deadpan_core::DEFAULT_BLEEP_FREQUENCY_HZ,
                level_millidecibels: deadpan_core::DEFAULT_BLEEP_LEVEL_MILLIDECIBELS,
            },
            C::Refuse("Bleep once, without a count."),
            "bleep",
        ),
    ] {
        add(
            &[plain(Key::Comma), plain(key)],
            action,
            count,
            short,
            false,
        );
    }
    for (key, kind) in [(Key::M, MarkPrefix::Set), (Key::Quote, MarkPrefix::Jump)] {
        for letter_key in LETTERS {
            for shift in [false, true] {
                let letter = mark_letter(letter_key, shift).expect("declared letter key");
                let action = match kind {
                    MarkPrefix::Set => Action::SetMark(letter),
                    MarkPrefix::Jump => Action::JumpMark(letter),
                };
                add(
                    &[plain(key), Stroke::Key(letter_key, shift)],
                    action,
                    C::Refuse("Use mark commands without a count."),
                    "mark",
                    false,
                );
            }
        }
    }

    add(
        &[Stroke::Key(Key::Quote, true), plain(Key::A)],
        Action::SelectRegister('a'),
        C::Refuse(PrefixKind::Register.count_refusal()),
        "select register",
        false,
    );

    add(
        &[plain(Key::Q), plain(Key::A)],
        Action::MacroRecord('a'),
        C::Refuse(PrefixKind::MacroRecord.count_refusal()),
        "record semantic macro",
        false,
    );
    add(
        &[Stroke::At, plain(Key::A)],
        Action::MacroExecute {
            register: 'a',
            count: 1,
        },
        C::Macro,
        "execute semantic macro",
        false,
    );

    bindings
}
const LETTERS: [Key; 26] = [
    Key::A,
    Key::B,
    Key::C,
    Key::D,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::I,
    Key::J,
    Key::K,
    Key::L,
    Key::M,
    Key::N,
    Key::O,
    Key::P,
    Key::Q,
    Key::R,
    Key::S,
    Key::T,
    Key::U,
    Key::V,
    Key::W,
    Key::X,
    Key::Y,
    Key::Z,
];
