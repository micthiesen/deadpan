//! Closed semantic intent. No input events, absolute targets or process hooks.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::{EditError, EditErrorCode, FrameCut, RegisterName, SemanticTextObject};

pub const MAX_SEMANTIC_PROGRAM_INSTRUCTIONS: usize = 1024;
pub const MAX_SEMANTIC_PROGRAM_BYTES: usize = 128 * 1024;
pub const MAX_SEMANTIC_INSTRUCTION_FUEL: usize = 4096;
pub const MAX_SEMANTIC_CALL_DEPTH: usize = 16;
/// Bound on the gaps one `SetRepeat` instruction authors.
pub const MAX_SEMANTIC_REPEAT_GAPS: usize = 64;

/// A relative destination in the current ordinary Sequence. Beat motions use
/// the explicit selected child when present, independently of the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticMotion {
    Frames {
        forward: bool,
        count: NonZeroU32,
    },
    Beats {
        forward: bool,
        count: NonZeroU32,
    },
    Scope {
        end: bool,
    },
    /// `w`, `b` and `e`: recognized word starts, or ends when `end` is set.
    /// Backward word ends are not part of the vocabulary.
    Words {
        forward: bool,
        count: NonZeroU32,
        end: bool,
    },
    /// `W` and `B`: recognized sentence starts.
    Sentences {
        forward: bool,
        count: NonZeroU32,
    },
    /// `]p` and `[p`: detected pause starts.
    Pauses {
        forward: bool,
        count: NonZeroU32,
    },
    /// `]s` and `[s`: detected shot starts.
    Shots {
        forward: bool,
        count: NonZeroU32,
    },
}

impl SemanticMotion {
    pub const fn uses_speech(self) -> bool {
        matches!(
            self,
            Self::Words { .. } | Self::Sentences { .. } | Self::Pauses { .. } | Self::Shots { .. }
        )
    }
}

/// A pause length, resolved once against the project frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "snake_case", deny_unknown_fields)]
pub enum PauseLength {
    Frames {
        frames: NonZeroU32,
    },
    /// Rounded once to the nearest project frame, ties to even.
    Milliseconds {
        milliseconds: NonZeroU32,
    },
}

impl PauseLength {
    pub fn resolve(self, rate: crate::FrameRate) -> Result<crate::FrameDuration, EditError> {
        let frames = match self {
            Self::Frames { frames } => i64::from(frames.get()),
            Self::Milliseconds { milliseconds } => {
                let numerator = i128::from(milliseconds.get()) * i128::from(rate.numerator());
                let denominator = 1000 * i128::from(rate.denominator());
                let (quotient, remainder) = (numerator / denominator, numerator % denominator);
                let rounded = match (remainder * 2).cmp(&denominator) {
                    std::cmp::Ordering::Less => quotient,
                    std::cmp::Ordering::Greater => quotient + 1,
                    std::cmp::Ordering::Equal => quotient + (quotient & 1),
                };
                i64::try_from(rounded).map_err(|_| {
                    EditError::new(EditErrorCode::TimingOverflow, "pause length overflows")
                })?
            }
        };
        if frames == 0 {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "the pause resolves to zero project frames",
            ));
        }
        crate::FrameDuration::new(frames)
            .map_err(|error| EditError::from(crate::DocumentError::from(error)))
    }
}

/// Select copied or deleted content without moving the context. Motion ranges
/// run from the entry cursor to the same destination as ordinary navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticSelector {
    #[serde(deserialize_with = "deserialize_empty")]
    SelectedBeat,
    #[serde(deserialize_with = "deserialize_empty")]
    VisualSelection,
    Motion {
        motion: SemanticMotion,
    },
    TextObject {
        object: SemanticTextObject,
    },
    /// `iw`, `aw`, `is`, `as`, `ip` and `ap` against analysed speech at the
    /// cursor.
    Speech {
        object: super::SpeechObject,
    },
}

impl SemanticSelector {
    pub const fn uses_speech(self) -> bool {
        match self {
            Self::Motion { motion } => motion.uses_speech(),
            Self::Speech { .. } => true,
            Self::SelectedBeat | Self::VisualSelection | Self::TextObject { .. } => false,
        }
    }
}

/// The supported macro vocabulary. Selectors resolve against each preceding
/// staged document and its checked ordinary Sequence navigation context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticInstruction {
    MoveFrames {
        forward: bool,
        count: NonZeroU32,
    },
    MoveBeats {
        forward: bool,
        count: NonZeroU32,
    },
    MoveScope {
        end: bool,
    },
    MoveWords {
        forward: bool,
        count: NonZeroU32,
        end: bool,
    },
    MoveSentences {
        forward: bool,
        count: NonZeroU32,
    },
    MovePauses {
        forward: bool,
        count: NonZeroU32,
    },
    MoveShots {
        forward: bool,
        count: NonZeroU32,
    },
    /// Select a word, sentence or pause object as an extending Visual time
    /// range.
    SelectSpeech {
        object: super::SpeechObject,
    },
    SelectObject {
        object: SemanticTextObject,
    },
    #[serde(deserialize_with = "deserialize_empty")]
    BeginSelection,
    #[serde(deserialize_with = "deserialize_empty")]
    FinishSelection,
    #[serde(deserialize_with = "deserialize_empty")]
    ClearSelection,
    Yank {
        selector: SemanticSelector,
        register: RegisterName,
    },
    Cut {
        selector: SemanticSelector,
        register: RegisterName,
    },
    Group {
        selector: SemanticSelector,
        label: String,
    },
    #[serde(deserialize_with = "deserialize_empty")]
    Ungroup,
    Repeat {
        selector: SemanticSelector,
        plays: NonZeroU32,
        /// Escalate the new Repeat's plays in the same transaction (`,e`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        escalation: Option<crate::RepeatEscalation>,
    },
    /// Apply a built-in gag recipe (`:gag`), pinned by version and parameters.
    Gag {
        recipe: super::GagRecipe,
    },
    /// Change the exposed parameters of the selected inserted gag group
    /// (`:gag-set`, specification §8.4): each changed parameter edits the
    /// ordinary part it made, and the group's pinned label is rewritten to the
    /// new recipe, as one transaction. The group must still have the shape the
    /// recipe expanded to; otherwise its parts are edited directly.
    SetGag {
        recipe: super::GagRecipe,
        /// The parameters to take from `recipe`; empty takes all of them.
        /// `.` and macros then change only these on another gag of the same
        /// recipe.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        parameters: Vec<super::GagParameter>,
    },
    /// Insert a silent freeze pause at the cursor (`,h`, `:hold`). The host
    /// resolves the frozen picture from the staged document. `black` inserts
    /// the project background instead (`:hold … video=black`), with no
    /// sampled picture.
    InsertPause {
        length: PauseLength,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        black: bool,
    },
    /// Bleep the Visual selection (`,b`, `:bleep`): cut it into `register`
    /// and put back a pause of exactly its length that plays the same
    /// pictures forward with a tone in place of their sound.
    Bleep {
        register: RegisterName,
        #[serde(default = "default_bleep_frequency")]
        frequency_hz: u32,
        #[serde(default = "default_bleep_level")]
        level: crate::GainDb,
    },
    /// Lift the Visual selection (`:lift`): cut it into `register` as `d`
    /// does and put back a silent black pause of exactly its length, so later
    /// content keeps its time.
    Lift {
        register: RegisterName,
    },
    /// Insert at the cursor a pause that plays the `length` before it
    /// backwards, picture and sound (`:reverse`, a reverse hiccup). With
    /// `bounce` the picture at the cursor is not shown twice: the pause
    /// starts one picture earlier and is one frame shorter (`:ping-pong`).
    InsertReverse {
        length: PauseLength,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        bounce: bool,
    },
    /// A hanging effect tail (`,t`, `:tail`). On a selected direct-child Hold
    /// its sound becomes the tail of the audio heard just before it, ringing
    /// for `length` (the whole Hold when omitted). Otherwise a freeze pause of
    /// `length` is inserted at the cursor with that tail.
    Tail {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length: Option<PauseLength>,
        #[serde(default, skip_serializing_if = "crate::TailEffect::is_reverb")]
        effect: crate::TailEffect,
    },
    /// Replace the selected direct child's framing (`,c`, `,z`).
    SetFraming {
        framing: Option<Box<crate::Framing>>,
    },
    /// `+`/`-`, `:gain` and `:saturate` on the selected direct child: change
    /// its clip-gain trim or its saturation stage, keeping every other
    /// treatment and the authored stage order.
    SetAudio {
        change: AudioChange,
    },
    /// A split edit at the cursor's seam between two adjacent source beats
    /// (`:jcut 6f`, `:lcut 6f`). The sound's cut moves `length` earlier (J:
    /// the next beat's sound starts under the current picture) or later (L:
    /// the current beat's sound continues under the next picture) while the
    /// picture still changes at the cursor: one Roll of the seam plus a
    /// cutaway that keeps the pictures, as one transaction. Both beats need
    /// enough source handle; the planner refuses a shortened edit.
    SplitEdit {
        kind: SplitEditKind,
        length: PauseLength,
    },
    /// A role-only delete of the Visual time range inside one direct child
    /// (`:delete role=audio|video`, specification §6.5): audio becomes a
    /// silent mute range of that beat, or video becomes a removed-picture
    /// cutaway exposing the background. Neither moves later content.
    DeleteRole {
        role: crate::MediaRole,
    },
    /// An audio-only or video-only repeat of the Visual time range inside
    /// one beat (`:repeat 3 role=audio`, specification §6.5). Audio: the
    /// range's own sound plays `plays` times in total as sample-timed root
    /// sound events, muting the host's sound under the repeats. Video: a
    /// looping cutaway repeats its pictures while the host's sound continues.
    /// No picture time is inserted; repeats past the beat refuse unless
    /// `trim` cuts them at its end.
    RoleRepeat {
        role: crate::MediaRole,
        plays: NonZeroU32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        trim: bool,
    },
    /// Set the selected direct-child Repeat's total count. Visual selection is
    /// incompatible with this node parameter edit, including an empty range.
    SetRepeatPlays {
        plays: NonZeroU32,
    },
    /// `:repeat [N] gap=… gain-step=… zoom-step=…` on the selected direct
    /// child, as one transaction. A Repeat changes its total plays, gaps and
    /// escalation; any other beat is first wrapped in a Repeat of `plays`.
    /// `gaps` gives the default silent freeze gap and then the gaps after the
    /// second and later plays as independent Holds. Given gaps are the complete
    /// set: earlier independent gap Holds end, and an empty list removes every
    /// gap. A step-free escalation removes the escalation. Omitted parts are kept.
    SetRepeat {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plays: Option<NonZeroU32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gaps: Option<Vec<PauseLength>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        escalation: Option<crate::RepeatEscalation>,
    },
    /// Give the selected direct-child Hold room tone from the exact audio of
    /// the Original moment held in `register`.
    SetRoomTone {
        register: RegisterName,
    },
    /// Show the Original moment held in `register` over the whole selected
    /// direct child while its own sound continues (`:cutaway register=`).
    /// The cutaway belongs to the beat's Source or Hold.
    SetCutaway {
        register: RegisterName,
        #[serde(default, skip_serializing_if = "is_hold_fit")]
        fit: crate::CutawayFit,
    },
    /// Caption the whole selected direct child with one line of text
    /// (`:caption`), starting `delay` after its first frame and, inside a
    /// Repeat, from play `reveal` on. The caption belongs to the beat's
    /// Source or Hold.
    SetCaption {
        text: String,
        #[serde(default, skip_serializing_if = "crate::CaptionPlacement::is_bottom")]
        placement: crate::CaptionPlacement,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delay: Option<PauseLength>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reveal: Option<NonZeroU32>,
    },
    /// `:audio-lag +80ms`: the selected direct child's Source sound plays
    /// this many 48 kHz samples later (negative: earlier) than its picture,
    /// as its explicit link offset.
    SetAudioLag {
        offset: crate::AudioSample,
    },
    /// `:edge hard|auto [side]`: the selected direct child's sound edge
    /// policy, without changing picture or timing. `plays` sets every play's
    /// start and end inside a selected Repeat (its loop seams) and `gaps` its
    /// gap edges.
    SetAudioEdges {
        side: EdgeSide,
        policy: crate::AudioEdgePolicy,
    },
    /// Change the selected direct child's speed (`:speed`, `:retime`). The
    /// speed divides the beat's exact input, rounded once to whole frames,
    /// ties to even. An ordinary Retime is updated in place unless `wrap`;
    /// any other beat is wrapped in a new Retime, which becomes selected.
    Retime {
        speed: crate::ExactRatio,
        #[serde(default = "preserve_pitch", skip_serializing_if = "is_preserve_pitch")]
        pitch: crate::PitchPolicy,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        wrap: bool,
    },
    /// A fixed pitch shift of the selected direct child at its current speed
    /// (`:pitch +3st`). An ordinary Retime keeps its exact speed and gets the
    /// shift; any other beat is wrapped in a unity-speed Retime with it. Zero
    /// restores plain pitch preservation on an existing Retime.
    Pitch {
        semitones: i8,
    },
    /// Change the selected direct-child pause's duration (`:hold-duration`),
    /// keeping its picture, sound policy and framing.
    SetHoldDuration {
        length: PauseLength,
    },
    CutFrames {
        operation: FrameCut,
        register: RegisterName,
    },
    YankBeat {
        register: RegisterName,
    },
    YankSelection {
        register: RegisterName,
    },
    CutSelection {
        register: RegisterName,
    },
    ReplaceSelection {
        register: RegisterName,
    },
    /// Explicit seam insertion for absent or Time Visual selection. An Object
    /// Visual replaces its exact owned target; ReplaceSelection handles both
    /// Visual kinds explicitly, including empty group contents at slot zero.
    Paste {
        register: RegisterName,
        before: bool,
    },
    Call {
        register: RegisterName,
        count: NonZeroU32,
    },
}

impl SemanticInstruction {
    /// True when resolving this instruction needs recognized speech.
    pub fn uses_speech(&self) -> bool {
        match self {
            Self::MoveWords { .. }
            | Self::MoveSentences { .. }
            | Self::MovePauses { .. }
            | Self::MoveShots { .. }
            | Self::SelectSpeech { .. } => true,
            Self::Yank { selector, .. }
            | Self::Cut { selector, .. }
            | Self::Group { selector, .. }
            | Self::Repeat { selector, .. } => selector.uses_speech(),
            _ => false,
        }
    }

    fn backward_word_end(&self) -> bool {
        let backward_end = |motion: &SemanticMotion| {
            matches!(
                motion,
                SemanticMotion::Words {
                    forward: false,
                    end: true,
                    ..
                }
            )
        };
        match self {
            Self::MoveWords {
                forward: false,
                end: true,
                ..
            } => true,
            Self::Yank { selector, .. }
            | Self::Cut { selector, .. }
            | Self::Group { selector, .. }
            | Self::Repeat { selector, .. } => {
                matches!(selector, SemanticSelector::Motion { motion } if backward_end(motion))
            }
            _ => false,
        }
    }
}

/// Which sound edges `:edge` sets on the selected beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSide {
    Start,
    End,
    Both,
    /// Each play's start and end inside a selected Repeat: the loop seams.
    Plays,
    /// A selected Repeat's gap starts and ends.
    Gaps,
}

/// Which role leads across a split edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitEditKind {
    /// Premature sound: the incoming sound starts before the picture changes.
    J,
    /// Lingering sound: the outgoing sound continues over the next picture.
    L,
}

impl SplitEditKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::J => "J-cut",
            Self::L => "L-cut",
        }
    }
}

/// One recorded change to a beat's audio treatments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioChange {
    /// Set the clip-gain trim (`:gain 6dB`).
    Trim { gain: crate::GainDb },
    /// Change the clip-gain trim by a signed amount (`+`, `-`, `:gain +=3dB`).
    Step { millidecibels: i32 },
    /// `+`/`-` over a Visual range inside one beat: change that range's
    /// constant step envelope by a signed amount. It needs a Visual time
    /// range, and a whole-beat step refuses one, so `.` never turns one into
    /// the other.
    RangeStep { millidecibels: i32 },
    /// Set the saturation drive, or remove the stage (`:saturate`).
    Saturation {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        drive: Option<crate::GainDb>,
    },
    /// Mute or unmute the whole beat (`,m` without a range, `:gain-mute`),
    /// keeping its trim, envelopes and mute ranges.
    Mute { muted: bool },
}

impl AudioChange {
    /// The beat's treatments after this change.
    pub fn apply(
        self,
        current: &crate::AudioTreatments,
    ) -> Result<crate::AudioTreatments, crate::GainError> {
        let clip = || current.clip_gain().cloned().unwrap_or_default();
        match self {
            Self::Trim { gain } => current.with_clip_gain_or_none(clip().with_trim(gain)),
            Self::Step { millidecibels } => {
                current.with_clip_gain_or_none(clip().adjust_trim(millidecibels)?)
            }
            Self::RangeStep { .. } => Err(crate::GainError::TimeRange),
            Self::Saturation { drive } => {
                current.with_saturation(drive.map(crate::Saturation::new).transpose()?)
            }
            Self::Mute { muted } => {
                let clip = clip();
                current.with_clip_gain_or_none(crate::ClipGain::new(
                    clip.trim(),
                    muted,
                    clip.envelopes().to_vec(),
                    clip.mute_ranges().to_vec(),
                )?)
            }
        }
    }
}

/// The classic censor bleep: 1 kHz.
pub const DEFAULT_BLEEP_FREQUENCY_HZ: u32 = 1_000;
/// -10 dBFS: clearly heard, below speech peaks.
pub const DEFAULT_BLEEP_LEVEL_MILLIDECIBELS: i32 = -10_000;

fn default_bleep_frequency() -> u32 {
    DEFAULT_BLEEP_FREQUENCY_HZ
}

fn default_bleep_level() -> crate::GainDb {
    crate::GainDb::new(DEFAULT_BLEEP_LEVEL_MILLIDECIBELS).expect("constant level")
}

fn preserve_pitch() -> crate::PitchPolicy {
    crate::PitchPolicy::Preserve
}

fn is_preserve_pitch(pitch: &crate::PitchPolicy) -> bool {
    *pitch == crate::PitchPolicy::Preserve
}

fn is_hold_fit(fit: &crate::CutawayFit) -> bool {
    *fit == crate::CutawayFit::Hold
}

// Internally tagged unit variants otherwise ignore unknown fields in Serde.
pub(super) fn deserialize_empty<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<(), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}

    Empty::deserialize(deserializer).map(|_| ())
}

/// Validated nonempty macro body. The immutable private body makes every
/// register-held program safe to expand without repeatedly scanning its wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProgram {
    instructions: Vec<SemanticInstruction>,
}

impl SemanticProgram {
    pub fn new(instructions: Vec<SemanticInstruction>) -> Result<Self, EditError> {
        let result = Self { instructions };
        result.validate()?;
        Ok(result)
    }

    pub fn instructions(&self) -> &[SemanticInstruction] {
        &self.instructions
    }

    pub fn validate(&self) -> Result<(), EditError> {
        if self.instructions.is_empty()
            || self.instructions.len() > MAX_SEMANTIC_PROGRAM_INSTRUCTIONS
        {
            return Err(EditError::new(
                EditErrorCode::LimitExceeded,
                "a semantic program needs 1..=1024 instructions",
            ));
        }
        for instruction in &self.instructions {
            if instruction.backward_word_end() {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "word-end motions move forward only",
                ));
            }
            if let SemanticInstruction::Group { label, .. } = instruction {
                crate::validate_group_label(label)?;
            }
            if let SemanticInstruction::SetCaption { text, .. } = instruction {
                crate::validate_caption_text(text).map_err(EditError::from)?;
            }
            if let SemanticInstruction::SetRepeat {
                plays,
                gaps,
                escalation,
            } = instruction
            {
                if plays.is_none() && gaps.is_none() && escalation.is_none() {
                    return Err(EditError::new(
                        EditErrorCode::InvalidCommand,
                        "a Repeat change needs plays, gaps or escalation",
                    ));
                }
                if gaps
                    .as_ref()
                    .is_some_and(|gaps| gaps.len() > MAX_SEMANTIC_REPEAT_GAPS)
                {
                    return Err(EditError::new(
                        EditErrorCode::LimitExceeded,
                        format!(
                            "one instruction sets at most {MAX_SEMANTIC_REPEAT_GAPS} Repeat gaps"
                        ),
                    ));
                }
            }
            if let SemanticInstruction::Retime { speed, pitch, .. } = instruction {
                if !speed.compare_integer(0).is_gt() {
                    return Err(EditError::new(
                        EditErrorCode::InvalidCommand,
                        "a retime speed must be positive",
                    ));
                }
                if let crate::PitchPolicy::Shift { semitones } = pitch {
                    crate::PitchPolicy::shifted(*semitones).map_err(EditError::from)?;
                }
            }
            if let SemanticInstruction::Pitch { semitones } = instruction {
                crate::PitchPolicy::shifted(*semitones).map_err(EditError::from)?;
            }
            if let SemanticInstruction::Call { register, .. } = instruction
                && *register == RegisterName::unnamed()
            {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "a macro call requires a named register a-z",
                ));
            }
        }
        crate::compound::wire::size(self, MAX_SEMANTIC_PROGRAM_BYTES)?;
        Ok(())
    }
}

impl<'de> Deserialize<'de> for SemanticProgram {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            instructions: Vec<SemanticInstruction>,
        }
        // Bound decoded bytes, depth, duplicate fields and the body array before
        // internally tagged instructions allocate their typed payloads. Hosts
        // still bound raw envelopes, including whitespace and escaped strings.
        let value = crate::compound::wire::read_bounded(deserializer, MAX_SEMANTIC_PROGRAM_BYTES)?;
        let wire = Wire::deserialize(value).map_err(serde::de::Error::custom)?;
        Self::new(wire.instructions).map_err(serde::de::Error::custom)
    }
}
