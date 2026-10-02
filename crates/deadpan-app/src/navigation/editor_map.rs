//! Shipped editor grammar. Routing and pending-key teaching share these rules.

use std::sync::LazyLock;

use super::binding_trie::{Binding, Prefix, Trie};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stroke(pub Key, pub bool);

impl Stroke {
    pub fn label(self) -> String {
        if let Some(letter) = mark_letter(self.0, self.1) {
            return letter.to_string();
        }
        match self.0 {
            Key::Comma => ",",
            Key::Quote => "'",
            Key::ArrowLeft => "←",
            Key::ArrowRight => "→",
            Key::ArrowUp => "↑",
            Key::ArrowDown => "↓",
            Key::Plus => "+",
            Key::Minus => "-",
            Key::Colon => ":",
            Key::Slash => "/",
            Key::Questionmark => "?",
            _ => self.0.name(),
        }
        .into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PrefixKind {
    Start,
    Leader,
    Repeat,
    Delete,
    Mark(MarkPrefix),
}

pub(super) struct PrefixInfo {
    pub kind: PrefixKind,
    pub hint: String,
    pub counted_hint: String,
    pub offer_insert: bool,
    pub invalid: Option<&'static str>,
}

#[derive(Clone, Copy)]
enum CountPolicy {
    Ignore,
    Motion,
    Frames,
    Repeat,
    DeleteBeat,
    Hold,
    Gain,
    Refuse(&'static str),
}

pub(super) struct Rule {
    action: Action,
    count: CountPolicy,
    short: &'static str,
    pub repeatable: bool,
}

impl Rule {
    pub fn resolve(&self, count: Option<u32>) -> Action {
        match self.count {
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
                _ => unreachable!("motion declarations carry a motion action"),
            },
            CountPolicy::Frames => match count {
                Some(0) => Action::Invalid("A frame cut count must be positive; no edit was made."),
                value => Action::DeleteFrames(value.unwrap_or(1)),
            },
            CountPolicy::Repeat => match count {
                Some(0) => Action::Invalid("An edit count must be positive; no edit was made."),
                value => Action::Edit(BeatEdit::WrapRepeat(value.unwrap_or(2))),
            },
            CountPolicy::DeleteBeat => match count {
                Some(0) => Action::Invalid("An edit count must be positive; no edit was made."),
                None | Some(1) => self.action,
                Some(_) => Action::Invalid(
                    "dd deletes one selected beat. Counted deletion is not available.",
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

    fn accepts_leader_count(&self) -> bool {
        matches!(self.count, CountPolicy::Hold)
    }
}

type EditorTrie = Trie<Stroke, Rule, PrefixInfo>;

static NORMAL: LazyLock<EditorTrie> = LazyLock::new(|| compile(false));
static VISUAL: LazyLock<EditorTrie> = LazyLock::new(|| compile(true));

pub(super) fn map(selection: EditSelection) -> &'static EditorTrie {
    match selection {
        EditSelection::None => &NORMAL,
        EditSelection::Empty | EditSelection::Range => &VISUAL,
    }
}

pub(super) fn rule(path: &[Stroke], selection: EditSelection) -> Option<&'static Rule> {
    map(selection)
        .resolve(path)?
        .terminal()
        .map(|binding| &binding.value)
}

pub(super) fn prefix(path: &[Stroke]) -> Option<&'static PrefixInfo> {
    map(EditSelection::None)
        .resolve(path)?
        .prefix()
        .map(|prefix| &prefix.value)
}

pub(super) fn interrupt(key: Key, shift: bool) -> Action {
    rule(&[Stroke(key, shift)], EditSelection::None)
        .expect("interrupt keys are declared in the shipped grammar")
        .resolve(None)
}

fn compile(visual: bool) -> EditorTrie {
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
            },
        });
    };
    use CountPolicy as C;
    let plain = |key| Stroke(key, false);
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
        (Key::Questionmark, false, Action::Help, "keys"),
    ] {
        add(&[Stroke(key, shift)], action, C::Ignore, short, false);
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
        Action::Edit(BeatEdit::WrapRepeat(2)),
        C::Repeat,
        "completes the Repeat",
        false,
    );
    if visual {
        add(
            &[plain(Key::D)],
            Action::DeleteSelection,
            C::Refuse("Delete the selected range once with d, without a count."),
            "cut selected range",
            false,
        );
    } else {
        add(
            &[plain(Key::D), plain(Key::D)],
            Action::Edit(BeatEdit::Delete),
            C::DeleteBeat,
            "cuts this whole beat",
            false,
        );
    }
    add(
        &[plain(Key::X)],
        Action::DeleteFrames(1),
        C::Frames,
        "cut frames",
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
                "Use v, y, p or P without a count. Move the range boundary with counted h/l.",
            ),
            short,
            false,
        );
    }
    add(
        &[Stroke(Key::P, true)],
        Action::PasteMoment { before: true },
        C::Refuse("Paste once with p or P, without a count or operator."),
        "paste before",
        false,
    );
    add(
        &[plain(Key::S)],
        Action::Edit(BeatEdit::Split),
        C::Refuse(
            "Split uses the current boundary. Move with a count first, for example 12l then s.",
        ),
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
            Key::I,
            Action::Insert,
            C::Refuse("Reuse inserts once. Use ,i without a count."),
            "reuse Original",
        ),
        (
            Key::S,
            Action::Sound(SoundAction::Place),
            C::Refuse("Place one sound with ,s, without a count."),
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
            Key::F,
            Action::Framing(FramingAction::EnterCamera),
            C::Refuse("Counts apply only to ,h. Use ,f, ,z, or ,c without a count."),
            "Camera",
        ),
        (
            Key::V,
            Action::Trim,
            C::Refuse("Open Trim once with ,v, without a count."),
            "Trim",
        ),
        (
            Key::Z,
            Action::Framing(FramingAction::PunchIn),
            C::Refuse("Counts apply only to ,h. Use ,f, ,z, or ,c without a count."),
            "punch in",
        ),
        (
            Key::C,
            Action::Framing(FramingAction::Creep),
            C::Refuse("Counts apply only to ,h. Use ,f, ,z, or ,c without a count."),
            "creep",
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
                    &[plain(key), Stroke(letter_key, shift)],
                    action,
                    C::Ignore,
                    "mark",
                    false,
                );
            }
        }
    }

    let mut prefixes = Vec::new();
    for (key, kind, offer_insert, invalid) in [
        (Key::G, PrefixKind::Start, false, None),
        (
            Key::Comma,
            PrefixKind::Leader,
            true,
            Some(
                "After comma, use i to reuse the Original, s to place a sound, h for a pause, f for Camera, v for Trim, z to punch in, or c to creep.",
            ),
        ),
        (
            Key::R,
            PrefixKind::Repeat,
            true,
            Some(
                "Use rr to repeat a beat, dd to delete a beat, or select time with v and cut it with d. Motion and text-object operators are not ready.",
            ),
        ),
        (
            Key::D,
            PrefixKind::Delete,
            true,
            Some(
                "Use rr to repeat a beat, dd to delete a beat, or select time with v and cut it with d. Motion and text-object operators are not ready.",
            ),
        ),
        (
            Key::M,
            PrefixKind::Mark(MarkPrefix::Set),
            false,
            Some("A mark name must be one letter, a–z or A–Z; no mark action was taken."),
        ),
        (
            Key::Quote,
            PrefixKind::Mark(MarkPrefix::Jump),
            false,
            Some("A mark name must be one letter, a–z or A–Z; no mark action was taken."),
        ),
    ] {
        if visual && kind == PrefixKind::Delete {
            continue;
        }
        let path = vec![plain(key)];
        let hint = match kind {
            PrefixKind::Mark(MarkPrefix::Set) => {
                "a–z / A–Z saves this position · Esc cancels".into()
            }
            PrefixKind::Mark(MarkPrefix::Jump) => {
                "a–z / A–Z jumps to that mark · Esc cancels".into()
            }
            _ => teaching(&bindings, &path, false),
        };
        let counted_hint = if kind == PrefixKind::Leader {
            teaching(&bindings, &path, true)
        } else {
            hint.clone()
        };
        prefixes.push(Prefix {
            label: plain(key).label(),
            path,
            value: PrefixInfo {
                kind,
                hint,
                counted_hint,
                offer_insert,
                invalid,
            },
        });
    }
    let trie =
        Trie::compile(bindings, prefixes).expect("shipped editor grammar must be unambiguous");
    // Root children are the actually reachable entries, not a separate list
    // of advertised shortcuts. Compilation rejects terminal/prefix ambiguity.
    assert!(
        trie.resolve(&[])
            .is_some_and(|root| root.children().next().is_some())
    );
    trie
}

fn teaching(bindings: &[Binding<Stroke, Rule>], prefix: &[Stroke], counted: bool) -> String {
    let mut parts: Vec<_> = bindings
        .iter()
        .filter(|binding| {
            binding.path.starts_with(prefix) && binding.path.len() == prefix.len() + 1
        })
        .filter(|binding| !counted || binding.value.accepts_leader_count())
        .map(|binding| {
            format!(
                "{} {}",
                binding.path[prefix.len()].label(),
                if counted {
                    "inserts the counted pause"
                } else {
                    binding.value.short
                }
            )
        })
        .collect();
    parts.push("Esc cancels".into());
    parts.join(" · ")
}

/// Enumerate the compiled branches for the Kestrel audit. New declarations
/// cannot add an unaudited pending state by omitting a hand-maintained list.
#[cfg(any(test, feature = "ui-harness"))]
pub(super) fn prefix_paths() -> Vec<Vec<Stroke>> {
    let mut prefixes = vec![Vec::new()];
    for selection in [EditSelection::None, EditSelection::Range] {
        for path in branch_paths(map(selection)) {
            if !prefixes.contains(&path) {
                prefixes.push(path);
            }
        }
    }
    prefixes
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
