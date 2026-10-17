//! The Keys sheet as a palette: which searched actions match, and what
//! running one does where the editor is now.
//!
//! Resolution is pure. The app supplies the current context and whether a
//! configured binding acts in it; the outcome names the same path a typed
//! command or key press takes. An action that cannot run here is explained,
//! never silently ignored (specification §8: the palette must not hide a
//! disabled action without saying why).

use super::BindingId;
use super::registry::{self, Contexts, KeyLabels, Keys, Section, Spec};

/// What running a palette entry does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Close Help and run `:verb`, exactly as if typed on the command line.
    Run(&'static str),
    /// Close Help and open the command line with `verb ` typed, so
    /// completion shows its usage.
    Prefill(&'static str),
    /// Close Help and perform this configured binding as its key press would.
    Key(BindingId),
    /// Keep Help open and say where and how the action works.
    Explain(String),
}

/// A selected action form survives search changes and deferred input intact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Choice {
    pub spec_id: &'static str,
    pub form: Form,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
    Whole,
    Command(usize),
    Binding(usize),
}

impl From<&'static str> for Choice {
    fn from(spec_id: &'static str) -> Self {
        Self {
            spec_id,
            form: Form::Whole,
        }
    }
}

impl Choice {
    /// Resolve the exact displayed form, never another usable sibling.
    pub fn spec(self) -> Option<Spec> {
        let parent = registry::by_id(self.spec_id)?;
        match self.form {
            Form::Whole => Some(*parent),
            Form::Command(index) => {
                let command = parent.commands.get(index)?;
                if alias(command.verb) {
                    return None;
                }
                let end = (index + 1..parent.commands.len())
                    .find(|next| !alias(parent.commands[*next].verb))
                    .unwrap_or(parent.commands.len());
                let name = command_name(command.verb, parent.name);
                let keys: &'static [Keys] = match (parent.id, index) {
                    ("undo", 0) => &[Keys::Editor(BindingId::Undo), Keys::Native("⌘Z")],
                    ("macro.record", 0) => &parent.keys[..1],
                    ("mark.history", 0) => &[Keys::Native("Ctrl O")],
                    ("mark.history", 1) => &[Keys::Native("Ctrl I")],
                    ("undo", 1) => &[Keys::Native("Ctrl R / ⌘⇧Z")],
                    ("paste", index) => &parent.keys[index..index + 1],
                    _ => &[],
                };
                Some(Spec {
                    name,
                    keys,
                    commands: &parent.commands[index..end],
                    help: name,
                    ..*parent
                })
            }
            Form::Binding(index) => {
                let Keys::Editor(binding) = parent.keys.get(index)? else {
                    return None;
                };
                let name = binding_name(*binding, parent.name);
                Some(Spec {
                    name,
                    keys: &parent.keys[index..index + 1],
                    commands: &[],
                    help: name,
                    ..*parent
                })
            }
        }
    }
}

// These are parser aliases, not additional operations. Keep their syntax
// searchable on the same row as the canonical verb.
fn alias(verb: &str) -> bool {
    matches!(verb, "new-url" | "generate-ai" | "previous-ai")
}

fn choices(spec: &'static Spec) -> Vec<Choice> {
    let commands = spec
        .commands
        .iter()
        .filter(|verb| !alias(verb.verb))
        .count();
    let bindings = editor_bindings(spec).count();
    if commands <= 1 && bindings <= 1 {
        return vec![spec.id.into()];
    }
    let mut choices = Vec::new();
    for (index, command) in spec.commands.iter().enumerate() {
        if !alias(command.verb) {
            choices.push(Choice {
                spec_id: spec.id,
                form: Form::Command(index),
            });
        }
    }
    for (index, key) in spec.keys.iter().enumerate() {
        if matches!(key, Keys::Editor(_))
            && (commands == 0 || spec.id == "gain.step" || spec.id == "ai.compare")
        {
            choices.push(Choice {
                spec_id: spec.id,
                form: Form::Binding(index),
            });
        }
    }
    choices
}

fn command_name(verb: &str, fallback: &'static str) -> &'static str {
    match verb {
        "undo" => "Undo",
        "redo" => "Redo",
        "jump-back" => "Jump backward in history",
        "jump-forward" => "Jump forward in history",
        "paste" => "Paste after or replace",
        "paste-before" => "Paste before or replace",
        "jcut" => "J-cut",
        "lcut" => "L-cut",
        "retime" => "Change speed",
        "wrap-retime" => "Wrap in a new speed stage",
        "reverse" => "Reverse hiccup",
        "ping-pong" => "Ping-pong hiccup",
        "gain" => "Set exact gain or open Gain draft",
        "sound-eq" => "Sound equalizer",
        "sound-saturate" => "Sound saturation",
        "sound-balance" => "Sound stereo balance",
        "sound-mute" => "Mute sound",
        "sound-envelope" => "Sound gain envelope",
        "sound-allow" => "Allow sound in a pause",
        "sound-silence" => "Silence sound in a pause",
        "next-ai" => "Choose next AI variant",
        "prev-ai" => "Choose previous AI variant",
        "pick-ai" => "Choose numbered AI variant",
        "preview-ai" => "Preview AI pictures",
        "audition-ai" => "Audition AI pictures",
        "gag-save" => "Save a gag preset",
        "gag-presets" => "List gag presets",
        "recipe-save" => "Save a local recipe",
        "recipe" => "Apply a local recipe",
        "recipe-inspect" => "Inspect a local recipe",
        "record" => "Record a macro",
        "record-stop" => "Stop and save macro recording",
        _ => fallback,
    }
}

fn binding_name(binding: BindingId, fallback: &'static str) -> &'static str {
    use BindingId::*;
    match binding {
        PaneNext => "Focus next pane",
        PanePrevious => "Focus previous pane",
        FramePrevious => "Previous frame",
        FrameNext => "Next frame",
        BeatNext => "Next beat",
        BeatPrevious => "Previous beat",
        WordNext => "Next word",
        WordPrevious => "Previous word",
        WordEnd => "End of word",
        SentenceNext => "Next sentence",
        SentencePrevious => "Previous sentence",
        PauseNext => "Next pause",
        PausePrevious => "Previous pause",
        ShotNext => "Next shot",
        ShotPrevious => "Previous shot",
        First => "First boundary",
        Last => "Last boundary",
        PlayNext => "Next Repeat play",
        PlayPrevious => "Previous Repeat play",
        SearchNext => "Next search match",
        SearchPrevious => "Previous search match",
        InnerGroup => "Select group contents",
        AroundGroup => "Select whole group",
        InnerBeat => "Select beat without attachments",
        AroundBeat => "Select beat with attachments",
        InnerWord => "Select word",
        AroundWord => "Select word with surrounding gap",
        InnerSentence => "Select sentence",
        AroundSentence => "Select sentence with surrounding gap",
        InnerPause => "Select pause",
        AroundPause => "Select pause with surrounding speech",
        InnerShot => "Select shot",
        AroundShot => "Select whole shot (same hard-cut boundaries)",
        RepeatOperator => "Repeat a motion",
        RepeatRange => "Repeat selected range",
        GainUp => "Increase gain",
        GainDown => "Decrease gain",
        CompareAi => "Toggle AI comparison",
        NextAi => "Show next AI variant",
        _ => fallback,
    }
}

/// One independently executable search result, with the text its row shows.
pub struct Match {
    pub choice: Choice,
    pub spec: Spec,
    pub keys: String,
    pub help: String,
}

/// Matching forms, best first. Exact keys, verbs and form names take priority
/// over the shared registry explanation.
pub fn matches(labels: &dyn KeyLabels, query: &str) -> Vec<Match> {
    let mut found = Vec::new();
    for (index, parent) in registry::SPECS.iter().enumerate() {
        for (form_index, choice) in choices(parent).into_iter().enumerate() {
            let Some(spec) = choice.spec() else {
                continue;
            };
            let keys = registry::key_text(&spec, labels);
            let shared_help = registry::render(parent.help, labels);
            // The row and its accessibility label already name this form.
            let help = shared_help.clone();
            // Keep the registry's detailed context discoverable as a prose
            // fallback. Exact keys/verbs and each form's own name rank first.
            let exact_key = !query.trim().is_empty()
                && keys
                    .split(" · ")
                    .flat_map(|part| part.split(" / "))
                    .any(|key| key == query.trim());
            let rank = if exact_key {
                Some(0)
            } else {
                registry::search_rank(&spec, &keys, spec.name, query)
                    .or_else(|| registry::search_rank(&spec, &keys, &shared_help, query).map(|_| 3))
            };
            if let Some(rank) = rank {
                found.push((
                    rank,
                    spec.section,
                    index,
                    form_index,
                    Match {
                        choice,
                        spec,
                        keys,
                        help,
                    },
                ));
            }
        }
    }
    found.sort_by_key(|(rank, section, index, form_index, _)| {
        (*rank, *section, *index, *form_index)
    });
    found.into_iter().map(|(.., found)| found).collect()
}

/// Whether a verb's usage names arguments, required or optional.
pub fn takes_arguments(verb: &registry::Verb) -> bool {
    verb.syntax().split_whitespace().nth(1).is_some()
}

/// What running `spec` does in `here`. `usable` says whether a configured
/// binding acts in the current selection and routing domain.
pub fn resolve(
    spec: &Spec,
    here: Contexts,
    usable: impl Fn(BindingId) -> bool,
    labels: &dyn KeyLabels,
) -> Outcome {
    let applies = spec.contexts.intersects(here);
    if applies {
        if let Some(verb) = spec.commands.first() {
            return if takes_arguments(verb) {
                Outcome::Prefill(verb.verb)
            } else {
                Outcome::Run(verb.verb)
            };
        }
        if let Some(id) = editor_bindings(spec).next().filter(|id| usable(*id)) {
            return Outcome::Key(id);
        }
    }
    Outcome::Explain(explanation(spec, here, applies, labels))
}

fn editor_bindings(spec: &Spec) -> impl Iterator<Item = BindingId> + '_ {
    spec.keys.iter().filter_map(|keys| match keys {
        Keys::Editor(id) => Some(*id),
        _ => None,
    })
}

/// Where and how an action that cannot run from here works.
fn explanation(spec: &Spec, here: Contexts, applies: bool, labels: &dyn KeyLabels) -> String {
    let keys = registry::key_text(spec, labels);
    let how = if keys.is_empty() {
        "its own controls".to_owned()
    } else {
        keys
    };
    let reason = if spec.section == Section::Help {
        format!("{} is a key of this sheet: press {how} here.", spec.name)
    } else if applies {
        // The action belongs here, but its key does not act on the current
        // selection, its path is unbound, or it is a native control.
        format!(
            "{} has no command to run from here; press {how} in {}.",
            spec.name,
            spec.contexts.label()
        )
    } else {
        format!(
            "{} works in {}, not in {}. There, use {how}.",
            spec.name,
            spec.contexts.label(),
            here.label()
        )
    };
    format!("{reason} Help stays open; Esc closes it.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::{Bindings, EditSelection, PaletteBinding, RoutingDomain};

    fn spec(id: &str) -> &'static Spec {
        registry::by_id(id).unwrap_or_else(|| panic!("{id} is registered"))
    }

    fn resolve_in(
        id: &str,
        here: Contexts,
        domain: RoutingDomain,
        selection: EditSelection,
    ) -> Outcome {
        let mut bindings = Bindings::default();
        bindings.set_routing_domain(domain);
        let probe = bindings.clone();
        resolve(
            spec(id),
            here,
            |binding| probe.palette_binding(binding, selection).is_some(),
            &bindings,
        )
    }

    fn edit(id: &str) -> Outcome {
        resolve_in(id, Contexts::EDIT, RoutingDomain::Edit, EditSelection::None)
    }

    #[test]
    fn verbs_run_bare_or_open_their_usage_and_keys_dispatch_their_binding() {
        // `:splice` opens Place slice exactly as typed.
        assert_eq!(edit("splice"), Outcome::Run("splice"));
        assert_eq!(edit("jobs"), Outcome::Run("jobs"));
        // This helper exercises the original registry view; form-specific
        // resolution below separately covers every executable sibling.
        assert_eq!(edit("undo"), Outcome::Run("undo"));
        // A verb with arguments, even optional ones, opens its usage.
        assert_eq!(edit("hold"), Outcome::Prefill("hold"));
        assert!(takes_arguments(&spec("hold").commands[0]));
        assert!(!takes_arguments(&spec("splice").commands[0]));
        // Key-only actions perform their first binding that acts here.
        assert_eq!(edit("beat"), Outcome::Key(BindingId::BeatNext));
        assert_eq!(edit("visual"), Outcome::Key(BindingId::Visual));
        assert_eq!(edit("cut.operator"), Outcome::Key(BindingId::CutOperator));
        assert_eq!(edit("copy.beat"), Outcome::Key(BindingId::CopyBeat));
    }

    #[test]
    fn actions_from_another_context_or_mode_explain_where_and_how() {
        // A Visual-only object in Normal names its context and keys.
        let Outcome::Explain(message) = edit("object.group") else {
            panic!("ig/ag needs a Visual selection");
        };
        assert!(
            message.contains("works in Visual, not in Your edit"),
            "{message}"
        );
        assert!(
            message.contains("ig / ag") || message.contains("ig"),
            "{message}"
        );
        assert!(
            message.ends_with("Help stays open; Esc closes it."),
            "{message}"
        );
        // Trim's own keys work only inside Trim.
        let Outcome::Explain(message) = edit("trim.apply") else {
            panic!("Trim keys are mode-only");
        };
        assert!(message.contains("works in Trim"), "{message}");
        assert!(message.contains("Enter"), "{message}");
        // The sheet's own keys are explained as such.
        let Outcome::Explain(message) = edit("help.close") else {
            panic!("Help keys are not editor actions");
        };
        assert!(message.contains("key of this sheet"), "{message}");
        // In Visual the same object selection acts.
        assert_eq!(
            resolve_in(
                "object.group",
                Contexts::VISUAL,
                RoutingDomain::Edit,
                EditSelection::Range
            ),
            Outcome::Key(BindingId::InnerGroup)
        );
        // Placing a copy is an Edit command; from the Original it is explained.
        let Outcome::Explain(message) = resolve_in(
            "splice",
            Contexts::ORIGINAL,
            RoutingDomain::Original,
            EditSelection::None,
        ) else {
            panic!(":splice belongs to Your edit");
        };
        assert!(message.contains("not in Original"), "{message}");
        assert!(message.contains(":splice"), "{message}");
    }

    #[test]
    fn a_key_disabled_in_the_current_selection_is_explained_not_dropped() {
        // `yy` copies a whole beat only without a Visual selection; the
        // registry still lists Edit, so the binding check decides.
        let Outcome::Explain(message) = resolve_in(
            "copy.beat",
            Contexts::EDIT,
            RoutingDomain::Edit,
            EditSelection::Range,
        ) else {
            panic!("yy does not act on a Visual range");
        };
        assert!(
            message.contains("has no command to run from here"),
            "{message}"
        );
    }

    #[test]
    fn every_registered_action_resolves_to_a_path_or_an_explanation_everywhere() {
        let contexts = [
            (
                Contexts::ORIGINAL,
                RoutingDomain::Original,
                EditSelection::None,
            ),
            (Contexts::EDIT, RoutingDomain::Edit, EditSelection::None),
            (Contexts::VISUAL, RoutingDomain::Edit, EditSelection::Range),
            (Contexts::CATALOG, RoutingDomain::Sound, EditSelection::None),
            (Contexts::PLACED, RoutingDomain::Sound, EditSelection::None),
            (Contexts::CONTENTS, RoutingDomain::Edit, EditSelection::None),
        ];
        for spec in registry::SPECS {
            for (here, domain, selection) in contexts {
                match resolve_in(spec.id, here, domain, selection) {
                    Outcome::Run(verb) | Outcome::Prefill(verb) => {
                        assert!(spec.contexts.intersects(here), "{}", spec.id);
                        assert!(
                            crate::navigation::command::parse(verb).is_ok()
                                || takes_arguments(&spec.commands[0]),
                            "{} runs :{verb}",
                            spec.id
                        );
                    }
                    Outcome::Key(id) => {
                        let mut bindings = Bindings::default();
                        bindings.set_routing_domain(domain);
                        assert!(
                            bindings.palette_binding(id, selection).is_some(),
                            "{} {id:?}",
                            spec.id
                        );
                    }
                    Outcome::Explain(message) => {
                        assert!(message.contains(spec.name), "{}: {message}", spec.id);
                    }
                }
            }
        }
    }

    #[test]
    fn prefix_bindings_enter_their_pending_path_like_their_first_key() {
        let mut bindings = Bindings::default();
        assert_eq!(
            bindings.palette_binding(BindingId::CutOperator, EditSelection::None),
            Some(PaletteBinding::Prefix)
        );
        assert!(bindings.enter_prefix(BindingId::CutOperator, EditSelection::None));
        assert_eq!(bindings.pending(), "d");
        assert!(bindings.operator_pending());
        // The pending path completes with the ordinary next key.
        let action = bindings.key(
            eframe::egui::Key::D,
            eframe::egui::Modifiers::NONE,
            false,
            false,
        );
        assert!(
            matches!(
                action,
                Some(crate::navigation::Action::Operator { cut: true, .. })
            ),
            "{action:?}"
        );
        // Terminal bindings resolve exactly as their path does.
        let mut keyed = Bindings::default();
        let pressed = keyed.key(
            eframe::egui::Key::L,
            eframe::egui::Modifiers::NONE,
            false,
            false,
        );
        assert_eq!(
            Bindings::default().palette_binding(BindingId::FrameNext, EditSelection::None),
            pressed.map(PaletteBinding::Action)
        );
        // A personal map's path is the one entered.
        let mut personal = Bindings::from_json(
            br#"{"version":1,"key_mode":"logical","bindings":[{"action":"cut.operator","keys":[["F7"]]}]}"#,
        )
        .unwrap();
        assert!(personal.enter_prefix(BindingId::CutOperator, EditSelection::None));
        assert_eq!(personal.pending(), "F7");
        // Visual has no cut operator prefix to enter.
        assert!(!personal.enter_prefix(BindingId::CutOperator, EditSelection::Range));
    }

    #[test]
    fn search_results_rank_exact_keys_and_verbs_first() {
        let bindings = Bindings::default();
        let found = matches(&bindings, "splice");
        assert_eq!(found.first().map(|found| found.spec.id), Some("splice"));
        let found = matches(&bindings, "dd");
        assert_eq!(
            found.len(),
            1,
            "{:?}",
            found.iter().map(|f| f.spec.id).collect::<Vec<_>>()
        );
        assert!(matches(&bindings, "zzzz-no-such-action").is_empty());
    }
    #[test]
    fn selected_verbs_directions_and_native_keys_resolve_the_exact_form() {
        let bindings = Bindings::default();
        for (query, expected) in [
            (":redo", Outcome::Run("redo")),
            ("⌘Z", Outcome::Run("undo")),
            ("⌘⇧Z", Outcome::Run("redo")),
            ("Ctrl O", Outcome::Run("jump-back")),
            ("Ctrl I", Outcome::Run("jump-forward")),
            ("previous beat", Outcome::Key(BindingId::BeatPrevious)),
            ("next beat", Outcome::Key(BindingId::BeatNext)),
            (":wrap-retime", Outcome::Prefill("wrap-retime")),
            (":previous-ai", Outcome::Run("prev-ai")),
            (":record-stop", Outcome::Run("record-stop")),
        ] {
            let found = matches(&bindings, query);
            let choice = found
                .first()
                .unwrap_or_else(|| panic!("no result for {query}"));
            assert_eq!(
                resolve(&choice.spec, Contexts::EDIT, |_| true, &bindings),
                expected,
                "{query}: {}",
                choice.spec.name
            );
        }
    }

    #[test]
    fn unavailable_direction_never_falls_back_to_its_usable_sibling() {
        let bindings = Bindings::default();
        let choice = matches(&bindings, "previous beat").remove(0);
        assert!(matches!(
            resolve(
                &choice.spec,
                Contexts::EDIT,
                |binding| binding == BindingId::BeatNext,
                &bindings
            ),
            Outcome::Explain(_)
        ));
        let personal = Bindings::from_json(
            br#"{"version":1,"key_mode":"logical","bindings":[{"action":"frame.previous","keys":[["F7"],["F8"]]}]}"#,
        ).unwrap();
        for key in ["F7", "F8"] {
            let chosen = matches(&personal, key).remove(0);
            assert_eq!(
                resolve(&chosen.spec, Contexts::EDIT, |_| true, &personal),
                Outcome::Key(BindingId::FramePrevious)
            );
        }
    }

    #[test]
    fn expanded_forms_have_unique_names_and_cover_every_declared_executable_form() {
        for parent in registry::SPECS {
            let choices = choices(parent);
            let mut identities = std::collections::HashSet::new();
            let mut names = std::collections::HashSet::new();
            let forms: Vec<_> = choices
                .iter()
                .map(|choice| choice.spec().unwrap())
                .collect();
            for (choice, form) in choices.iter().zip(&forms) {
                assert!(
                    identities.insert(*choice),
                    "duplicate choice in {}",
                    parent.id
                );
                assert!(
                    names.insert(form.name),
                    "ambiguous form name in {}",
                    parent.id
                );
                assert!(
                    form.commands.len() <= 1
                        || form.commands[1..].iter().all(|verb| alias(verb.verb)),
                    "grouped operations in {}",
                    parent.id
                );
                assert!(
                    editor_bindings(form).count() <= 1,
                    "grouped bindings in {}",
                    parent.id
                );
            }
            for command in parent.commands {
                assert!(
                    forms.iter().any(|form| form.commands.contains(command)),
                    "lost :{}",
                    command.verb
                );
            }
            for binding in editor_bindings(parent) {
                assert!(
                    forms
                        .iter()
                        .any(|form| editor_bindings(form).any(|id| id == binding)),
                    "lost {binding:?}"
                );
            }
        }
    }

    #[test]
    fn aliases_share_one_choice_and_mixed_parameterized_actions_remain_distinct() {
        let bindings = Bindings::default();
        for (first, alias) in [
            (":youtube", ":new-url"),
            (":generate", ":generate-ai"),
            (":prev-ai", ":previous-ai"),
        ] {
            assert_eq!(
                matches(&bindings, first)[0].choice,
                matches(&bindings, alias)[0].choice
            );
        }
        let gain = choices(spec("gain.step"));
        assert_eq!(gain.len(), 3);
        let compare = choices(spec("ai.compare"));
        assert_eq!(compare.len(), 3);
        let outcomes: Vec<_> = compare
            .into_iter()
            .map(|choice| resolve(&choice.spec().unwrap(), Contexts::EDIT, |_| true, &bindings))
            .collect();
        assert!(outcomes.contains(&Outcome::Prefill("compare-ai")));
        assert!(outcomes.contains(&Outcome::Key(BindingId::CompareAi)));
        assert!(outcomes.contains(&Outcome::Key(BindingId::NextAi)));
        assert!(
            matches(&bindings, "full-Original starting point")
                .iter()
                .any(|found| found.choice.spec_id == "undo")
        );
    }
}
