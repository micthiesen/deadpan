use eframe::egui::{Key, Modifiers};

#[cfg(test)]
mod ai_tests;
mod binding_trie;
pub mod camera;
pub mod caption;
pub mod command;
pub mod corrections;
#[cfg(test)]
mod delete_range_tests;
pub mod duration;
mod editor_map;
mod keymap_config;
pub use editor_map::BindingId;
pub mod cutaway;
pub mod escalation;
pub mod gag;
pub mod gain;
mod group;
#[cfg(test)]
mod group_tests;
pub mod hold_effects;
#[cfg(test)]
mod macro_tests;
#[cfg(test)]
mod mark_tests;
#[cfg(test)]
mod object_tests;
#[cfg(test)]
mod operator_tests;
pub mod panels;
#[cfg(test)]
mod register_tests;
pub mod registry;
#[cfg(test)]
mod repeat_last_tests;
#[cfg(test)]
mod repeat_operator_tests;
pub mod retime;
pub mod room_tone;
pub mod slip;
mod sound;
pub mod splice;
pub mod trim;
pub mod youtube;
pub mod zoom;
pub use sound::SoundAction;
#[cfg(any(test, feature = "ui-harness"))]
pub mod shortcut_audit;
pub use camera::route_camera_key;

/// A native ⌘ chord, with or without Shift: egui sets `command` and macOS
/// also sets `mac_cmd`, so either flag counts. Option and Control+⌘ do not.
pub fn native_command(modifiers: Modifiers) -> bool {
    (modifiers.command || modifiers.mac_cmd)
        && !modifiers.alt
        && !(modifiers.ctrl && modifiers.mac_cmd)
}

/// Normalize a press for the fixed mode routers by its immediate companion
/// text; `None` is a character none of them names. See `keymap_config`.
pub fn mode_key(key: Key, modifiers: Modifiers, text: Option<&str>) -> Option<(Key, Modifiers)> {
    keymap_config::mode_key(key, modifiers, text)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeatEdit {
    Split,
    InsertHold(duration::DurationInput),
    /// `:hold … video=black`: a silent pause with black picture.
    InsertBlack(duration::DurationInput),
    Repeat(u32),
    WrapRepeat(u32),
    /// Set the selected Repeat's per-play escalation.
    Escalate(escalation::EscalationInput),
    /// Place or clear picture-only cutaways over the selected beat.
    Cutaway(cutaway::CutawayInput),
    Delete,
    HoldDuration(deadpan_core::FrameDuration),
    Retime(retime::RetimeInput),
    /// `:pitch +3st`: shift the selected beat's pitch by whole semitones at
    /// its current speed, with pitch-preserving processing (0 removes it).
    Pitch(i8),
    /// `:audio-lag +80ms`: the selected Source's sound plays this much later
    /// (or earlier) than its picture, as an explicit link offset.
    AudioLag {
        earlier: bool,
        amount: Option<duration::DurationInput>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pane {
    Sources,
    #[default]
    Viewer,
    Sequence,
    Sounds,
    Inspector,
}

impl Pane {
    pub fn visible(self, inspector: bool) -> Self {
        if self == Self::Inspector && !inspector {
            Self::Viewer
        } else {
            self
        }
    }

    /// Like [`Self::cycle_visible`], also skipping panes `drawn` rejects.
    /// The Viewer is always available.
    pub fn cycle_available(
        self,
        reverse: bool,
        inspector: bool,
        drawn: impl Fn(Self) -> bool,
    ) -> Self {
        let mut pane = self;
        for _ in 0..5 {
            pane = pane.cycle_visible(reverse, inspector);
            if pane == Self::Viewer || drawn(pane) {
                return pane;
            }
        }
        Self::Viewer
    }

    pub fn cycle_visible(self, reverse: bool, inspector: bool) -> Self {
        if !inspector {
            return if self == Self::Inspector {
                Self::Viewer
            } else {
                self.cycle(reverse)
            };
        }
        match (self, reverse) {
            (Self::Sources, false) | (Self::Inspector, true) => Self::Viewer,
            (Self::Viewer, false) | (Self::Sequence, true) => Self::Inspector,
            (Self::Inspector, false) | (Self::Sounds, true) => Self::Sequence,
            (Self::Sequence, false) | (Self::Sources, true) => Self::Sounds,
            (Self::Sounds, false) | (Self::Viewer, true) => Self::Sources,
        }
    }

    pub fn cycle(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::Sources, false) | (Self::Sequence, true) => Self::Viewer,
            (Self::Viewer, false) | (Self::Sounds, true) => Self::Sequence,
            (Self::Sequence, false) | (Self::Sources, true) => Self::Sounds,
            (Self::Sounds, false) | (Self::Viewer, true) => Self::Sources,
            (Self::Inspector, _) => Self::Viewer,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Repeat {
        selector: deadpan_core::SemanticSelector,
        plays: std::num::NonZeroU32,
    },
    Operator {
        cut: bool,
        selector: deadpan_core::SemanticSelector,
    },
    New,
    /// Start a project from one YouTube video URL (`⌘⇧N`, `:youtube`).
    NewFromUrl,
    Open,
    Import,
    Render,
    Insert,
    Trim,
    Undo,
    Redo,
    Playback,
    Audition,
    EnterGroup,
    LeaveGroup,
    Group,
    Ungroup,
    /// `:explode`: the selected Repeat becomes an ordinary Sequence of its plays.
    Explode,
    /// `:duplicate`: copy the selected beat or Visual range after itself.
    Duplicate,
    /// `,e`: wrap the selected beat or Visual range in an escalating Repeat.
    EscalatingRepeat,
    /// `,m`: mute the Visual range inside the selected beat, or toggle the
    /// whole beat's mute.
    Mute,
    /// `,r`: pick a register holding an Original moment for a reaction
    /// cutaway over the selected beat or Visual range.
    CutawayPicker,
    /// `:lift`: cut the Visual range and refill its time with a silent black
    /// pause of the same length.
    Lift,
    /// `,b` / `:bleep`: replace the Visual range's sound with a tone while
    /// its pictures keep playing.
    Bleep {
        frequency_hz: u32,
        level_millidecibels: i32,
    },
    /// `,t`: open `:tail` with its length ready to change, for the selected
    /// pause or a new tail pause at the cursor.
    TailPicker,
    /// `:reverse D` / `:ping-pong D`: a pause that plays the `D` before the
    /// cursor backwards; `bounce` leaves out the picture at the turn.
    Reverse {
        length: duration::DurationInput,
        bounce: bool,
    },
    /// `:repeat N role=audio|video [overflow=trim]`: repeat one role of the
    /// Visual range over its beat without inserting time.
    RoleRepeat {
        role: deadpan_core::MediaRole,
        plays: std::num::NonZeroU32,
        trim: bool,
    },
    /// `:jcut D` / `:lcut D`: move the sound's cut at the cursor's seam
    /// earlier or later by `D` while the picture still cuts at the cursor.
    SplitEdit {
        kind: deadpan_core::SplitEditKind,
        length: duration::DurationInput,
    },
    /// `:tail [D] [effect=…]`: a hanging effect tail on the selected pause,
    /// or a new tail pause at the cursor.
    Tail {
        length: Option<duration::DurationInput>,
        effect: deadpan_core::TailEffect,
    },
    /// `:framing-save a`: keep the selected beat's framing as a reusable
    /// preset, a one-instruction macro applied with `@a`.
    SaveFraming(char),
    /// AI pause pictures for the selected Hold (`,a`, `:generate`, …).
    Ai(AiAction),
    /// `:gag NAME`: apply a built-in recipe.
    Gag(gag::GagInput),
    VisualMoment,
    SelectObject(deadpan_core::SemanticTextObject),
    DeleteSelection,
    DeleteFrames(u32),
    RepeatLast,
    CopyMoment,
    SelectRegister(char),
    MacroRecord(char),
    MacroStop,
    MacroCancel,
    MacroExecute {
        register: char,
        count: u32,
    },
    SetMark(char),
    JumpMark(char),
    DeleteMark(char),
    JumpHistory {
        forward: bool,
    },
    Marks,
    PasteMoment {
        before: bool,
    },
    Step {
        forward: bool,
        count: u32,
    },
    Beat {
        forward: bool,
        count: u32,
    },
    /// Recognized word starts (`w`, `b`) or the next word end (`e`).
    Word {
        forward: bool,
        end: bool,
        count: u32,
    },
    /// Recognized sentence starts (`W`, `B`).
    Sentence {
        forward: bool,
        count: u32,
    },
    /// Repeat plays of the nearest open Repeat (`]r`, `[r`): All plays,
    /// then play 1..N; on a selected Repeat it opens play 1 or the last.
    Play {
        forward: bool,
        count: u32,
    },
    /// Detected pause starts (`]p`, `[p`).
    Pause {
        forward: bool,
        count: u32,
    },
    /// Detected shot starts (`]s`, `[s`).
    Shot {
        forward: bool,
        count: u32,
    },
    /// Select a word, sentence or pause object (`iw`, `aw`, `is`, `as`,
    /// `ip`, `ap`).
    SelectSpeech(deadpan_core::SpeechObject),
    /// Next / previous transcript search match (`n`, `N`).
    SearchStep {
        forward: bool,
    },
    First,
    Last,
    Pane {
        reverse: bool,
    },
    Search,
    Command,
    Help,
    Escape,
    OfferInsert,
    Edit(BeatEdit),
    Framing(FramingAction),
    Sound(SoundAction),
    GainStep(i32),
    Invalid(&'static str),
}

/// Empty Visual selections remain explicit targets, including after v finishes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditSelection {
    #[default]
    None,
    Empty,
    Range,
    Object,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoutingDomain {
    #[default]
    Edit,
    Original,
    Sound,
}

/// Selectors reachable below a pending typed Repeat path. The completed action
/// carries the exact selector; this query never authorizes a completed edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatPendingScope {
    SelectedBeat,
    Motion,
    VisualSelection,
    TextObject,
    Mixed,
}

/// The AI pause workflow. Only Accept edits the project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiAction {
    /// Generate this many variants, one after another.
    Generate {
        variants: u8,
    },
    Cancel,
    /// Choose which Ready variant Preview, Audition and Accept use.
    Choose(VariantChoice),
    Preview,
    /// Loop the pause in context with the chosen variant's pictures and the
    /// pause's own sound, previewing it first when needed.
    Audition,
    Accept,
    Discard,
    /// Compare at the same picture frame and heard sample: switch the
    /// viewer and audition between the pause's committed picture (Before)
    /// and a Ready variant, previewing the chosen variant first when needed.
    Compare(CompareChoice),
    /// Keep the chosen variant from automatic expiry, or release it.
    Keep,
}

/// What an AI comparison shows next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareChoice {
    /// Before when a variant shows, otherwise the chosen variant.
    Toggle,
    /// The pause's committed picture (its freeze or accepted pictures).
    Before,
    /// 1-based, as the inspector numbers them.
    Variant(u8),
}

/// Which Ready AI variant to choose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantChoice {
    Next,
    Previous,
    /// 1-based, as the inspector numbers them.
    Number(u8),
}

/// Framing actions owned by the selected edited beat.
///
/// Camera opens a cancellable draft. PunchIn and Creep are ordinary typed
/// edits and must be committed by the project service as one history step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramingAction {
    EnterCamera,
    PunchIn,
    Creep,
}

const DIGITS: &[(Key, u32)] = &[
    (Key::Num0, 0),
    (Key::Num1, 1),
    (Key::Num2, 2),
    (Key::Num3, 3),
    (Key::Num4, 4),
    (Key::Num5, 5),
    (Key::Num6, 6),
    (Key::Num7, 7),
    (Key::Num8, 8),
    (Key::Num9, 9),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkPrefix {
    Set,
    Jump,
}

/// An immutable compiled map with independent pending-path and held-key state.
#[derive(Clone)]
pub struct Bindings {
    map: std::sync::Arc<editor_map::Compiled>,
    count: Option<u32>,
    count_overflow: bool,
    path: Vec<editor_map::Stroke>,
    selection: Option<EditSelection>,
    held: Option<HeldBinding>,
    macro_recording: bool,
    domain: RoutingDomain,
    domain_interrupted: bool,
    motion_count: Option<u32>,
    motion_count_depth: Option<usize>,
    operator_refusal: Option<&'static str>,
}

#[derive(Clone, Copy)]
struct HeldBinding {
    identity: Key,
    stroke: editor_map::Stroke,
    action: Action,
}

impl Default for Bindings {
    fn default() -> Self {
        Self::with_map(std::sync::Arc::clone(&editor_map::SHIPPED))
    }
}

impl Bindings {
    fn with_map(map: std::sync::Arc<editor_map::Compiled>) -> Self {
        Self {
            map,
            count: None,
            count_overflow: false,
            path: Vec::new(),
            selection: None,
            held: None,
            macro_recording: false,
            domain: RoutingDomain::Edit,
            domain_interrupted: false,
            motion_count: None,
            motion_count_depth: None,
            operator_refusal: None,
        }
    }

    #[cfg(test)]
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        Self::from_json_reporting(bytes).map(|(bindings, _)| bindings)
    }

    /// The complete map plus non-fatal warnings about logical strokes that
    /// no layout, or only some layouts, can deliver.
    pub fn from_json_reporting(bytes: &[u8]) -> Result<(Self, Vec<String>), String> {
        keymap_config::parse(bytes).map(|(map, warnings)| (Self::with_map(map), warnings))
    }

    /// Teaching for a press the router refused because Kestrel reserves its
    /// physical chord although the layout types an editor character there:
    /// QWERTZ Option+5 and AZERTY Shift+Option+5 type `[`, QWERTZ Option+L
    /// types `@`. Logical maps only; `logical_text` is the immediate companion.
    pub fn reserved_layout_notice(
        &self,
        key: Key,
        physical_key: Option<Key>,
        modifiers: Modifiers,
        logical_text: Option<&str>,
    ) -> Option<String> {
        let text = logical_text?;
        let physical = physical_key.unwrap_or(key);
        if self.map.mode != editor_map::KeyMode::Logical
            || modifiers.ctrl
            || modifiers.command
            || modifiers.mac_cmd
            || !keymap_config::reservations::reserved(physical, modifiers)
            || !(text.len() == 1 && text.as_bytes()[0].is_ascii_punctuation())
        {
            return None;
        }
        let chord = format!(
            "{}{}{}",
            if modifiers.shift { "Shift+" } else { "" },
            if modifiers.alt { "Option+" } else { "" },
            physical.symbol_or_name()
        );
        let alternative = match text {
            "[" => {
                "For [r use :scope play N or :scope all. [p and [s have no command: bind pause.previous / shot.previous to another key in keymap.json, or go back with gg and a counted ]p / ]s."
            }
            "@" => "Run a macro with :macro a (a count after the name repeats it).",
            _ => "Bind that action to another key in keymap.json.",
        };
        Some(format!(
            "{chord} types {text} on this layout, but Kestrel reserves {chord}, so Deadpan ignores it. {alternative}"
        ))
    }

    pub fn set_routing_domain(&mut self, domain: RoutingDomain) {
        if self.domain != domain {
            let interrupted = !self.path.is_empty();
            self.clear();
            self.domain = domain;
            self.domain_interrupted = interrupted;
        }
    }

    pub fn operator_pending(&self) -> bool {
        !self.path.is_empty()
            && self
                .map
                .operator_pending(&self.path, self.active_selection(), self.domain)
    }

    pub fn repeat_pending(&self) -> bool {
        self.repeat_pending_scope().is_some()
    }

    pub fn repeat_pending_scope(&self) -> Option<RepeatPendingScope> {
        if self.path.is_empty() {
            return None;
        }
        self.map
            .repeat_pending_scope(&self.path, self.active_selection(), self.domain)
    }

    pub fn key_label_in(
        &self,
        id: BindingId,
        domain: RoutingDomain,
        selection: EditSelection,
    ) -> String {
        self.map
            .labels(Self::contextual_id(id, domain, selection))
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    pub fn key_labels_in(
        &self,
        id: BindingId,
        domain: RoutingDomain,
        selection: EditSelection,
    ) -> String {
        self.map
            .labels(Self::contextual_id(id, domain, selection))
            .join(" / ")
    }

    fn contextual_id(id: BindingId, domain: RoutingDomain, selection: EditSelection) -> BindingId {
        if id == BindingId::Copy
            && domain == RoutingDomain::Edit
            && selection == EditSelection::None
        {
            BindingId::CopyBeat
        } else if id == BindingId::Repeat
            && domain == RoutingDomain::Edit
            && selection != EditSelection::None
        {
            BindingId::RepeatRange
        } else {
            id
        }
    }

    /// Synchronize native recording mode before routing, including immediately
    /// after a start or stop in the same input batch.
    pub fn set_macro_recording(&mut self, recording: bool) {
        if self.macro_recording != recording {
            self.clear();
            self.macro_recording = recording;
        }
    }

    /// Capture macro intent from the first configured ancestor, including the
    /// complete family prefix. A delayed letter must not capture a newer target.
    pub fn macro_pending(&self) -> bool {
        matches!(
            self.family_prefix(),
            Some(editor_map::PrefixKind::MacroRecord | editor_map::PrefixKind::MacroExecute)
        ) || [BindingId::MacroRecord, BindingId::MacroExecute]
            .into_iter()
            .any(|id| {
                self.map
                    .has_descendant(&self.path, self.active_selection(), id, self.domain)
            })
    }

    pub fn key_mode_label(&self) -> &'static str {
        match self.map.mode {
            editor_map::KeyMode::Logical => "Logical keys",
            editor_map::KeyMode::Physical => "Physical positions",
        }
    }

    pub fn key_label(&self, id: BindingId) -> String {
        self.map.labels(id).into_iter().next().unwrap_or_default()
    }
    pub fn key_labels(&self, id: BindingId) -> String {
        self.map.labels(id).join(" / ")
    }
    pub fn counted_label(&self, id: BindingId, count: u32) -> String {
        format!("{count}{}", self.key_label(id))
    }

    /// Cancel pending input while retaining a resolved motion's autorepeat latch.
    /// Use only for synchronous motion-induced context updates.
    pub fn clear_pending(&mut self) {
        self.count = None;
        self.count_overflow = false;
        self.path.clear();
        self.selection = None;
        self.motion_count = None;
        self.motion_count_depth = None;
        self.operator_refusal = None;
        self.domain_interrupted = false;
    }
    /// Focus, pointer, mode, or external context changes also revoke held input.
    pub fn clear(&mut self) {
        self.clear_pending();
        self.held = None;
    }

    fn active_selection(&self) -> EditSelection {
        self.selection.unwrap_or(EditSelection::None)
    }
    pub fn mark_prefix(&self) -> Option<MarkPrefix> {
        match self.family_prefix() {
            Some(editor_map::PrefixKind::Mark(kind)) => Some(kind),
            _ => None,
        }
    }
    fn family_prefix(&self) -> Option<editor_map::PrefixKind> {
        self.map
            .prefix(&self.path, self.active_selection(), self.domain)
    }
    fn letter_family_pending(&self) -> bool {
        self.family_prefix().is_some_and(|kind| !kind.is_operator())
    }

    fn effective_count(&self) -> Option<u32> {
        self.motion_count.or(self.count)
    }
    fn counts(&self) -> editor_map::Counts {
        editor_map::Counts {
            leading: self.count,
            motion: self.motion_count,
        }
    }
    /// The pending path can still complete an AI pause binding.
    pub fn ai_pending(&self) -> bool {
        self.count.is_none()
            && !self.count_overflow
            && [
                BindingId::GenerateAi,
                BindingId::CompareAi,
                BindingId::NextAi,
            ]
            .into_iter()
            .any(|id| {
                self.map
                    .has_descendant(&self.path, self.active_selection(), id, self.domain)
            })
    }
    pub fn trim_pending(&self) -> bool {
        self.count.is_none()
            && !self.count_overflow
            && self.map.has_descendant(
                &self.path,
                self.active_selection(),
                BindingId::Trim,
                self.domain,
            )
    }
    #[cfg(test)]
    pub fn reuse_pending(&self) -> bool {
        self.count.is_none()
            && !self.count_overflow
            && self.map.has_descendant(
                &self.path,
                self.active_selection(),
                BindingId::Insert,
                self.domain,
            )
    }

    /// Compatibility probe. Event routing itself owns autorepeat admission.
    #[cfg(test)]
    pub fn allows_key_repeat(&self, key: Key, modifiers: Modifiers) -> bool {
        let Some(stroke) = self.stroke(key, Some(key), modifiers) else {
            return false;
        };
        let mut path = self.path.clone();
        path.push(stroke);
        self.map
            .rule(&path, self.active_selection(), self.domain)
            .is_some_and(|rule| rule.repeatable)
    }

    #[cfg(test)]
    pub fn native_control_owns_cut(
        &self,
        key: Key,
        modifiers: Modifiers,
        selection: EditSelection,
    ) -> bool {
        self.native_control_owns_cut_event(key, Some(key), modifiers, selection)
    }
    #[cfg(test)]
    pub fn native_control_owns_cut_event(
        &self,
        key: Key,
        physical_key: Option<Key>,
        modifiers: Modifiers,
        selection: EditSelection,
    ) -> bool {
        self.native_control_owns_cut_event_with_logical_text(
            key,
            physical_key,
            modifiers,
            selection,
            None,
        )
    }

    pub fn native_control_owns_cut_event_with_logical_text(
        &self,
        key: Key,
        physical_key: Option<Key>,
        modifiers: Modifiers,
        selection: EditSelection,
        logical_text: Option<&str>,
    ) -> bool {
        match self.clone().route_event_with_logical_text(
            key,
            physical_key,
            modifiers,
            false,
            false,
            false,
            true,
            selection,
            logical_text,
        ) {
            Some(
                Action::DeleteFrames(_)
                | Action::DeleteSelection
                | Action::RepeatLast
                | Action::Repeat { .. }
                | Action::Group
                | Action::Ungroup
                | Action::Explode
                | Action::Duplicate
                | Action::MacroExecute { .. },
            ) => true,
            Some(Action::Operator { cut: true, .. }) => true,
            Some(Action::Edit(BeatEdit::Delete)) => selection != EditSelection::None,
            _ => false,
        }
    }

    pub fn pending(&self) -> String {
        if let Some(count) = self.motion_count {
            let depth = self
                .motion_count_depth
                .expect("motion counts require an operator prefix");
            return format!(
                "{}{}{}{}",
                self.count
                    .map_or_else(String::new, |count| count.to_string()),
                self.map.path_label(&self.path[..depth]),
                if self.count_overflow {
                    "count overflow".into()
                } else {
                    count.to_string()
                },
                self.map.path_label(&self.path[depth..])
            );
        }
        format!(
            "{}{}",
            if self.count_overflow {
                "count overflow".into()
            } else {
                self.count
                    .map_or_else(String::new, |count| count.to_string())
            },
            self.map.path_label(&self.path)
        )
    }
    pub fn pending_next_keys(&self) -> Option<String> {
        if let Some(message) = self.operator_refusal {
            return Some(message.into());
        }
        if self.count_overflow {
            return Some("Count is too large. Esc clears it.".into());
        }
        if self.path.is_empty() && self.count.is_none() {
            return None;
        }
        self.map.next_keys(
            &self.path,
            self.active_selection(),
            self.counts(),
            self.macro_recording,
            self.domain,
        )
    }
    pub fn pending_hint(&self) -> Option<String> {
        if let Some(message) = self.operator_refusal {
            return Some(message.into());
        }
        if self.count_overflow {
            return Some("Count is too large. Esc clears it.".into());
        }
        if !self.path.is_empty() {
            return self.map.hint(
                &self.path,
                self.active_selection(),
                self.counts(),
                self.macro_recording,
                self.domain,
            );
        }
        self.count.map(|count| {
            if count == 0 { format!("Zero count: {}/{} moves one frame; {} requests zero time; Esc clears it.",
                self.key_label(BindingId::FramePrevious), self.key_label(BindingId::FrameNext), self.key_label(BindingId::Hold)) }
            else { format!("Then {}/{} to move, {} to cut frames, {} for total plays, {}/{} for gain, {} to pause, or {} + name to execute a macro · Esc cancels",
                self.key_label(BindingId::FramePrevious), self.key_label(BindingId::FrameNext), self.key_label(BindingId::CutFrames),
                self.key_label(BindingId::Repeat), self.key_label(BindingId::GainUp), self.key_label(BindingId::GainDown), self.key_label(BindingId::Hold), self.key_label(BindingId::MacroExecute)) }
        })
    }

    #[cfg(any(test, feature = "ui-harness"))]
    pub fn key(&mut self, key: Key, modifiers: Modifiers, text: bool, ime: bool) -> Option<Action> {
        self.key_with_selection(key, modifiers, text, ime, EditSelection::None)
    }
    #[cfg(any(test, feature = "ui-harness"))]
    pub fn key_with_selection(
        &mut self,
        key: Key,
        modifiers: Modifiers,
        text: bool,
        ime: bool,
        selection: EditSelection,
    ) -> Option<Action> {
        self.route_event(key, Some(key), modifiers, text, ime, false, true, selection)
    }

    #[cfg(test)]
    fn stroke(
        &self,
        key: Key,
        physical_key: Option<Key>,
        modifiers: Modifiers,
    ) -> Option<editor_map::Stroke> {
        self.stroke_with_logical_text(key, physical_key, modifiers, None)
    }

    #[cfg(any(test, feature = "ui-harness"))]
    fn audit_stroke(
        &mut self,
        stroke: editor_map::Stroke,
        selection: EditSelection,
    ) -> Option<Action> {
        let (key, modifiers, text) = match stroke {
            editor_map::Stroke::Key(key, shift) => (
                key,
                if shift {
                    Modifiers::SHIFT
                } else {
                    Modifiers::NONE
                },
                None,
            ),
            editor_map::Stroke::At => (Key::Num2, Modifiers::SHIFT, Some("@")),
        };
        self.route_event_with_logical_text(
            key,
            Some(key),
            modifiers,
            false,
            false,
            false,
            true,
            selection,
            text,
        )
    }

    fn stroke_with_logical_text(
        &self,
        key: Key,
        physical_key: Option<Key>,
        modifiers: Modifiers,
        logical_text: Option<&str>,
    ) -> Option<editor_map::Stroke> {
        use editor_map::{KeyMode, Stroke};
        if modifiers.ctrl || modifiers.command || modifiers.mac_cmd {
            return None;
        }
        if self.map.mode == KeyMode::Logical
            && let Some(text) = logical_text
        {
            // egui-winit falls back to the physical position when a layout's
            // character has no egui key: AZERTY `&` arrives as Num1, QWERTZ `"`
            // as Shift+Num2 and `ö` as Semicolon. The immediate companion text
            // is the layout's actual character, so it decides the stroke.
            match keymap_config::companion_stroke(key, text) {
                keymap_config::Companion::Stroke(stroke) => return Some(stroke),
                keymap_config::Companion::Unnamed => return None,
                keymap_config::Companion::Delivered => {}
            }
        }
        let key = if self.map.mode == KeyMode::Physical {
            physical_key?
        } else {
            key
        };
        // Logical mode follows egui's delivered identity, including its fallback
        // to a physical identity when a layout cannot produce a named egui key.
        // A physical map requires an actual physical key and never falls back.
        if self.map.mode == KeyMode::Logical && keymap_config::logical_symbol(key) {
            // Keep the shipped Option+Plus refusal. Every physical reservation
            // has already been checked before interpreting a layout's symbol.
            return (!(modifiers.alt && key == Key::Plus)).then_some(Stroke::Key(key, false));
        }
        if modifiers.alt {
            return None;
        }
        Some(Stroke::Key(key, modifiers.shift))
    }

    /// Route both presses and releases. Count applies to the first execution;
    /// subsequent repeats use the same resolved motion with a unit count. A
    /// repeat can never enter, complete, or consume an unlatched pending path.
    #[allow(clippy::too_many_arguments)]
    pub fn route_event(
        &mut self,
        key: Key,
        physical_key: Option<Key>,
        modifiers: Modifiers,
        text: bool,
        ime: bool,
        repeat: bool,
        pressed: bool,
        selection: EditSelection,
    ) -> Option<Action> {
        self.route_event_with_logical_text(
            key,
            physical_key,
            modifiers,
            text,
            ime,
            repeat,
            pressed,
            selection,
            None,
        )
    }

    /// `logical_text` must come from the immediate `Event::Text` companion of
    /// this key event, never a later event, paste or IME commit. It only restores
    /// egui's missing `@` identity; physical maps continue to use positions.
    #[allow(clippy::too_many_arguments)]
    pub fn route_event_with_logical_text(
        &mut self,
        key: Key,
        physical_key: Option<Key>,
        modifiers: Modifiers,
        text: bool,
        ime: bool,
        repeat: bool,
        pressed: bool,
        selection: EditSelection,
        logical_text: Option<&str>,
    ) -> Option<Action> {
        let identity = physical_key.unwrap_or(key);
        if !pressed {
            if self.held.is_some_and(|held| held.identity == identity) {
                self.held = None;
            }
            return None;
        }
        if ime {
            self.clear();
            return None;
        }
        if keymap_config::modifier_key(key) {
            if text {
                self.clear();
            }
            return None;
        }
        // Kestrel owns physical positions even when a layout delivers a different
        // logical symbol. Without a physical identity, the logical fallback is
        // the strongest check the event provides; physical maps never use it.
        if keymap_config::reservations::reserved(physical_key.unwrap_or(key), modifiers) {
            self.clear();
            return None;
        }
        if repeat {
            if text {
                self.clear();
                return None;
            }
            let stroke = self.stroke_with_logical_text(key, physical_key, modifiers, logical_text);
            return self
                .held
                .filter(|held| held.identity == identity && Some(held.stroke) == stroke)
                .map(|held| held.action);
        }
        self.held = None;
        // Native application shortcuts have priority over the editor trie.
        // egui also carries `command` on Control outside macOS.
        if matches!(key, Key::O | Key::I)
            && modifiers.matches_exact(Modifiers::CTRL)
            && !modifiers.mac_cmd
        {
            let pending = !self.pending().is_empty();
            self.clear();
            return (!text).then_some(if pending {
                Action::Invalid(
                    "Use Ctrl-o or Ctrl-i without a count or pending prefix; no jump was made.",
                )
            } else {
                Action::JumpHistory {
                    forward: key == Key::I,
                }
            });
        }
        if key == Key::R && modifiers.matches_exact(Modifiers::CTRL) && !modifiers.mac_cmd {
            self.clear();
            return (!text).then_some(Action::Redo);
        }
        if native_command(modifiers) {
            self.clear();
            return match (key, modifiers.shift) {
                (Key::N, false) => Some(Action::New),
                (Key::N, true) => Some(Action::NewFromUrl),
                (Key::O, false) => Some(Action::Open),
                (Key::I, false) => Some(Action::Import),
                (Key::E, false) if !text => Some(Action::Render),
                (Key::Z, false) if !text => Some(Action::Undo),
                (Key::Z, true) if !text => Some(Action::Redo),
                _ => None,
            };
        }
        if text {
            self.clear();
            return None;
        }

        if (key == Key::Escape && modifiers == Modifiers::NONE)
            || (key == Key::Tab && (modifiers == Modifiers::NONE || modifiers == Modifiers::SHIFT))
        {
            self.clear();
            return Some(if key == Key::Escape {
                Action::Escape
            } else {
                Action::Pane {
                    reverse: modifiers.shift,
                }
            });
        }

        if self.domain_interrupted {
            self.clear();
            return Some(Action::Invalid(
                "The editor context changed during the pending command; start it again.",
            ));
        }

        if self.letter_family_pending()
            && modifiers != Modifiers::NONE
            && modifiers != Modifiers::SHIFT
        {
            self.clear();
            return None;
        }
        let Some(stroke) =
            self.stroke_with_logical_text(key, physical_key, modifiers, logical_text)
        else {
            self.clear();
            return None;
        };
        // A shifted physical digit can be a configured position (the shipped
        // macro path uses Shift+2). Unbound positions retain count behavior.
        let configured_shifted_digit = self.map.mode == editor_map::KeyMode::Physical
            && matches!(stroke, editor_map::Stroke::Key(_, true))
            && self
                .map
                .map(self.selection.unwrap_or(selection), self.domain)
                .resolve(&{
                    let mut path = self.path.clone();
                    path.push(stroke);
                    path
                })
                .is_some();
        if !configured_shifted_digit
            && let Some((_, digit)) = DIGITS.iter().find(
                |(bound, _)| matches!(stroke, editor_map::Stroke::Key(key, _) if key == *bound),
            )
        {
            if !self.path.is_empty() {
                if self
                    .family_prefix()
                    .is_some_and(editor_map::PrefixKind::is_operator)
                {
                    if self
                        .motion_count_depth
                        .is_some_and(|depth| depth != self.path.len())
                    {
                        self.operator_refusal = Some(
                            "Put the motion count after the complete operator prefix; start the command again.",
                        );
                        return None;
                    }
                    self.motion_count_depth = Some(self.path.len());
                    if self.count.is_some() {
                        self.operator_refusal = Some(
                            "Use one count before the operator or before its motion, not both.",
                        );
                    }
                    if let Some(count) = self
                        .motion_count
                        .unwrap_or(0)
                        .checked_mul(10)
                        .and_then(|count| count.checked_add(*digit))
                    {
                        self.motion_count = Some(count);
                    } else {
                        self.count_overflow = true;
                    }
                    return None;
                }
                if self.operator_pending() || self.repeat_pending() {
                    self.operator_refusal =
                        Some("Put the motion count immediately after the operator prefix.");
                    return None;
                }
                let first_prefix = self.map.has_descendant(
                    &self.path,
                    self.active_selection(),
                    BindingId::First,
                    self.domain,
                );
                self.clear();
                return (!first_prefix)
                    .then_some(Action::Invalid("Put one count before the command path."));
            }
            if let Some(count) = self
                .count
                .unwrap_or(0)
                .checked_mul(10)
                .and_then(|value| value.checked_add(*digit))
            {
                self.count = Some(count);
            } else {
                self.count_overflow = true;
            }
            return None;
        }
        let selection = *self.selection.get_or_insert(selection);
        let mut next = self.path.clone();
        next.push(stroke);
        let existing = self.map.map(selection, self.domain).resolve(&next);
        // Existing continuations always win. Semantic root interrupts retain
        // their shipped cancellation behavior only when no continuation exists.
        if existing.is_none()
            && !self.letter_family_pending()
            && let Some(rule) = self.map.rule(&[stroke], selection, self.domain)
            && rule.interrupt
        {
            let action = rule.resolve(None);
            self.clear();
            return Some(action);
        }
        let Some(node) = existing else {
            let had_pending = !self.path.is_empty();
            let start_only =
                self.map
                    .has_descendant(&self.path, selection, BindingId::First, self.domain);
            let attempted_family = self.map.prefix(&[stroke], selection, self.domain).is_some();
            let root_id = self
                .map
                .rule(&[stroke], selection, self.domain)
                .map(editor_map::Rule::id);
            let deliberate_refusal = attempted_family
                || matches!(
                    root_id,
                    Some(BindingId::GainUp | BindingId::GainDown | BindingId::PasteBefore)
                );
            let modified = modifiers != Modifiers::NONE
                && !deliberate_refusal
                && !self.letter_family_pending();
            self.clear();
            return (had_pending && !modified && (!start_only || deliberate_refusal)).then_some(
                Action::Invalid("Key does not continue the pending command; no action was taken."),
            );
        };
        if node
            .prefix()
            .is_some_and(|prefix| prefix.value.is_operator())
            && self
                .motion_count_depth
                .is_some_and(|depth| depth != next.len())
        {
            self.operator_refusal = Some(
                "Put the motion count after the complete operator prefix; start the command again.",
            );
        }
        if let Some(prefix) = node.prefix()
            && (self.effective_count().is_some() || self.count_overflow)
            && !prefix.value.is_operator()
            && !prefix.value.allows_count(self.effective_count())
        {
            let message = prefix.value.count_refusal();
            self.clear();
            return Some(Action::Invalid(message));
        }
        if self.macro_recording
            && node
                .prefix()
                .is_some_and(|prefix| prefix.value == editor_map::PrefixKind::MacroRecord)
        {
            self.clear();
            return Some(Action::MacroStop);
        }
        if let Some(binding) = node.terminal() {
            // Root transport/group commands have always ignored pending counts,
            // including an overflow. Their aliases inherit that semantic policy.
            let ignores_overflow = binding.value.interrupt;
            let action = if let Some(message) = self.operator_refusal {
                Action::Invalid(message)
            } else if self.count_overflow && !ignores_overflow {
                Action::Invalid("Count exceeds 4294967295; no edit was made.")
            } else {
                binding.value.resolve_counts(self.counts())
            };
            let held = binding.value.repeatable.then(|| HeldBinding {
                identity,
                stroke,
                action: binding.value.resolve(None),
            });
            self.clear_pending();
            if !matches!(action, Action::Invalid(_)) {
                self.held = held;
            }
            return Some(action);
        }
        if self.count_overflow
            && !self
                .map
                .has_descendant(&next, selection, BindingId::Hold, self.domain)
            && !self.map.operator_pending(&next, selection, self.domain)
            && self
                .map
                .repeat_pending_scope(&next, selection, self.domain)
                .is_none()
        {
            self.clear();
            return Some(Action::Invalid(
                "Count exceeds 4294967295; no edit was made.",
            ));
        }
        self.path = next;
        [
            BindingId::Insert,
            BindingId::Hold,
            BindingId::Repeat,
            BindingId::RepeatOperator,
            BindingId::RepeatRange,
            BindingId::CutBeat,
            BindingId::CopyBeat,
            BindingId::CutOperator,
            BindingId::YankOperator,
        ]
        .into_iter()
        .any(|id| {
            self.map
                .has_descendant(&self.path, selection, id, self.domain)
        })
        .then_some(Action::OfferInsert)
    }
}

fn mark_letter(key: Key, uppercase: bool) -> Option<char> {
    let letter: char = match key {
        Key::A => 'a',
        Key::B => 'b',
        Key::C => 'c',
        Key::D => 'd',
        Key::E => 'e',
        Key::F => 'f',
        Key::G => 'g',
        Key::H => 'h',
        Key::I => 'i',
        Key::J => 'j',
        Key::K => 'k',
        Key::L => 'l',
        Key::M => 'm',
        Key::N => 'n',
        Key::O => 'o',
        Key::P => 'p',
        Key::Q => 'q',
        Key::R => 'r',
        Key::S => 's',
        Key::T => 't',
        Key::U => 'u',
        Key::V => 'v',
        Key::W => 'w',
        Key::X => 'x',
        Key::Y => 'y',
        Key::Z => 'z',
        _ => return None,
    };
    Some(if uppercase {
        letter.to_ascii_uppercase()
    } else {
        letter
    })
}

/// A held key may navigate, but cannot finish an operator or repeat an edit.
#[cfg(test)]
pub fn allows_key_repeat(key: Key, modifiers: Modifiers) -> bool {
    modifiers == Modifiers::NONE
        && editor_map::rule(&[editor_map::Stroke::Key(key, false)], EditSelection::None)
            .is_some_and(|rule| rule.repeatable)
}

pub fn inspector_parameter_key(
    key: Key,
    modifiers: Modifiers,
    pane: Pane,
    text: bool,
    ime: bool,
) -> bool {
    key == Key::Enter && modifiers == Modifiers::NONE && pane == Pane::Inspector && !text && !ime
}

pub fn boundary_step(current: u64, length: u64, forward: bool, count: u32) -> u64 {
    if forward {
        current.saturating_add(u64::from(count)).min(length)
    } else {
        current.saturating_sub(u64::from(count)).min(length)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAction {
    Open,
    Leave,
}

pub fn text_action(
    key: Key,
    modifiers: Modifiers,
    text_focused: bool,
    ime_active: bool,
) -> Option<TextAction> {
    if !text_focused || ime_active || modifiers != Modifiers::NONE {
        return None;
    }
    match key {
        Key::Enter => Some(TextAction::Open),
        Key::Escape => Some(TextAction::Leave),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn render_captures_only_the_exact_shortcut_outside_text_and_composition() {
        use super::*;
        let command = Modifiers {
            command: true,
            mac_cmd: true,
            ..Modifiers::NONE
        };
        for (text, ime, expected) in [
            (false, false, Some(Action::Render)),
            (true, false, None),
            (false, true, None),
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(Key::E, command, text, ime), expected);
            assert!(bindings.pending().is_empty());
        }
        for modifiers in [
            Modifiers::NONE,
            Modifiers::ALT,
            Modifiers {
                shift: true,
                ..command
            },
            Modifiers {
                alt: true,
                ..command
            },
            Modifiers {
                ctrl: true,
                ..command
            },
        ] {
            assert_ne!(
                Bindings::default().key(Key::E, modifiers, false, false),
                Some(Action::Render)
            );
        }
        assert!(!allows_key_repeat(Key::E, command));
    }

    #[test]
    fn moment_keys_are_native_owned_and_never_counted_or_repeated_edits() {
        for (key, modifiers, action) in [
            (Key::V, Modifiers::NONE, Action::VisualMoment),
            (Key::Y, Modifiers::NONE, Action::CopyMoment),
            (
                Key::P,
                Modifiers::NONE,
                Action::PasteMoment { before: false },
            ),
            (
                Key::P,
                Modifiers::SHIFT,
                Action::PasteMoment { before: true },
            ),
        ] {
            let mut original = Bindings::default();
            original.set_routing_domain(RoutingDomain::Original);
            assert_eq!(original.key(key, modifiers, false, false), Some(action));
            assert_eq!(Bindings::default().key(key, modifiers, true, false), None);
            assert_eq!(Bindings::default().key(key, modifiers, false, true), None);
            assert!(!allows_key_repeat(key, modifiers));
            let mut pending = Bindings::default();
            pending.set_routing_domain(RoutingDomain::Original);
            pending.key(Key::Num3, Modifiers::NONE, false, false);
            assert!(matches!(
                pending.key(key, modifiers, false, false),
                Some(Action::Invalid(_))
            ));
            assert!(pending.pending().is_empty());
        }
    }

    #[test]
    fn group_navigation_cancels_prefixes_and_respects_native_input_ownership() {
        for (key, action) in [
            (Key::Enter, Action::EnterGroup),
            (Key::Backspace, Action::LeaveGroup),
        ] {
            for prefix in [
                vec![Key::G],
                vec![Key::R],
                vec![Key::Comma],
                vec![Key::Num3],
            ] {
                let mut bindings = Bindings::default();
                for part in prefix {
                    bindings.key(part, Modifiers::NONE, false, false);
                }
                assert_eq!(
                    bindings.key(key, Modifiers::NONE, false, false),
                    Some(action)
                );
                assert!(bindings.pending().is_empty());
                assert_eq!(bindings.key(Key::G, Modifiers::NONE, false, false), None);
                assert_eq!(bindings.pending(), "g");
            }
            for modifiers in [
                Modifiers::SHIFT,
                Modifiers::ALT,
                Modifiers::CTRL,
                Modifiers::COMMAND,
                Modifiers::MAC_CMD,
            ] {
                let mut bindings = Bindings::default();
                bindings.key(Key::G, Modifiers::NONE, false, false);
                assert_eq!(bindings.key(key, modifiers, false, false), None);
                assert!(bindings.pending().is_empty());
            }
            for (text, ime) in [(true, false), (false, true), (true, true)] {
                let mut bindings = Bindings::default();
                bindings.key(Key::R, Modifiers::NONE, false, false);
                assert_eq!(bindings.key(key, Modifiers::NONE, text, ime), None);
                assert!(bindings.pending().is_empty());
            }
            assert!(!allows_key_repeat(key, Modifiers::NONE));
        }
    }

    #[test]
    fn space_is_one_normal_mode_toggle_and_clears_pending_edits() {
        use super::*;
        for prefix in [None, Some(Key::R), Some(Key::Comma), Some(Key::G)] {
            let mut bindings = Bindings::default();
            if let Some(key) = prefix {
                bindings.key(key, Modifiers::NONE, false, false);
            }
            assert_eq!(
                bindings.key(Key::Space, Modifiers::NONE, false, false),
                Some(Action::Playback)
            );
            assert!(bindings.pending().is_empty());
        }
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                Bindings::default().key(Key::Space, Modifiers::NONE, text, ime),
                None
            );
        }
        for modifiers in [
            Modifiers::ALT,
            Modifiers::CTRL,
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
        ] {
            assert_eq!(
                Bindings::default().key(Key::Space, modifiers, false, false),
                None
            );
        }
        assert!(!allows_key_repeat(Key::Space, Modifiers::NONE));
    }

    #[test]
    fn shift_space_loops_once_and_preserves_native_and_global_ownership() {
        for prefix in [
            None,
            Some(Key::R),
            Some(Key::Comma),
            Some(Key::G),
            Some(Key::Num3),
        ] {
            let mut bindings = Bindings::default();
            if let Some(key) = prefix {
                bindings.key(key, Modifiers::NONE, false, false);
            }
            assert_eq!(
                bindings.key(Key::Space, Modifiers::SHIFT, false, false),
                Some(Action::Audition)
            );
            assert!(bindings.pending().is_empty());
        }
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            let mut bindings = Bindings::default();
            bindings.key(Key::R, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(Key::Space, Modifiers::SHIFT, text, ime), None);
            assert!(bindings.pending().is_empty());
        }
        for modifiers in [
            Modifiers::SHIFT | Modifiers::ALT,
            Modifiers::SHIFT | Modifiers::CTRL,
            Modifiers::SHIFT | Modifiers::COMMAND,
            Modifiers::SHIFT | Modifiers::MAC_CMD,
        ] {
            assert_eq!(
                Bindings::default().key(Key::Space, modifiers, false, false),
                None
            );
        }
        assert!(!allows_key_repeat(Key::Space, Modifiers::SHIFT));
    }
    use super::*;

    #[test]
    fn pause_leader_preserves_counts_and_cannot_fire_from_text_or_repeat() {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.key(Key::Num3, Modifiers::NONE, false, false), None);
        assert_eq!(
            bindings.key(Key::Comma, Modifiers::NONE, false, false),
            Some(Action::OfferInsert)
        );
        assert_eq!(bindings.pending(), "3,");
        assert_eq!(
            bindings.pending_hint(),
            Some("h inserts the counted pause · Esc cancels".into())
        );
        assert_eq!(
            bindings.key(Key::H, Modifiers::NONE, false, false),
            Some(Action::Edit(BeatEdit::InsertHold(
                duration::DurationInput::half_seconds(3)
            )))
        );
        assert!(bindings.pending().is_empty());
        assert!(!allows_key_repeat(Key::Comma, Modifiers::NONE));
        // h may repeat as a motion, but the pause prefix is consumed once.
        assert_eq!(
            bindings.key(Key::H, Modifiers::NONE, false, false),
            Some(Action::Step {
                forward: false,
                count: 1
            })
        );
        for (text, ime) in [(true, false), (false, true)] {
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(Key::H, Modifiers::NONE, text, ime), None);
            assert!(bindings.pending().is_empty());
        }
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert_eq!(
            bindings.key(Key::Escape, Modifiers::NONE, false, false),
            Some(Action::Escape)
        );
        assert!(bindings.pending().is_empty());
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert!(matches!(
            bindings.key(Key::Num2, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert!(matches!(
            bindings.key(Key::Q, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
    }

    #[test]
    fn logical_comma_accepts_layout_modifiers_but_never_command_or_control() {
        for modifiers in [
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::ALT | Modifiers::SHIFT,
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Num3, Modifiers::NONE, false, false);
            assert_eq!(
                bindings.key(Key::Comma, modifiers, false, false),
                Some(Action::OfferInsert)
            );
            assert_eq!(bindings.pending(), "3,");
            assert_eq!(
                bindings.key(Key::H, Modifiers::NONE, false, false),
                Some(Action::Edit(BeatEdit::InsertHold(
                    duration::DurationInput::half_seconds(3)
                )))
            );
        }
        for modifiers in [Modifiers::CTRL, Modifiers::COMMAND, Modifiers::MAC_CMD] {
            let mut bindings = Bindings::default();
            assert_eq!(bindings.key(Key::Comma, modifiers, false, false), None);
            assert!(bindings.pending().is_empty());
        }
    }

    #[test]
    fn comma_f_z_c_are_direct_unrepeatable_camera_actions() {
        for (key, action) in [
            (Key::F, Action::Framing(FramingAction::EnterCamera)),
            (Key::Z, Action::Framing(FramingAction::PunchIn)),
            (Key::C, Action::Framing(FramingAction::Creep)),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                bindings.key(Key::Comma, Modifiers::NONE, false, false),
                Some(Action::OfferInsert)
            );
            assert_eq!(bindings.pending(), ",");
            assert_eq!(
                bindings.key(key, Modifiers::NONE, false, false),
                Some(action)
            );
            assert!(bindings.pending().is_empty());
            assert!(!allows_key_repeat(key, Modifiers::NONE));
        }

        for (key, modifiers) in [
            (Key::F, Modifiers::NONE),
            (Key::Z, Modifiers::NONE),
            (Key::C, Modifiers::NONE),
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Num2, Modifiers::NONE, false, false);
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert!(matches!(
                bindings.key(key, modifiers, false, false),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
    }

    #[test]
    fn comma_i_reuses_once_and_keeps_native_text_and_global_return_free() {
        let mut bindings = Bindings::default();
        assert!(!bindings.reuse_pending());
        assert_eq!(
            bindings.key(Key::Comma, Modifiers::NONE, false, false),
            Some(Action::OfferInsert)
        );
        assert!(bindings.reuse_pending());
        assert!(
            bindings
                .pending_hint()
                .unwrap()
                .contains("i reuse Original")
        );
        assert_eq!(
            bindings.key(Key::I, Modifiers::NONE, false, false),
            Some(Action::Insert)
        );
        assert!(!bindings.reuse_pending());
        assert!(!allows_key_repeat(Key::I, Modifiers::NONE));
        assert_eq!(bindings.key(Key::I, Modifiers::NONE, false, false), None);
        assert!(matches!(
            keys(&mut bindings, &[Key::Num3, Key::Comma, Key::I]),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(Key::I, Modifiers::NONE, text, ime), None);
            assert!(bindings.pending().is_empty());
        }
        for modifiers in [
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
            Modifiers::MAC_CMD | Modifiers::COMMAND,
        ] {
            assert_eq!(bindings.key(Key::Enter, modifiers, false, false), None);
        }
    }

    fn keys(bindings: &mut Bindings, keys: &[Key]) -> Option<Action> {
        keys.iter().fold(None, |_, key| {
            bindings.key(*key, Modifiers::NONE, false, false)
        })
    }

    #[test]
    fn word_and_sentence_keys_move_and_compose_after_operators() {
        use deadpan_core::{SemanticMotion as M, SemanticSelector as S, SpeechObject as O};
        let count = |value| std::num::NonZeroU32::new(value).unwrap();
        let mut bindings = Bindings::default();
        assert_eq!(
            keys(&mut bindings, &[Key::Num3, Key::W]),
            Some(Action::Word {
                forward: true,
                end: false,
                count: 3
            })
        );
        assert_eq!(
            keys(&mut bindings, &[Key::E]),
            Some(Action::Word {
                forward: true,
                end: true,
                count: 1
            })
        );
        assert_eq!(
            bindings.key(Key::B, Modifiers::SHIFT, false, false),
            Some(Action::Sentence {
                forward: false,
                count: 1
            })
        );
        assert_eq!(
            keys(&mut bindings, &[Key::D, Key::Num2, Key::W]),
            Some(Action::Operator {
                cut: true,
                selector: S::Motion {
                    motion: M::Words {
                        forward: true,
                        count: count(2),
                        end: false
                    }
                }
            })
        );
        assert_eq!(
            keys(&mut bindings, &[Key::Y, Key::I, Key::S]),
            Some(Action::Operator {
                cut: false,
                selector: S::Speech {
                    object: O::InnerSentence
                }
            })
        );
        assert_eq!(
            keys(&mut bindings, &[Key::Num3, Key::R, Key::I, Key::W]),
            Some(Action::Repeat {
                selector: S::Speech {
                    object: O::InnerWord
                },
                plays: count(3)
            })
        );
        assert!(matches!(
            keys(&mut bindings, &[Key::D, Key::Num2, Key::I, Key::W]),
            Some(Action::Invalid(_))
        ));
        assert_eq!(
            keys(&mut bindings, &[Key::N]),
            Some(Action::SearchStep { forward: true })
        );
        assert_eq!(
            bindings.key(Key::N, Modifiers::SHIFT, false, false),
            Some(Action::SearchStep { forward: false })
        );
        assert!(bindings.pending().is_empty());
    }

    #[test]
    fn split_uses_a_single_unmodified_key_and_respects_native_text_entry() {
        let mut bindings = Bindings::default();
        assert_eq!(
            keys(&mut bindings, &[Key::S]),
            Some(Action::Edit(BeatEdit::Split))
        );
        assert!(!allows_key_repeat(Key::S, Modifiers::NONE));
        assert!(matches!(
            keys(&mut bindings, &[Key::Num3, Key::S]),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
        for (modifiers, text, ime) in [
            (Modifiers::NONE, true, false),
            (Modifiers::NONE, false, true),
            (Modifiers::SHIFT, false, false),
            (Modifiers::ALT, false, false),
            (Modifiers::COMMAND, false, false),
        ] {
            assert_eq!(bindings.key(Key::S, modifiers, text, ime), None);
        }
        assert!(matches!(
            keys(&mut bindings, &[Key::R, Key::S]),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
    }

    #[test]
    fn repeat_and_delete_operators_keep_counts_and_wait_without_a_timeout() {
        for (prefix, pending, result) in [
            (
                vec![Key::R],
                "r",
                Action::Repeat {
                    selector: deadpan_core::SemanticSelector::SelectedBeat,
                    plays: std::num::NonZeroU32::new(2).unwrap(),
                },
            ),
            (
                vec![Key::Num1, Key::R],
                "1r",
                Action::Repeat {
                    selector: deadpan_core::SemanticSelector::SelectedBeat,
                    plays: std::num::NonZeroU32::new(1).unwrap(),
                },
            ),
            (
                vec![Key::Num3, Key::R],
                "3r",
                Action::Repeat {
                    selector: deadpan_core::SemanticSelector::SelectedBeat,
                    plays: std::num::NonZeroU32::new(3).unwrap(),
                },
            ),
            (
                vec![Key::D],
                "d",
                Action::Operator {
                    cut: true,
                    selector: deadpan_core::SemanticSelector::SelectedBeat,
                },
            ),
            (
                vec![Key::Num1, Key::D],
                "1d",
                Action::Operator {
                    cut: true,
                    selector: deadpan_core::SemanticSelector::SelectedBeat,
                },
            ),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(keys(&mut bindings, &prefix), Some(Action::OfferInsert));
            for _ in 0..1_000 {
                assert_eq!(bindings.pending(), pending);
            }
            assert_eq!(
                keys(&mut bindings, &[*prefix.last().unwrap()]),
                Some(result)
            );
            assert!(bindings.pending().is_empty());
        }
    }

    #[test]
    fn operator_counts_and_selectors_accept_beat_objects_and_reject_invalid_forms() {
        for (prefix, object) in [
            (Key::I, deadpan_core::SemanticTextObject::InnerBeat),
            (Key::A, deadpan_core::SemanticTextObject::AroundBeat),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                keys(&mut bindings, &[Key::R, prefix, Key::B]),
                Some(Action::Repeat {
                    selector: deadpan_core::SemanticSelector::TextObject { object },
                    plays: std::num::NonZeroU32::new(2).unwrap(),
                })
            );
            assert!(bindings.pending().is_empty());
        }
        for sequence in [
            vec![Key::Num0, Key::R, Key::R],
            vec![Key::Num0, Key::D, Key::D],
            vec![Key::Num2, Key::D, Key::D],
            vec![Key::Num3, Key::R, Key::Num2, Key::L],
            vec![Key::R, Key::I, Key::O],
            vec![Key::R, Key::Num2, Key::I, Key::B],
            vec![Key::D, Key::O],
            vec![Key::R, Key::D],
        ] {
            let mut bindings = Bindings::default();
            assert!(
                matches!(keys(&mut bindings, &sequence), Some(Action::Invalid(_))),
                "{sequence:?}"
            );
            assert!(bindings.pending().is_empty());
        }
        let mut bindings = Bindings::default();
        keys(&mut bindings, &[Key::Num9; 30]);
        assert!(matches!(
            keys(&mut bindings, &[Key::R, Key::R]),
            Some(Action::Invalid(_))
        ));
    }

    #[test]
    fn pending_edits_cancel_for_text_ime_context_reset_and_escape() {
        for operator in [Key::R, Key::D] {
            for (text, ime) in [(true, false), (false, true), (true, true)] {
                let mut bindings = Bindings::default();
                keys(&mut bindings, &[Key::Num3, operator]);
                assert!(bindings.key(operator, Modifiers::NONE, text, ime).is_none());
                assert!(bindings.pending().is_empty());
            }
            let mut bindings = Bindings::default();
            keys(&mut bindings, &[operator]);
            assert_eq!(keys(&mut bindings, &[Key::Escape]), Some(Action::Escape));
            keys(&mut bindings, &[operator]);
            bindings.clear();
            assert_eq!(keys(&mut bindings, &[operator]), Some(Action::OfferInsert));
        }
    }

    #[test]
    fn held_keys_cannot_finish_an_operator_or_repeat_an_edit() {
        for key in [Key::R, Key::D, Key::U, Key::Enter, Key::Num3, Key::G] {
            assert!(!allows_key_repeat(key, Modifiers::NONE));
        }
        assert!(!allows_key_repeat(Key::R, Modifiers::CTRL));
        assert!(!allows_key_repeat(Key::Z, Modifiers::COMMAND));
        for key in [Key::H, Key::J, Key::K, Key::L, Key::ArrowDown] {
            assert!(allows_key_repeat(key, Modifiers::NONE));
            assert!(!allows_key_repeat(key, Modifiers::ALT));
        }
        let mut bindings = Bindings::default();
        keys(&mut bindings, &[Key::R]);
        assert!(!allows_key_repeat(Key::R, Modifiers::NONE));
        assert_eq!(bindings.pending(), "r");
        assert_eq!(
            keys(&mut bindings, &[Key::R]),
            Some(Action::Repeat {
                selector: deadpan_core::SemanticSelector::SelectedBeat,
                plays: std::num::NonZeroU32::new(2).unwrap(),
            })
        );
    }

    #[test]
    fn enter_and_escape_belong_to_composition_until_it_finishes() {
        for key in [Key::Enter, Key::Escape] {
            assert!(text_action(key, Modifiers::NONE, true, true).is_none());
            assert!(text_action(key, Modifiers::NONE, false, false).is_none());
            assert!(text_action(key, Modifiers::COMMAND, true, false).is_none());
        }
        assert_eq!(
            text_action(Key::Enter, Modifiers::NONE, true, false),
            Some(TextAction::Open)
        );
        assert_eq!(
            text_action(Key::Escape, Modifiers::NONE, true, false),
            Some(TextAction::Leave)
        );
        assert!(text_action(Key::Tab, Modifiers::NONE, true, false).is_none());
    }

    #[test]
    fn frame_and_beat_bindings_use_one_accumulated_count() {
        for (key, forward, beat) in [
            (Key::H, false, false),
            (Key::ArrowLeft, false, false),
            (Key::L, true, false),
            (Key::ArrowRight, true, false),
            (Key::J, true, true),
            (Key::ArrowDown, true, true),
            (Key::K, false, true),
            (Key::ArrowUp, false, true),
        ] {
            let mut bindings = Bindings::default();
            for digit in [Key::Num1, Key::Num2] {
                assert!(bindings.key(digit, Modifiers::NONE, false, false).is_none());
            }
            assert_eq!(bindings.pending(), "12");
            let expected = if beat {
                Action::Beat { forward, count: 12 }
            } else {
                Action::Step { forward, count: 12 }
            };
            assert_eq!(
                bindings.key(key, Modifiers::NONE, false, false),
                Some(expected)
            );
            assert!(bindings.pending().is_empty());
            assert_eq!(
                bindings.key(Key::L, Modifiers::NONE, false, false),
                Some(Action::Step {
                    forward: true,
                    count: 1
                })
            );
        }
    }

    #[test]
    fn counts_reject_overflow_and_zero_never_creates_a_zero_distance_motion() {
        let mut bindings = Bindings::default();
        for _ in 0..100 {
            bindings.key(Key::Num9, Modifiers::NONE, false, false);
        }
        assert!(bindings.pending().contains("overflow"));
        assert!(matches!(
            bindings.key(Key::H, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
        bindings.key(Key::Num0, Modifiers::NONE, false, false);
        assert_eq!(
            bindings.key(Key::L, Modifiers::NONE, false, false),
            Some(Action::Step {
                forward: true,
                count: 1
            })
        );
    }

    #[test]
    fn g_prefix_survives_idle_observation_without_a_clock_or_timeout() {
        let mut bindings = Bindings::default();
        bindings.key(Key::Num2, Modifiers::NONE, false, false);
        assert!(
            bindings
                .key(Key::G, Modifiers::NONE, false, false)
                .is_none()
        );
        // Rendering the status during arbitrarily many idle updates does not
        // consume the prefix. Binding state intentionally has no clock input.
        for _ in 0..10_000 {
            assert_eq!(bindings.pending(), "2g");
        }
        assert_eq!(
            bindings.key(Key::G, Modifiers::NONE, false, false),
            Some(Action::First)
        );
        assert!(bindings.pending().is_empty());
    }

    #[test]
    fn invalid_prefixes_escape_and_explicit_reset_discard_pending_input() {
        let mut bindings = Bindings::default();
        for invalid in [Key::H, Key::Num2, Key::A] {
            bindings.key(Key::G, Modifiers::NONE, false, false);
            assert!(
                bindings
                    .key(invalid, Modifiers::NONE, false, false)
                    .is_none()
            );
            assert!(bindings.pending().is_empty());
        }
        bindings.key(Key::Num3, Modifiers::NONE, false, false);
        bindings.key(Key::G, Modifiers::NONE, false, false);
        assert_eq!(
            bindings.key(Key::Escape, Modifiers::NONE, false, false),
            Some(Action::Escape)
        );
        assert!(bindings.pending().is_empty());
        bindings.key(Key::Num9, Modifiers::NONE, false, false);
        bindings.clear();
        assert_eq!(
            bindings.key(Key::J, Modifiers::NONE, false, false),
            Some(Action::Beat {
                forward: true,
                count: 1
            })
        );
    }

    #[test]
    fn endpoint_search_history_and_pane_actions_clear_counts() {
        for (key, modifiers, expected) in [
            (Key::Home, Modifiers::NONE, Action::First),
            (Key::End, Modifiers::NONE, Action::Last),
            (Key::G, Modifiers::SHIFT, Action::Last),
            (Key::Slash, Modifiers::NONE, Action::Search),
            (Key::Colon, Modifiers::NONE, Action::Command),
            (Key::U, Modifiers::NONE, Action::Undo),
            (Key::Tab, Modifiers::NONE, Action::Pane { reverse: false }),
            (Key::Tab, Modifiers::SHIFT, Action::Pane { reverse: true }),
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Num4, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(key, modifiers, false, false), Some(expected));
            assert!(bindings.pending().is_empty());
        }
    }

    #[test]
    fn logical_symbols_and_digits_do_not_assume_a_us_layout() {
        for modifiers in [
            Modifiers::NONE,
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::ALT | Modifiers::SHIFT,
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                bindings.key(Key::Colon, modifiers, false, false),
                Some(Action::Command)
            );
            assert_eq!(
                bindings.key(Key::Slash, modifiers, false, false),
                Some(Action::Search)
            );
            assert!(
                bindings
                    .key(Key::Semicolon, modifiers, false, false)
                    .is_none()
            );
        }
        // Logical digits can require Shift, while Option-number chords belong
        // to Kestrel's Desktop navigation and cannot become editor counts.
        for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
            let mut bindings = Bindings::default();
            assert_eq!(bindings.key(Key::Num3, modifiers, false, false), None);
            assert_eq!(bindings.pending(), "3");
            assert_eq!(
                bindings.key(Key::L, Modifiers::NONE, false, false),
                Some(Action::Step {
                    forward: true,
                    count: 3
                })
            );
        }
        for modifiers in [Modifiers::ALT, Modifiers::ALT | Modifiers::SHIFT] {
            for key in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5] {
                let mut bindings = Bindings::default();
                assert_eq!(bindings.key(key, modifiers, false, false), None);
                assert!(bindings.pending().is_empty());
                assert_eq!(
                    bindings.key(Key::L, Modifiers::NONE, false, false),
                    Some(Action::Step {
                        forward: true,
                        count: 1
                    })
                );
            }
        }
    }

    /// One press as egui-winit 0.36 delivers it on macOS: the logical key, or
    /// the physical position when egui cannot name the character, plus the
    /// immediate companion text.
    fn layout_press(
        bindings: &mut Bindings,
        key: Key,
        physical: Key,
        modifiers: Modifiers,
        text: &str,
    ) -> Option<Action> {
        bindings.route_event_with_logical_text(
            key,
            Some(physical),
            modifiers,
            false,
            false,
            false,
            true,
            EditSelection::None,
            Some(text),
        )
    }

    #[test]
    fn layout_companion_text_names_fallback_characters() {
        let step = |count| Action::Step {
            forward: true,
            count,
        };
        // AZERTY top row: unshifted symbols arrive as Num1..Num0 fallbacks and
        // never become counts. Shift produces the actual digit.
        for (key, text) in [
            (Key::Num1, "&"),
            (Key::Num2, "é"),
            (Key::Num5, "("),
            (Key::Num6, "§"),
            (Key::Num9, "ç"),
            (Key::Num0, "à"),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                layout_press(&mut bindings, key, key, Modifiers::NONE, text),
                None
            );
            assert!(bindings.pending().is_empty(), "{text}");
            assert_eq!(
                layout_press(&mut bindings, Key::L, Key::L, Modifiers::NONE, "l"),
                Some(step(1))
            );
        }
        let mut bindings = Bindings::default();
        layout_press(&mut bindings, Key::Num3, Key::Num3, Modifiers::SHIFT, "3");
        assert_eq!(bindings.pending(), "3");
        assert_eq!(
            layout_press(&mut bindings, Key::L, Key::L, Modifiers::NONE, "l"),
            Some(step(3))
        );
        // US Shift+3 is `#`, not a count; QWERTZ ö, ä, ü and ß are not the
        // Semicolon, Quote, bracket or Minus bindings at their positions.
        for (key, modifiers, text) in [
            (Key::Num3, Modifiers::SHIFT, "#"),
            (Key::Semicolon, Modifiers::NONE, "ö"),
            (Key::Quote, Modifiers::NONE, "ä"),
            (Key::OpenBracket, Modifiers::NONE, "ü"),
            (Key::Minus, Modifiers::NONE, "ß"),
            (Key::Comma, Modifiers::SHIFT, "<"),
            (Key::CloseBracket, Modifiers::NONE, "$"),
            (Key::OpenBracket, Modifiers::ALT, "“"),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(layout_press(&mut bindings, key, key, modifiers, text), None);
            assert!(bindings.pending().is_empty(), "{text}");
        }
        // Without companion text the delivered identity still decides.
        let mut bindings = Bindings::default();
        bindings.key(Key::Num3, Modifiers::SHIFT, false, false);
        assert_eq!(bindings.pending(), "3");
    }

    #[test]
    fn layout_companion_text_selects_quotes_brackets_and_macros() {
        // QWERTZ `"` is Shift+2 and AZERTY `"` is unshifted 3: both select a
        // register instead of starting a count.
        for (key, modifiers) in [(Key::Num2, Modifiers::SHIFT), (Key::Num3, Modifiers::NONE)] {
            let mut bindings = Bindings::default();
            assert_eq!(layout_press(&mut bindings, key, key, modifiers, "\""), None);
            assert_eq!(bindings.pending(), "\"");
        }
        // QWERTZ `'` is Shift+# (physical Backslash): a mark jump, not `"`.
        let mut bindings = Bindings::default();
        layout_press(
            &mut bindings,
            Key::Quote,
            Key::Backslash,
            Modifiers::SHIFT,
            "'",
        );
        assert_eq!(bindings.pending(), "'");
        // QWERTZ `]` is Option+6; AZERTY `]` is Shift+Option at physical Minus.
        for (physical, modifiers) in [
            (Key::Num6, Modifiers::ALT),
            (Key::Minus, Modifiers::ALT | Modifiers::SHIFT),
        ] {
            let mut bindings = Bindings::default();
            layout_press(&mut bindings, Key::CloseBracket, physical, modifiers, "]");
            assert_eq!(bindings.pending(), "]");
            assert_eq!(
                layout_press(&mut bindings, Key::P, Key::P, Modifiers::NONE, "p"),
                Some(Action::Pause {
                    forward: true,
                    count: 1
                })
            );
        }
        // Kestrel keeps Option+5 (QWERTZ `[`), Shift+Option+5 (AZERTY `[`) and
        // Option+L (QWERTZ `@`) even though the produced character is valid.
        for (key, physical, modifiers, text) in [
            (Key::OpenBracket, Key::Num5, Modifiers::ALT, "["),
            (
                Key::OpenBracket,
                Key::Num5,
                Modifiers::ALT | Modifiers::SHIFT,
                "[",
            ),
            (Key::L, Key::L, Modifiers::ALT, "@"),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                layout_press(&mut bindings, key, physical, modifiers, text),
                None
            );
            assert!(bindings.pending().is_empty(), "{text}");
        }
        // AZERTY `@` is unshifted at the top-left position.
        let mut bindings = Bindings::default();
        layout_press(
            &mut bindings,
            Key::Backtick,
            Key::Backtick,
            Modifiers::NONE,
            "@",
        );
        assert_eq!(bindings.pending(), "@");
        assert!(bindings.macro_pending());
    }

    #[test]
    fn non_latin_letters_fall_back_to_their_physical_positions() {
        let step = |forward, count| Some(Action::Step { forward, count });
        // Russian ЙЦУКЕН, Greek and Hebrew letters have no egui key, so
        // egui-winit delivers the physical position, which stands for the
        // Latin letter there: р on H is h, д / λ / ך on L is l.
        for (physical, modifiers, text, expected) in [
            (Key::H, Modifiers::NONE, "р", step(false, 1)),
            (Key::L, Modifiers::NONE, "д", step(true, 1)),
            (Key::H, Modifiers::NONE, "η", step(false, 1)),
            (Key::L, Modifiers::NONE, "λ", step(true, 1)),
            (Key::H, Modifiers::NONE, "י", step(false, 1)),
            (Key::L, Modifiers::NONE, "ך", step(true, 1)),
            (Key::G, Modifiers::SHIFT, "П", Some(Action::Last)),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                layout_press(&mut bindings, physical, physical, modifiers, text),
                expected,
                "{text}"
            );
        }
        // Counts and two-key paths compose: 3 then д is 3l, пп is gg.
        let mut bindings = Bindings::default();
        layout_press(&mut bindings, Key::Num3, Key::Num3, Modifiers::NONE, "3");
        assert_eq!(
            layout_press(&mut bindings, Key::L, Key::L, Modifiers::NONE, "д"),
            step(true, 3)
        );
        layout_press(&mut bindings, Key::G, Key::G, Modifiers::NONE, "п");
        assert_eq!(
            layout_press(&mut bindings, Key::G, Key::G, Modifiers::NONE, "п"),
            Some(Action::First)
        );
        // Russian Shift+3 types №: not a count. Latin-script letters at a
        // position stay inert (QWERTZ ö, Turkish ı at I, Polish ł at L).
        for (key, modifiers, text) in [
            (Key::Num3, Modifiers::SHIFT, "№"),
            (Key::Semicolon, Modifiers::NONE, "ö"),
            (Key::I, Modifiers::NONE, "ı"),
            (Key::L, Modifiers::ALT, "ł"),
            (Key::Semicolon, Modifiers::NONE, "΄"),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                layout_press(&mut bindings, key, key, modifiers, text),
                None,
                "{text}"
            );
            assert!(bindings.pending().is_empty(), "{text}");
        }
        // Option still refuses a letter, and Kestrel still owns Option+H even
        // when a non-Latin layout types a letter there.
        let mut bindings = Bindings::default();
        assert_eq!(
            layout_press(&mut bindings, Key::H, Key::H, Modifiers::ALT, "р"),
            None
        );
        assert!(keymap_config::non_latin_letter("ж"));
        assert!(!keymap_config::non_latin_letter("ß"));
        assert!(!keymap_config::non_latin_letter("жж"));
        assert!(!keymap_config::non_latin_letter("1"));
    }

    #[test]
    fn mode_routers_read_the_typed_character() {
        let none = Modifiers::NONE;
        let shift = Modifiers::SHIFT;
        // AZERTY: Shift+3 types 3 and is a plain digit; unshifted & é " ( _
        // at the digit positions name nothing.
        assert_eq!(
            mode_key(Key::Num3, shift, Some("3")),
            Some((Key::Num3, none))
        );
        for (key, text) in [
            (Key::Num1, "&"),
            (Key::Num2, "é"),
            (Key::Num3, "\""),
            (Key::Num5, "("),
            (Key::Num8, "_"),
        ] {
            assert_eq!(mode_key(key, none, Some(text)), None, "{text}");
        }
        // US Shift+3 is # and Shift+2 is @: neither is a digit.
        assert_eq!(mode_key(Key::Num3, shift, Some("#")), None);
        assert_eq!(mode_key(Key::Num2, shift, Some("@")), None);
        // Letters keep Shift; non-Latin letters use their position.
        assert_eq!(mode_key(Key::H, shift, Some("H")), Some((Key::H, shift)));
        assert_eq!(mode_key(Key::L, none, Some("д")), Some((Key::L, none)));
        assert_eq!(mode_key(Key::I, none, Some("ı")), None);
        assert_eq!(
            mode_key(Key::Quote, none, Some("'")),
            Some((Key::Quote, none))
        );
        // No companion text (Enter, arrows, held keys): unchanged.
        assert_eq!(mode_key(Key::Num3, shift, None), Some((Key::Num3, shift)));
        // Through the real routers: AZERTY Shift+3 counts in Place slice and
        // Camera, & does not; Trim and Slip take a Cyrillic л as l.
        let routed = |key, modifiers, text| {
            mode_key(key, modifiers, text).and_then(|(key, modifiers)| {
                splice::route_key(key, modifiers, false, true, false, false)
            })
        };
        assert_eq!(
            routed(Key::Num3, shift, Some("3")),
            Some(splice::SpliceKey::Count(3))
        );
        assert_eq!(routed(Key::Num1, none, Some("&")), None);
        let camera = |key, modifiers, text| {
            mode_key(key, modifiers, text)
                .and_then(|(key, modifiers)| route_camera_key(key, modifiers, false, false, false))
        };
        assert_eq!(
            camera(Key::Num3, shift, Some("3")),
            Some(camera::CameraKey::Digit(3))
        );
        assert_eq!(camera(Key::Num1, none, Some("&")), None);
        assert_eq!(
            mode_key(Key::L, none, Some("л")).and_then(|(key, modifiers)| {
                trim::route_key(key, modifiers, false, true, false, false)
            }),
            Some(trim::TrimKey::Nudge(1))
        );
        assert!(
            mode_key(Key::H, none, Some("р"))
                .and_then(|(key, modifiers)| {
                    slip::route_key(key, modifiers, false, true, false, false)
                })
                .is_some()
        );
        let correction = |key, text| {
            mode_key(key, none, Some(text)).and_then(|(key, modifiers)| {
                corrections::route_key(key, modifiers, false, false, false, false)
            })
        };
        assert_eq!(correction(Key::C, "ç"), None);
        assert_eq!(
            correction(Key::C, "c"),
            Some(corrections::CorrectionKey::EditText)
        );
        assert_eq!(
            correction(Key::L, "д"),
            Some(corrections::CorrectionKey::Next)
        );
        // Gain and room tone route only Enter, Space and Escape, which carry
        // no layout character to reinterpret.
        assert_eq!(
            mode_key(Key::Space, none, Some(" ")),
            Some((Key::Space, none))
        );
    }

    #[test]
    fn kestrel_blocked_layout_characters_teach_their_alternatives() {
        let bindings = Bindings::default();
        let alt = Modifiers::ALT;
        // QWERTZ Option+5 and AZERTY Shift+Option+5 type [.
        for modifiers in [alt, alt | Modifiers::SHIFT] {
            let notice = bindings
                .reserved_layout_notice(Key::OpenBracket, Some(Key::Num5), modifiers, Some("["))
                .unwrap();
            assert!(notice.contains("types [ on this layout"), "{notice}");
            assert!(notice.contains(":scope play N"), "{notice}");
            assert!(notice.contains("[p and [s have no command"), "{notice}");
            let mut routed = bindings.clone();
            assert_eq!(
                layout_press(&mut routed, Key::OpenBracket, Key::Num5, modifiers, "["),
                None
            );
        }
        let notice = bindings
            .reserved_layout_notice(Key::L, Some(Key::L), alt, Some("@"))
            .unwrap();
        assert!(notice.starts_with("Option+L types @"), "{notice}");
        assert!(notice.contains(":macro a"), "{notice}");
        // Not reserved, not a typed ASCII symbol (US Option+5 is ∞), or a
        // physical map: no notice.
        assert_eq!(
            bindings.reserved_layout_notice(Key::CloseBracket, Some(Key::Num6), alt, Some("]")),
            None
        );
        assert_eq!(
            bindings.reserved_layout_notice(Key::Num5, Some(Key::Num5), alt, Some("∞")),
            None
        );
        assert_eq!(
            bindings.reserved_layout_notice(Key::Num5, Some(Key::Num5), alt, None),
            None
        );
        let physical =
            Bindings::from_json(br#"{"version":1,"key_mode":"physical","bindings":[]}"#).unwrap();
        assert_eq!(
            physical.reserved_layout_notice(Key::OpenBracket, Some(Key::Num5), alt, Some("[")),
            None
        );
    }

    #[test]
    fn native_command_shortcuts_preserve_text_editing_history() {
        for command in [
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
            Modifiers::MAC_CMD | Modifiers::COMMAND,
            Modifiers::CTRL | Modifiers::COMMAND,
        ] {
            let mut bindings = Bindings::default();
            for text in [false, true] {
                assert_eq!(
                    bindings.key(Key::N, command | Modifiers::SHIFT, text, false),
                    Some(Action::NewFromUrl)
                );
                for (key, expected) in [
                    (Key::N, Action::New),
                    (Key::O, Action::Open),
                    (Key::I, Action::Import),
                ] {
                    let expected = if command.ctrl && matches!(key, Key::O | Key::I) {
                        (!text).then_some(Action::JumpHistory {
                            forward: key == Key::I,
                        })
                    } else {
                        Some(expected)
                    };
                    assert_eq!(bindings.key(key, command, text, false), expected);
                }
            }
            for (key, modifiers, expected) in [
                (Key::Z, command, Action::Undo),
                (Key::Z, command | Modifiers::SHIFT, Action::Redo),
            ] {
                assert_eq!(bindings.key(key, modifiers, false, false), Some(expected));
                assert!(bindings.key(key, modifiers, true, false).is_none());
            }
            for key in [Key::A, Key::C, Key::V, Key::X] {
                assert!(bindings.key(key, command, true, false).is_none());
            }
        }
    }

    #[test]
    fn control_r_is_redo_only_outside_text_and_composition() {
        for modifiers in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::COMMAND] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Num4, Modifiers::NONE, false, false);
            assert_eq!(
                bindings.key(Key::R, modifiers, false, false),
                Some(Action::Redo)
            );
            assert!(bindings.pending().is_empty());
            assert!(bindings.key(Key::R, modifiers, true, false).is_none());
            assert!(bindings.key(Key::R, modifiers, false, true).is_none());
        }
    }

    #[test]
    fn text_and_ime_discard_pending_normal_mode_bindings() {
        let normal_keys = [
            Key::H,
            Key::L,
            Key::J,
            Key::K,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::ArrowUp,
            Key::ArrowDown,
            Key::Home,
            Key::End,
            Key::G,
            Key::U,
            Key::R,
            Key::D,
            Key::Slash,
            Key::Colon,
            Key::Tab,
            Key::Escape,
            Key::Num3,
        ];
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            for key in normal_keys {
                let mut bindings = Bindings::default();
                bindings.key(Key::Num3, Modifiers::NONE, false, false);
                bindings.key(Key::G, Modifiers::NONE, false, false);
                assert!(bindings.key(key, Modifiers::NONE, text, ime).is_none());
                assert!(bindings.pending().is_empty());
            }
        }
        for key in [Key::N, Key::O, Key::I, Key::Z, Key::Enter] {
            let mut bindings = Bindings::default();
            assert!(bindings.key(key, Modifiers::COMMAND, false, true).is_none());
        }
    }

    #[test]
    fn modified_navigation_and_reserved_shortcuts_do_not_edit() {
        for key in [
            Key::H,
            Key::J,
            Key::K,
            Key::L,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::Home,
            Key::End,
        ] {
            for modifiers in [
                Modifiers::SHIFT,
                Modifiers::ALT,
                Modifiers::CTRL,
                Modifiers::COMMAND,
                Modifiers::MAC_CMD,
            ] {
                let mut bindings = Bindings::default();
                bindings.key(Key::Num3, Modifiers::NONE, false, false);
                assert!(bindings.key(key, modifiers, false, false).is_none());
                assert!(bindings.pending().is_empty());
            }
        }
        for modifiers in [
            Modifiers::COMMAND | Modifiers::ALT,
            Modifiers::MAC_CMD | Modifiers::COMMAND | Modifiers::CTRL,
        ] {
            for key in [Key::N, Key::O, Key::I, Key::Z, Key::R, Key::Tab, Key::Colon] {
                assert!(
                    Bindings::default()
                        .key(key, modifiers, false, false)
                        .is_none()
                );
            }
        }
    }

    #[test]
    fn boundaries_include_the_end_and_saturate_without_wrapping() {
        assert_eq!(boundary_step(0, 0, true, 1), 0);
        assert_eq!(boundary_step(0, 1, true, 1), 1);
        assert_eq!(boundary_step(0, 120, false, 20), 0);
        assert_eq!(boundary_step(119, 120, true, 1), 120);
        assert_eq!(boundary_step(120, 120, true, 1), 120);
        assert_eq!(boundary_step(120, 120, false, 1), 119);
        assert_eq!(boundary_step(19, 120, true, 12), 31);
        assert_eq!(boundary_step(19, 120, false, 12), 7);
        assert_eq!(boundary_step(u64::MAX - 1, u64::MAX, true, 2), u64::MAX);
        assert_eq!(boundary_step(u64::MAX, 120, true, u32::MAX), 120);
    }

    #[test]
    fn pane_order_cycles_both_directions_and_reverses_exactly() {
        assert_eq!(Pane::default(), Pane::Viewer);
        assert_eq!(Pane::Sources.cycle(false), Pane::Viewer);
        assert_eq!(Pane::Viewer.cycle(false), Pane::Sequence);
        assert_eq!(Pane::Sequence.cycle(false), Pane::Sounds);
        assert_eq!(Pane::Sounds.cycle(false), Pane::Sources);
        for pane in [Pane::Sources, Pane::Viewer, Pane::Sequence, Pane::Sounds] {
            assert_eq!(pane.cycle(false).cycle(true), pane);
            assert_eq!(pane.cycle(true).cycle(false), pane);
            assert_eq!(
                pane.cycle(false).cycle(false).cycle(false).cycle(false),
                pane
            );
        }
    }

    #[test]
    fn inspector_participates_only_while_visible() {
        let visible = [
            Pane::Sources,
            Pane::Viewer,
            Pane::Inspector,
            Pane::Sequence,
            Pane::Sounds,
        ];
        for (index, pane) in visible.into_iter().enumerate() {
            assert_eq!(
                pane.cycle_visible(false, true),
                visible[(index + 1) % visible.len()]
            );
            assert_eq!(
                pane.cycle_visible(false, true).cycle_visible(true, true),
                pane
            );
        }
        for pane in [Pane::Sources, Pane::Viewer, Pane::Sequence, Pane::Sounds] {
            for reverse in [false, true] {
                assert_ne!(pane.cycle_visible(reverse, false), Pane::Inspector);
                assert_eq!(pane.cycle_visible(reverse, false), pane.cycle(reverse));
            }
        }
        assert_eq!(Pane::Inspector.cycle_visible(false, false), Pane::Viewer);
        assert_eq!(Pane::Inspector.visible(false), Pane::Viewer);
        assert_eq!(Pane::Inspector.visible(true), Pane::Inspector);
        assert_eq!(Pane::Sequence.visible(false), Pane::Sequence);
        assert_eq!(Pane::Sounds.visible(false), Pane::Sounds);
        assert_eq!(Pane::Sounds.visible(true), Pane::Sounds);
    }

    #[test]
    fn tab_skips_panes_the_layout_does_not_draw() {
        // The start screen draws no Placed sounds or Inspector. Its focus
        // cycle must not name them: AccessKit requires the focused control.
        let start = |pane: Pane| !matches!(pane, Pane::Sounds | Pane::Inspector);
        assert_eq!(
            Pane::Sequence.cycle_available(false, false, start),
            Pane::Sources
        );
        assert_eq!(
            Pane::Sources.cycle_available(true, false, start),
            Pane::Sequence
        );
        assert_eq!(
            Pane::Viewer.cycle_available(false, true, start),
            Pane::Sequence
        );
        let all = |_: Pane| true;
        for pane in [Pane::Sources, Pane::Viewer, Pane::Sequence, Pane::Sounds] {
            for reverse in [false, true] {
                assert_eq!(
                    pane.cycle_available(reverse, true, all),
                    pane.cycle_visible(reverse, true)
                );
            }
        }
        assert_eq!(
            Pane::Sources.cycle_available(false, true, |_| false),
            Pane::Viewer
        );
    }

    #[test]
    fn inspector_enter_is_distinct_from_native_text_and_composition() {
        assert!(inspector_parameter_key(
            Key::Enter,
            Modifiers::NONE,
            Pane::Inspector,
            false,
            false
        ));
        for (pane, text, ime) in [
            (Pane::Viewer, false, false),
            (Pane::Sequence, false, false),
            (Pane::Inspector, true, false),
            (Pane::Inspector, false, true),
        ] {
            assert!(!inspector_parameter_key(
                Key::Enter,
                Modifiers::NONE,
                pane,
                text,
                ime
            ));
        }
        assert!(!inspector_parameter_key(
            Key::Enter,
            Modifiers::COMMAND,
            Pane::Inspector,
            false,
            false
        ));
        assert!(!inspector_parameter_key(
            Key::Escape,
            Modifiers::NONE,
            Pane::Inspector,
            false,
            false
        ));
    }

    #[test]
    fn logical_question_mark_opens_help_and_remains_text_during_editing() {
        for modifiers in [Modifiers::NONE, Modifiers::SHIFT, Modifiers::ALT] {
            let mut bindings = Bindings::default();
            assert_eq!(
                bindings.key(Key::Questionmark, modifiers, false, false),
                Some(Action::Help)
            );
            assert_eq!(
                bindings.key(Key::Questionmark, modifiers, true, false),
                None
            );
            assert_eq!(
                bindings.key(Key::Questionmark, modifiers, false, true),
                None
            );
            assert!(bindings.pending().is_empty());
        }
    }

    #[test]
    fn comma_s_places_once_without_counts_or_native_input_capture() {
        let mut bindings = Bindings::default();
        assert_eq!(
            bindings.key(Key::Comma, Modifiers::NONE, false, false),
            Some(Action::OfferInsert)
        );
        assert!(bindings.pending_hint().unwrap().contains("s place sound"));
        assert_eq!(
            bindings.key(Key::S, Modifiers::NONE, false, false),
            Some(Action::Sound(SoundAction::Place))
        );
        assert!(bindings.pending().is_empty());
        // The UI's existing repeat filter rejects a held S before this router;
        // a later distinct S press retains its ordinary Split meaning.
        assert!(!allows_key_repeat(Key::S, Modifiers::NONE));
        assert!(!allows_key_repeat(Key::Comma, Modifiers::NONE));
        assert_eq!(
            bindings.key(Key::S, Modifiers::NONE, false, false),
            Some(Action::Edit(BeatEdit::Split))
        );
        for count in [Key::Num0, Key::Num1, Key::Num3] {
            assert!(matches!(
                keys(&mut bindings, &[count, Key::Comma, Key::S]),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
        for (modifiers, text, ime) in [
            (Modifiers::NONE, true, false),
            (Modifiers::NONE, false, true),
            (Modifiers::NONE, true, true),
            (Modifiers::SHIFT, false, false),
            (Modifiers::ALT, false, false),
            (Modifiers::CTRL, false, false),
            (Modifiers::COMMAND, false, false),
            (Modifiers::MAC_CMD, false, false),
        ] {
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(Key::S, modifiers, text, ime), None);
            assert!(bindings.pending().is_empty());
        }
        for modifiers in [
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::ALT | Modifiers::SHIFT,
        ] {
            bindings.key(Key::Comma, modifiers, false, false);
            assert_eq!(
                bindings.key(Key::S, Modifiers::NONE, false, false),
                Some(Action::Sound(SoundAction::Place))
            );
        }
    }

    #[test]
    fn sound_gain_counts_are_checked_and_never_complete_another_prefix() {
        let mut bindings = Bindings::default();
        for (count, key, expected) in [
            ("", Key::Plus, 3000),
            ("", Key::Minus, -3000),
            ("2", Key::Plus, 6000),
            ("3", Key::Minus, -9000),
            ("715827", Key::Plus, 2147481000),
            ("715827", Key::Minus, -2147481000),
        ] {
            for digit in count.bytes() {
                bindings.key(
                    DIGITS[usize::from(digit - b'0')].0,
                    Modifiers::NONE,
                    false,
                    false,
                );
            }
            assert_eq!(
                bindings.key(key, Modifiers::NONE, false, false),
                Some(Action::GainStep(expected))
            );
            assert!(bindings.pending().is_empty());
        }
        for count in ["0", "715828", "4294967295", "4294967296"] {
            for key in [Key::Plus, Key::Minus] {
                for digit in count.bytes() {
                    bindings.key(
                        DIGITS[usize::from(digit - b'0')].0,
                        Modifiers::NONE,
                        false,
                        false,
                    );
                }
                assert!(
                    matches!(
                        bindings.key(key, Modifiers::NONE, false, false),
                        Some(Action::Invalid(_))
                    ),
                    "{count} {key:?}"
                );
                assert!(bindings.pending().is_empty());
            }
        }
        for prefix in [Key::G, Key::R, Key::D, Key::Comma] {
            for key in [Key::Plus, Key::Minus] {
                bindings.key(prefix, Modifiers::NONE, false, false);
                assert!(matches!(
                    bindings.key(key, Modifiers::NONE, false, false),
                    Some(Action::Invalid(_))
                ));
                assert!(bindings.pending().is_empty());
            }
        }
    }

    #[test]
    fn sound_gain_uses_logical_symbols_and_preserves_text_and_reserved_modifiers() {
        for (key, delta) in [(Key::Plus, 3000), (Key::Minus, -3000)] {
            for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                let expected = if key == Key::Minus && modifiers.shift {
                    None
                } else {
                    Some(Action::GainStep(delta))
                };
                assert_eq!(
                    Bindings::default().key(key, modifiers, false, false),
                    expected
                );
                assert!(!allows_key_repeat(key, modifiers));
            }
            for (text, ime) in [(true, false), (false, true), (true, true)] {
                let mut bindings = Bindings::default();
                bindings.key(Key::Num3, Modifiers::NONE, false, false);
                assert_eq!(bindings.key(key, Modifiers::SHIFT, text, ime), None);
                assert!(bindings.pending().is_empty());
            }
            for modifiers in [
                Modifiers::ALT,
                Modifiers::ALT | Modifiers::SHIFT,
                Modifiers::CTRL,
                Modifiers::COMMAND,
                Modifiers::MAC_CMD,
            ] {
                let mut bindings = Bindings::default();
                bindings.key(Key::Num3, Modifiers::NONE, false, false);
                assert_eq!(bindings.key(key, modifiers, false, false), None);
                assert!(bindings.pending().is_empty());
            }
        }
        for modifiers in [Modifiers::NONE, Modifiers::SHIFT, Modifiers::ALT] {
            assert_eq!(
                Bindings::default().key(Key::Equals, modifiers, false, false),
                None,
                "a US physical key position cannot stand in for logical Plus"
            );
        }
        let mut bindings = Bindings::default();
        bindings.key(Key::Num3, Modifiers::NONE, false, false);
        assert_eq!(
            bindings.key(Key::Minus, Modifiers::SHIFT, false, false),
            None,
            "underscore fallback cannot change sound gain"
        );
        assert!(bindings.pending().is_empty());
    }
}
