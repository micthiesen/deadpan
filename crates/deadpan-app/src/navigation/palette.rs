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

/// One search result, with the text its row shows.
pub struct Match {
    pub spec: &'static Spec,
    pub keys: String,
    pub help: String,
}

/// Matching actions, best first: an exact key, then a verb, a name, any
/// text; ties keep the registry's section and declaration order. The Keys
/// sheet draws exactly this list, and Enter runs its highlighted entry.
pub fn matches(labels: &dyn KeyLabels, query: &str) -> Vec<Match> {
    let mut found: Vec<_> = registry::SPECS
        .iter()
        .enumerate()
        .filter_map(|(index, spec)| {
            let keys = registry::key_text(spec, labels);
            let help = registry::render(spec.help, labels);
            registry::search_rank(spec, &keys, &help, query)
                .map(|rank| (rank, spec.section, index, Match { spec, keys, help }))
        })
        .collect();
    found.sort_by_key(|(rank, section, index, _)| (*rank, *section, *index));
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
        if let Some(id) = editor_bindings(spec).find(|id| usable(*id)) {
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
        // The first verb of a pair; `:redo` stays reachable by typing.
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
}
