//! One registry of user actions. Each action is described once: its id and
//! name, keys, command-line verbs and usage, the contexts where it applies,
//! macro and dot behaviour, its headless equivalent and its help text.
//!
//! Configurable editor keys are named by [`BindingId`], so the Keys sheet,
//! footer teaching and generated reference show whatever path the binding
//! trie currently holds, including a personal keymap. Fixed mode keys name
//! their router chords; tests prove those chords and the routers agree.
//! The command parser admits exactly the registry's verbs and completion
//! lists their usages.

use std::sync::LazyLock;

use eframe::egui::{Key, Modifiers};

use super::BindingId;

mod entries;
pub use entries::SPECS;

#[cfg(test)]
mod baseline;
#[cfg(test)]
mod tests;

/// Where an action applies. Editor contexts are where Help opens; the others
/// are temporary modes and panels that own the keyboard while open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contexts(u32);

impl Contexts {
    pub const ORIGINAL: Self = Self(1);
    /// Your edit with no Visual selection (Normal).
    pub const EDIT: Self = Self(1 << 1);
    /// Your edit with an active, finished or object Visual selection.
    pub const VISUAL: Self = Self(1 << 2);
    /// The sound catalog in the Original/sounds pane.
    pub const CATALOG: Self = Self(1 << 3);
    pub const PLACED: Self = Self(1 << 4);
    /// The scoped inspector inside Repeat or Retime contents.
    pub const CONTENTS: Self = Self(1 << 5);
    pub const CAMERA: Self = Self(1 << 6);
    pub const TRIM: Self = Self(1 << 7);
    pub const SLIP: Self = Self(1 << 8);
    pub const SPLICE: Self = Self(1 << 9);
    pub const ROOM_TONE: Self = Self(1 << 10);
    pub const GAIN: Self = Self(1 << 11);
    pub const CORRECTIONS: Self = Self(1 << 12);
    pub const MARKS: Self = Self(1 << 13);
    pub const JOBS: Self = Self(1 << 14);
    pub const STORAGE: Self = Self(1 << 15);
    pub const MODELS: Self = Self(1 << 16);
    pub const HELP: Self = Self(1 << 17);
    pub const YOUTUBE: Self = Self(1 << 18);
    /// No project is open (the start window).
    pub const NO_PROJECT: Self = Self(1 << 19);
    pub const YOUR_EDIT: Self = Self::EDIT.or(Self::VISUAL);
    pub const TIMELINE: Self = Self::ORIGINAL.or(Self::YOUR_EDIT);
    /// Every editor context: the command line, Help and native menus.
    pub const EDITOR: Self = Self::TIMELINE
        .or(Self::CATALOG)
        .or(Self::PLACED)
        .or(Self::CONTENTS);
    pub const ANYWHERE: Self = Self::EDITOR.or(Self::NO_PROJECT);

    const NAMES: [(Self, &'static str); 20] = [
        (Self::ORIGINAL, "Original"),
        (Self::EDIT, "Your edit"),
        (Self::VISUAL, "Visual"),
        (Self::CATALOG, "Sound catalog"),
        (Self::PLACED, "Placed sounds"),
        (Self::CONTENTS, "Repeat contents"),
        (Self::CAMERA, "Camera"),
        (Self::TRIM, "Trim"),
        (Self::SLIP, "Slip"),
        (Self::SPLICE, "Place slice"),
        (Self::ROOM_TONE, "Room tone"),
        (Self::GAIN, "Gain draft"),
        (Self::CORRECTIONS, "Corrections"),
        (Self::MARKS, "Marks"),
        (Self::JOBS, "Jobs"),
        (Self::STORAGE, "Storage"),
        (Self::MODELS, "Models"),
        (Self::HELP, "Keys"),
        (Self::YOUTUBE, "YouTube URL"),
        (Self::NO_PROJECT, "No project"),
    ];

    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[cfg(test)]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Readable names, collapsing the common unions.
    pub fn label(self) -> String {
        if self.contains(Self::ANYWHERE) {
            return "Anywhere".into();
        }
        let mut remaining = self;
        let mut names = Vec::new();
        if remaining.contains(Self::EDITOR) {
            names.push("Every editor context");
            remaining = Self(remaining.0 & !Self::EDITOR.0);
        } else if remaining.contains(Self::YOUR_EDIT) {
            names.push("Your edit (Normal and Visual)");
            remaining = Self(remaining.0 & !Self::YOUR_EDIT.0);
        }
        for (flag, name) in Self::NAMES {
            if remaining.contains(flag) {
                names.push(name);
            }
        }
        names.join(", ")
    }
}

/// Keyboard owners with their own fixed routers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Camera,
    Trim,
    Slip,
    Splice,
    RoomTone,
    Gain,
    Corrections,
    Marks,
    Jobs,
    Storage,
    Help,
}

impl Mode {
    #[cfg(test)]
    pub const ALL: [Self; 11] = [
        Self::Camera,
        Self::Trim,
        Self::Slip,
        Self::Splice,
        Self::RoomTone,
        Self::Gain,
        Self::Corrections,
        Self::Marks,
        Self::Jobs,
        Self::Storage,
        Self::Help,
    ];

    #[cfg(test)]
    pub fn context(self) -> Contexts {
        match self {
            Self::Camera => Contexts::CAMERA,
            Self::Trim => Contexts::TRIM,
            Self::Slip => Contexts::SLIP,
            Self::Splice => Contexts::SPLICE,
            Self::RoomTone => Contexts::ROOM_TONE,
            Self::Gain => Contexts::GAIN,
            Self::Corrections => Contexts::CORRECTIONS,
            Self::Marks => Contexts::MARKS,
            Self::Jobs => Contexts::JOBS,
            Self::Storage => Contexts::STORAGE,
            Self::Help => Contexts::HELP,
        }
    }
}

/// The modifier state a fixed chord accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mods {
    /// No modifier.
    Plain,
    /// Shift alone.
    Shift,
    /// No modifier or Shift alone.
    PlainOrShift,
    /// No modifier, or egui's `shift_only`, which also admits Control.
    LooseShift,
    /// Control alone.
    Ctrl,
    /// A native ⌘ chord without Shift (`navigation::native_command`).
    Cmd,
    /// A native ⌘ chord with Shift.
    CmdShift,
}

impl Mods {
    pub fn accepts(self, modifiers: Modifiers) -> bool {
        match self {
            Self::Plain => modifiers.is_none(),
            Self::Shift => modifiers == Modifiers::SHIFT,
            Self::PlainOrShift => modifiers.is_none() || modifiers == Modifiers::SHIFT,
            Self::LooseShift => modifiers.is_none() || modifiers.shift_only(),
            Self::Ctrl => modifiers == Modifiers::CTRL,
            Self::Cmd => super::native_command(modifiers) && !modifiers.shift,
            Self::CmdShift => super::native_command(modifiers) && modifiers.shift,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub mods: Mods,
}

pub const fn chord(key: Key, mods: Mods) -> Chord {
    Chord { key, mods }
}

/// How an action is reached from the keyboard.
#[derive(Clone, Copy, Debug)]
pub enum Keys {
    /// A configurable Normal/Visual/Original path in the binding trie.
    Editor(BindingId),
    /// A fixed native shortcut or control key outside the trie, described.
    Native(&'static str),
    /// A fixed chord of a mode router, with the label its footer and Help show.
    Mode {
        mode: Mode,
        label: &'static str,
        chords: &'static [Chord],
    },
}

/// One command-line verb and the usage completion shows for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verb {
    pub verb: &'static str,
    pub usage: &'static str,
}

pub const fn verb(verb: &'static str, usage: &'static str) -> Verb {
    Verb { verb, usage }
}

impl Verb {
    /// The usage without its trailing explanation: `:hold 0.5s [video=black]`.
    pub fn syntax(&self) -> &'static str {
        self.usage.split("  ").next().unwrap_or(self.usage)
    }
}

/// Macro and dot-repeat behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replay {
    /// Writes nothing and is not recorded: views, panels, audition, history.
    Ignored,
    /// Recorded as a motion or selection instruction; `.` does not repeat it.
    Motion,
    /// A recorded edit; `.` does not repeat it.
    Recorded,
    /// A recorded edit that `.` repeats at the current target.
    RecordedAndDot,
    /// Changes the project but is not recorded and `.` does not repeat it.
    NotRecorded,
}

impl Replay {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ignored => "not recorded",
            Self::Motion => "recorded as a motion",
            Self::Recorded => "recorded in macros",
            Self::RecordedAndDot => "recorded; . repeats",
            Self::NotRecorded => "edits; not recorded",
        }
    }
}

/// Headless parity status, as in [PARITY](../../../../docs/PARITY.md).
/// Every action is either reachable headlessly through the same typed path
/// or has no project write to reach; a new action that is neither needs a
/// headless form before it ships.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parity {
    Equivalent,
    GuiOnly,
}

impl Parity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Equivalent => "Equivalent",
            Self::GuiOnly => "GUI-only",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Headless {
    pub parity: Parity,
    /// The headless request or subcommand, or why none is needed.
    pub form: &'static str,
    /// The PARITY.md section anchor.
    pub anchor: &'static str,
}

/// Help sections, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Start,
    Move,
    Select,
    CopyCut,
    Structure,
    Repeat,
    Pauses,
    Framing,
    Audio,
    Sounds,
    Ai,
    Gags,
    Registers,
    Macros,
    Project,
    Camera,
    Trim,
    Slip,
    Splice,
    RoomTone,
    GainDraft,
    Corrections,
    Marks,
    Jobs,
    Storage,
    Models,
    Help,
}

impl Section {
    pub const ALL: [Self; 27] = [
        Self::Start,
        Self::Move,
        Self::Select,
        Self::CopyCut,
        Self::Structure,
        Self::Repeat,
        Self::Pauses,
        Self::Framing,
        Self::Audio,
        Self::Sounds,
        Self::Ai,
        Self::Gags,
        Self::Registers,
        Self::Macros,
        Self::Project,
        Self::Camera,
        Self::Trim,
        Self::Slip,
        Self::Splice,
        Self::RoomTone,
        Self::GainDraft,
        Self::Corrections,
        Self::Marks,
        Self::Jobs,
        Self::Storage,
        Self::Models,
        Self::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Start => "START & VIEW",
            Self::Move => "MOVE",
            Self::Select => "SELECT",
            Self::CopyCut => "COPY, CUT & PASTE",
            Self::Structure => "RESHAPE THE SELECTED BEAT",
            Self::Repeat => "REPEAT & SPEED",
            Self::Pauses => "PAUSES & PUNCTUATION",
            Self::Framing => "FRAMING",
            Self::Audio => "SOUND OF A BEAT",
            Self::Sounds => "PLACED SOUNDS",
            Self::Ai => "AI PAUSES",
            Self::Gags => "GAGS, CAPTIONS & CUTAWAYS",
            Self::Registers => "REGISTERS",
            Self::Macros => "SEMANTIC MACROS & DOT",
            Self::Project => "PROJECT, RENDER & PANELS",
            Self::Camera => "CAMERA MODE",
            Self::Trim => "TRIM MODE",
            Self::Slip => "SLIP PREVIEW",
            Self::Splice => "PLACE SLICE",
            Self::RoomTone => "ROOM TONE SHEET",
            Self::GainDraft => "GAIN DRAFT",
            Self::Corrections => "TRANSCRIPT CORRECTIONS",
            Self::Marks => "MARKS PANEL",
            Self::Jobs => "JOBS PANEL",
            Self::Storage => "STORAGE PANEL",
            Self::Models => "MODELS PANEL",
            Self::Help => "THIS REFERENCE",
        }
    }
}

/// One user action.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    /// Stable id. A configurable action uses its keymap action name.
    pub id: &'static str,
    pub name: &'static str,
    pub section: Section,
    pub keys: &'static [Keys],
    pub commands: &'static [Verb],
    pub contexts: Contexts,
    pub replay: Replay,
    pub headless: Headless,
    /// Help text. `{binding.id}` names every configured path of that
    /// binding, `{binding.id!}` only its primary path.
    pub help: &'static str,
}

impl Spec {
    #[cfg(test)]
    pub fn editor_keys(&self) -> impl Iterator<Item = BindingId> + '_ {
        self.keys.iter().filter_map(|keys| match keys {
            Keys::Editor(id) => Some(*id),
            _ => None,
        })
    }
}

/// Looks up a configured binding's labels. The app supplies the live map; the
/// generated reference uses the shipped logical map.
pub trait KeyLabels {
    /// Every alias, joined with " / ". Empty when the action has no path.
    fn all(&self, id: BindingId) -> String;
    /// The primary path, or empty.
    fn primary(&self, id: BindingId) -> String;
}

impl KeyLabels for super::Bindings {
    fn all(&self, id: BindingId) -> String {
        self.key_labels(id)
    }
    fn primary(&self, id: BindingId) -> String {
        self.key_label(id)
    }
}

impl BindingId {
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == name)
    }
}

/// Substitute `{binding.id}` and `{binding.id!}` placeholders.
pub fn render(template: &str, labels: &dyn KeyLabels) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        let (name, primary) = match name.strip_suffix('!') {
            Some(name) => (name, true),
            None => (name, false),
        };
        match BindingId::from_name(name) {
            Some(id) => {
                let label = if primary {
                    labels.primary(id)
                } else {
                    labels.all(id)
                };
                if label.is_empty() {
                    // A configured map may leave an action without a path.
                    out.push_str(&format!("(unbound {})", id.as_str()));
                } else {
                    out.push_str(&label);
                }
            }
            None => {
                out.push('{');
                out.push_str(&after[..=end]);
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Placeholders a template names that are not binding ids.
#[cfg(test)]
pub fn unknown_placeholders(template: &str) -> Vec<String> {
    let mut unknown = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            unknown.push(after.to_owned());
            break;
        };
        let name = after[..end].trim_end_matches('!');
        if BindingId::from_name(name).is_none() {
            unknown.push(name.to_owned());
        }
        rest = &after[end + 1..];
    }
    unknown
}

/// The keycap text for an action: its key paths, then its verbs.
pub fn key_text(spec: &Spec, labels: &dyn KeyLabels) -> String {
    let mut parts: Vec<String> = Vec::new();
    for keys in spec.keys {
        let label = match keys {
            Keys::Editor(id) => labels.all(*id),
            Keys::Native(label) => (*label).to_owned(),
            Keys::Mode { label, .. } => (*label).to_owned(),
        };
        if !label.is_empty() && !parts.contains(&label) {
            parts.push(label);
        }
    }
    for command in spec.commands {
        let syntax = command.syntax().to_owned();
        if !parts.contains(&syntax) {
            parts.push(syntax);
        }
    }
    parts.join(" · ")
}

/// Every verb with its usage, sorted by verb: the parser's admission table.
pub static VERBS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    let mut verbs: Vec<_> = SPECS
        .iter()
        .flat_map(|spec| spec.commands.iter().map(|verb| (verb.verb, verb.usage)))
        .collect();
    verbs.sort_unstable_by_key(|(verb, _)| *verb);
    verbs
});

pub fn by_id(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.id == id)
}

/// The label a mode footer or button shows for a fixed mode action.
pub fn mode_label(id: &str) -> &'static str {
    mode_label_at(id, 0)
}

/// The label of an action's `index`-th fixed mode key (`camera.region` has
/// `n` then `c`). Painting never panics: an unknown id shows `?`, and a test
/// covers every id the interface uses.
pub fn mode_label_at(id: &str, index: usize) -> &'static str {
    by_id(id)
        .and_then(|spec| {
            spec.keys
                .iter()
                .filter_map(|keys| match keys {
                    Keys::Mode { label, .. } => Some(*label),
                    _ => None,
                })
                .nth(index)
        })
        .unwrap_or("?")
}

/// The declared chords of one mode, with the action that owns them.
pub fn mode_chords(mode: Mode) -> impl Iterator<Item = (&'static Spec, Chord)> {
    SPECS.iter().flat_map(move |spec| {
        spec.keys
            .iter()
            .filter_map(move |keys| match keys {
                Keys::Mode {
                    mode: owner,
                    chords,
                    ..
                } if *owner == mode => Some(chords.iter().map(move |chord| (spec, *chord))),
                _ => None,
            })
            .flatten()
    })
}

/// How well an action matches a Help search. `None` hides it. Every
/// whitespace-separated term must match; a term that equals a key path
/// (case-sensitive, so `G` is not `gg`) or names a verb ranks first. A query
/// of only terms shorter than three characters matches keys and exact verbs,
/// never prose, so `dd` does not also list every action whose text contains
/// "add".
pub fn search_rank(spec: &Spec, keys: &str, help: &str, query: &str) -> Option<u8> {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Some(3);
    }
    let haystack = format!(
        "{} {} {} {} {} {}",
        spec.name,
        spec.id,
        keys,
        help,
        spec.contexts.label(),
        spec.commands
            .iter()
            .map(|command| command.usage)
            .collect::<Vec<_>>()
            .join(" ")
    )
    .to_lowercase();
    // A query of only short terms (`dd`, `s`, `G`) is a key lookup; once any
    // term has three characters, short words such as "a" match prose too.
    let key_like = terms.iter().all(|term| term.chars().count() < 3);
    let mut best = 3;
    for term in terms {
        let verb_term = term.strip_prefix(':');
        let exact_key = keys
            .split(" · ")
            .flat_map(|part| part.split(" / "))
            .any(|part| part == term);
        let prose = !key_like;
        // `:` alone lists every verb; `:h…` matches verb prefixes; a bare
        // term matches a verb prefix only from three characters, so `s`
        // finds the key `s` rather than every verb starting with s.
        let wanted = verb_term.unwrap_or(term).to_ascii_lowercase();
        let verb = spec.commands.iter().any(|command| match verb_term {
            Some(_) => command.verb.starts_with(&wanted),
            None if term.chars().count() >= 3 => command.verb.starts_with(&wanted),
            None => command.verb == wanted,
        });
        let rank = if exact_key {
            0
        } else if verb {
            1
        } else if !prose {
            return None;
        } else if spec.name.to_lowercase().contains(&term.to_lowercase()) {
            2
        } else if verb_term.is_none() && haystack.contains(&term.to_lowercase()) {
            3
        } else {
            return None;
        };
        best = best.min(rank);
    }
    Some(best)
}

/// The generated Markdown reference with the shipped logical keymap.
#[cfg(test)]
pub fn reference_markdown(labels: &dyn KeyLabels) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    out.push_str("# Command and key reference\n\n");
    out.push_str(
        "Generated from the action registry in\n\
         [`registry/entries.rs`](../crates/deadpan-app/src/navigation/registry/entries.rs)\n\
         by `cargo test -p deadpan-app --bin deadpan-app commands_reference`; set\n\
         `DEADPAN_UPDATE_COMMANDS_MD=1` to rewrite it. Do not edit it by hand.\n\
         Keys are the shipped logical defaults; a personal\n\
         [keymap](KEYMAP.md) changes configurable paths, and the app's Keys sheet\n\
         (`?`) always shows the live ones. Fixed mode keys are not configurable;\n\
         tests prove each mode router acts on exactly them. Native shortcuts\n\
         (menus, ⌘ chords, panel controls) are described, not router-verified.\n\
         Headless status links to [PARITY](PARITY.md).\n\n",
    );
    let _ = writeln!(
        out,
        "{} actions, {} command verbs.\n",
        SPECS.len(),
        VERBS.len()
    );
    for section in Section::ALL {
        let specs: Vec<_> = SPECS
            .iter()
            .filter(|spec| spec.section == section)
            .collect();
        if specs.is_empty() {
            continue;
        }
        let title = section.title();
        let mut heading = String::new();
        for (index, word) in title.split(' ').enumerate() {
            if index > 0 {
                heading.push(' ');
            }
            let lower = word.to_lowercase();
            if index == 0 || !matches!(lower.as_str(), "of" | "a" | "the" | "&") {
                let mut chars = lower.chars();
                if let Some(first) = chars.next() {
                    heading.extend(first.to_uppercase());
                    heading.push_str(chars.as_str());
                }
            } else {
                heading.push_str(&lower);
            }
        }
        let _ = writeln!(out, "## {heading}\n");
        out.push_str("| Action | Keys | Command | Where | Macro / dot | Headless |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for spec in specs {
            let keys = spec
                .keys
                .iter()
                .map(|keys| match keys {
                    Keys::Editor(id) => {
                        let label = labels.all(*id);
                        if label.is_empty() {
                            format!("none (`{}`)", id.as_str())
                        } else {
                            format!("`{}` (`{}`)", label.replace('|', "\\|"), id.as_str())
                        }
                    }
                    Keys::Native(label) => format!("`{label}` (native)"),
                    Keys::Mode { label, .. } => format!("`{label}` (fixed)"),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let commands = spec
                .commands
                .iter()
                .map(|command| format!("`{}`", command.syntax().replace('|', "\\|")))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "| **{}** (`{}`)<br>{} | {} | {} | {} | {} | [{}](PARITY.md#{}): `{}` |",
                spec.name,
                spec.id,
                render(spec.help, labels).replace('|', "\\|"),
                if keys.is_empty() { "-".into() } else { keys },
                if commands.is_empty() {
                    "-".into()
                } else {
                    commands
                },
                spec.contexts.label(),
                spec.replay.label(),
                spec.headless.parity.label(),
                spec.headless.anchor,
                spec.headless.form.replace('|', "\\|"),
            );
        }
        out.push('\n');
    }
    out
}
