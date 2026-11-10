use super::*;
use crate::navigation::command::parse;

fn empty() -> Context {
    Context::default()
}

fn guided(input: &str) -> Guide {
    guide(input, &empty()).unwrap_or_else(|| panic!("no guide for {input}"))
}

fn verbs() -> Vec<&'static str> {
    SPECS
        .iter()
        .flat_map(|spec| spec.commands.iter().map(|command| command.verb))
        .collect()
}

#[test]
fn every_registry_verb_has_a_schema_or_a_reason() {
    let verbs = verbs();
    for verb in &verbs {
        let declared = SCHEMAS.iter().filter(|(known, _)| known == verb).count();
        let reasons = REASONS.iter().filter(|(known, _)| known == verb).count();
        assert_eq!(
            declared + reasons,
            1,
            ":{verb} needs exactly one declared schema or one reason"
        );
    }
    for verb in SCHEMAS
        .iter()
        .map(|(verb, _)| verb)
        .chain(REASONS.iter().map(|(verb, _)| verb))
    {
        assert!(verbs.contains(verb), ":{verb} is not a registry verb");
    }
    for (_, reason) in REASONS {
        assert!(reason.ends_with('.') && reason.len() > 10, "{reason}");
    }
}

#[test]
fn the_requested_high_traffic_verbs_are_declared() {
    for verb in [
        "hold",
        "hold-duration",
        "retime",
        "ramp",
        "repeat",
        "gain",
        "zoom",
        "creep",
        "caption",
        "split",
        "room-tone",
        "audio-lag",
        "slip",
        "trim",
        "edge",
        "gag",
        "gag-set",
        "macro",
        "scope",
    ] {
        assert!(declared(verb).is_some(), ":{verb}");
    }
}

#[test]
fn every_example_parses_and_passes_the_guide() {
    for (verb, schema) in SCHEMAS {
        for example in schema.examples {
            let typed = example
                .split_whitespace()
                .next()
                .unwrap()
                .trim_start_matches(':');
            assert!(lookup(typed).is_some(), "{example}");
            assert!(
                parse(example).is_ok(),
                ":{verb} example {example}: {:?}",
                parse(example)
            );
            if example.contains(char::is_whitespace) {
                let guide = guided(&format!("{example} "));
                assert_eq!(guide.problem, None, "{example}");
            }
        }
    }
}

/// Every declared parameter is exercised by an example the parser accepts.
#[test]
fn every_declared_parameter_appears_in_a_parsed_example() {
    for (verb, schema) in SCHEMAS {
        if *verb == "gag-set" {
            continue;
        }
        // Verbs sharing one parameter set (:gag and :gag-inspect) share examples.
        let names = schema.option_names();
        let examples = || {
            SCHEMAS
                .iter()
                .filter(|(_, other)| other.option_names() == names)
                .flat_map(|(_, other)| other.examples.iter())
        };
        for name in &names {
            assert!(
                examples().any(|example| example.contains(&format!(" {name}="))),
                ":{verb} {name}= has no example"
            );
        }
        if !schema.positional.is_empty() {
            assert!(
                examples().any(|example| example.split_whitespace().count() > 1),
                ":{verb} has no example with a value"
            );
        }
        for (form, _) in schema.forms {
            assert!(
                examples().any(|example| example.ends_with(&format!(" {form}"))),
                ":{verb} {form} has no example"
            );
        }
    }
}

/// `:gag-set` changes exactly the parameters the built-in recipes expose.
#[test]
fn gag_set_names_every_recipe_parameter() {
    let mut set = declared("gag-set").unwrap().option_names();
    let mut recipes = declared("gag").unwrap().option_names();
    set.sort_unstable();
    recipes.sort_unstable();
    assert_eq!(set, recipes);
}

/// The parser refuses a parameter the schema does not declare, and the
/// guide names it before Enter.
#[test]
fn undeclared_parameters_refuse_in_both() {
    for (verb, schema) in SCHEMAS {
        if schema.option_names().is_empty() && schema.variants.is_empty() {
            continue;
        }
        let Some(example) = schema
            .examples
            .iter()
            .find(|example| !schema.forms.iter().any(|(form, _)| example.ends_with(form)))
        else {
            continue;
        };
        if schema.rest_text {
            // Trailing text keeps unknown words as text in both.
            continue;
        }
        let input = format!("{example} bogus=1");
        assert!(parse(&input).is_err(), "{input} parsed");
        let guide = guided(&format!("{input} "));
        assert!(
            guide
                .problem
                .as_deref()
                .is_some_and(|problem| problem.contains("bogus=")),
            ":{verb}: {:?}",
            guide.problem
        );
    }
}

fn replace(example: &str, index: usize, word: &str) -> String {
    let mut words: Vec<String> = example.split_whitespace().map(str::to_owned).collect();
    words[index] = word.to_owned();
    words.join(" ")
}

/// Every declared choice is one the parser accepts, and a value that fits
/// no declared type is refused by both.
#[test]
fn declared_values_agree_with_the_parser() {
    for (verb, schema) in SCHEMAS {
        for example in schema.examples {
            let words: Vec<&str> = example.split_whitespace().collect();
            for (index, word) in words.iter().enumerate().skip(1) {
                let (param, option) = match split_option(word) {
                    Some((name, _)) => {
                        let variant = schema
                            .variants
                            .iter()
                            .find(|variant| words.get(1) == Some(&variant.value));
                        let Some(param) = schema
                            .options
                            .iter()
                            .chain(variant.into_iter().flat_map(|variant| variant.options))
                            .find(|param| param.name == name)
                        else {
                            continue;
                        };
                        (param, true)
                    }
                    None if index == 1 && !schema.rest_text && schema.variants.is_empty() => {
                        let Some(param) = schema.positional.first() else {
                            continue;
                        };
                        (param, false)
                    }
                    None if index == 2 && schema.variants.is_empty() && !schema.rest_text => {
                        let Some(param) = schema.positional.get(1) else {
                            continue;
                        };
                        (param, false)
                    }
                    None => continue,
                };
                let with = |value: &str| {
                    if option {
                        replace(example, index, &format!("{}={value}", param.name))
                    } else {
                        replace(example, index, value)
                    }
                };
                if let Kind::Choice(choices) | Kind::ChoiceOr(choices, _) = param.kind {
                    for choice in choices {
                        let input = with(choice);
                        assert!(
                            parse(&input).is_ok(),
                            ":{verb}: {input} {:?}",
                            parse(&input)
                        );
                        assert_eq!(guided(&format!("{input} ")).problem, None, "{input}");
                    }
                }
                if matches!(
                    param.kind,
                    Kind::Text | Kind::Name(Names::Target | Names::Gag)
                ) {
                    continue;
                }
                let input = with("x1y");
                assert!(parse(&input).is_err(), ":{verb}: {input} parsed");
                let problem = guided(&format!("{input} ")).problem.unwrap_or_default();
                let named = if option {
                    format!("{}=x1y", param.name)
                } else {
                    format!("{} x1y", param.name)
                };
                assert!(problem.starts_with(&named), ":{verb}: {problem}");
            }
        }
    }
}

/// A registry usage may only mention parameters its verb declares.
#[test]
fn registry_usages_name_only_declared_parameters() {
    for spec in SPECS {
        for command in spec.commands {
            let Some(schema) = declared(command.verb) else {
                continue;
            };
            let names = schema.option_names();
            for word in command.syntax().split_whitespace().skip(1) {
                let word = word.trim_matches(['[', ']']);
                if let Some((name, _)) = split_option(word)
                    && name != "key"
                {
                    assert!(
                        names.contains(&name),
                        ":{} usage names undeclared {name}=",
                        command.verb
                    );
                }
            }
        }
    }
}

#[test]
fn repeat_lists_its_parameters_with_defaults() {
    let guide = guided(":repeat 3 ");
    assert_eq!(guide.problem, None);
    assert!(
        guide
            .candidates
            .contains(&"gap=duration (keeps)".to_owned()),
        "{:?}",
        guide.candidates
    );
    assert!(
        guide
            .candidates
            .contains(&"pitch=preserve|tape (keeps)".to_owned())
    );
    assert!(
        guide.next.starts_with("optional parameters"),
        "{}",
        guide.next
    );
    // Before the count, the count is the expected value.
    let first = guided(":repeat ");
    assert!(
        first.next.starts_with("plays total plays"),
        "{}",
        first.next
    );
    assert!(
        first.next.contains("whole count: 1 or more"),
        "{}",
        first.next
    );
    assert!(
        first.next.contains("omitted keeps the current value"),
        "{}",
        first.next
    );
}

#[test]
fn enum_values_complete_after_their_name() {
    let guide = guided(":ramp 1 2 pitch=");
    assert_eq!(guide.candidates, ["pitch=preserve", "pitch=tape"]);
    assert!(
        guide.next.starts_with("pitch= keep pitch"),
        "{}",
        guide.next
    );
    assert!(guide.next.contains("default tape"), "{}", guide.next);
    assert_eq!(guided(":ramp 1 2 pitch=p").candidates, ["pitch=preserve"]);
    assert_eq!(
        guided(":ramp 1 2 pitch=q").problem.as_deref(),
        Some("pitch=q is not one of preserve, tape")
    );
    assert_eq!(guided(":edge h").candidates, ["hard"]);
    assert_eq!(guided(":hold 1s v").candidates, ["video="]);
}

#[test]
fn invalid_values_name_the_parameter() {
    assert_eq!(
        guided(":hold 1.5x ").problem.as_deref(),
        Some("duration 1.5x is not a duration: use 12f, 250ms, 1.5s or 01:02.500")
    );
    assert_eq!(
        guided(":repeat 3 gap=1.5x ").problem.as_deref(),
        Some("gap=1.5x is not a duration: use 6f, 120ms or 0 for no gaps")
    );
    assert_eq!(
        guided(":repeat 3 colour=red ").problem.as_deref(),
        Some(
            "colour= is not a parameter of :repeat; it takes gap, gap-step, speed-step, pitch, gain-step, zoom-step, progression, zoom-target, role, overflow, extend"
        )
    );
    assert_eq!(
        guided(":ramp 1 0 ").problem.as_deref(),
        Some("to 0 is not a speed: use 0.75 or 3/4, above zero")
    );
    assert_eq!(
        guided(":pitch +30st ").problem.as_deref(),
        Some("shift +30st is not a pitch shift: use +5st or -2st, whole semitones within ±24")
    );
    assert_eq!(
        guided(":split now ").problem.as_deref(),
        Some(":split takes no value now")
    );
    assert_eq!(
        guided(":repeat 3 gap=6f gap=2f ").problem.as_deref(),
        Some("gap= is given twice")
    );
    assert_eq!(
        guided(":zoom off 2").problem.as_deref(),
        Some(":zoom off takes nothing else")
    );
    assert_eq!(guided(":hold 12f ").problem, None);
}

#[test]
fn trailing_text_keeps_option_words_until_the_options() {
    let guide = guided(":caption the at=noon line at=t");
    assert_eq!(guide.candidates, ["at=top"]);
    assert_eq!(guide.problem, None);
    let guide = guided(":caption hello ");
    assert!(
        guide
            .candidates
            .iter()
            .any(|chip| chip == "at=bottom|top|center|middle (bottom)")
    );
}

#[test]
fn gag_recipes_choose_their_parameters() {
    let guide = guided(":gag one-more-time ");
    assert!(
        guide
            .candidates
            .iter()
            .any(|chip| chip.starts_with("vary="))
    );
    assert!(
        !guide
            .candidates
            .iter()
            .any(|chip| chip.starts_with("creep="))
    );
    assert_eq!(
        guided(":gag long-answer plays=3 ").problem.as_deref(),
        Some("plays= is not a parameter of :gag; it takes pause, creep")
    );
    let mut context = empty();
    context.gag_presets = Some(vec!["mine".into()]);
    let guide = super::guide(":gag mine pause=3s ", &context).unwrap();
    assert_eq!(guide.problem, None);
    assert_eq!(guide.next, "parameters of the saved preset mine");
    let guide = super::guide(":gag nope ", &context).unwrap();
    assert!(
        guide
            .problem
            .unwrap()
            .starts_with("gag nope is not a built-in gag or saved preset")
    );
}

#[test]
fn known_names_are_offered_as_values() {
    let mut context = empty();
    context.registers = vec![('a', "edited"), ('q', "macro")];
    context.targets = vec![("target-1".into(), "Speaker".into())];
    let guide = guide(":macro ", &context).unwrap();
    assert_eq!(guide.candidates, ["q (macro)"]);
    assert!(
        guide.next.starts_with("name the recorded macro to run"),
        "{}",
        guide.next
    );
    assert_eq!(guide_candidates(":recipe ", &context), ["a (edited)"]);
    assert_eq!(
        guide_candidates(":zoom 2 target=", &context),
        [
            "target=current",
            "target=center",
            "target=target-1 (Speaker)"
        ]
    );
    assert_eq!(
        guide_candidates(":zoom 2 target=S", &context),
        ["target=target-1 (Speaker)"]
    );
    assert_eq!(
        guide_candidates(":cutaway register=", &context),
        ["register=a (edited)", "register=q (macro)"]
    );
}

fn guide_candidates(input: &str, context: &Context) -> Vec<String> {
    guide(input, context).unwrap().candidates
}

#[test]
fn the_selector_names_what_the_command_acts_on_now() {
    let mut context = empty();
    assert_eq!(
        guided(":repeat 3 ").target,
        "the selected Repeat or beat (none selected)"
    );
    context.beat = Some(Beat {
        label: "Hello".into(),
        kind: "Repeat · 2 total plays".into(),
        frames: 48,
        gag: false,
    });
    assert_eq!(
        guide(":repeat 3 ", &context).unwrap().target,
        "the selected Repeat Hello (Repeat · 2 total plays, 48 f)"
    );
    context.beat.as_mut().unwrap().kind = "Source".into();
    assert_eq!(
        guide(":repeat 3 ", &context).unwrap().target,
        "Hello (Source, 48 f), wrapped in a new Repeat"
    );
    context.cursor = Some("00:01.000".into());
    assert_eq!(
        guide(":hold ", &context).unwrap().target,
        "the Edit cursor at 00:01.000"
    );
    context.visual = Some("[12..40) 28 f".into());
    assert_eq!(
        guide(":caption hi ", &context).unwrap().target,
        "the Visual range [12..40) 28 f"
    );
    assert_eq!(
        guide(":room-tone ", &context).unwrap().target,
        "the selected pause (the selected beat Hello is not a pause)"
    );
    assert_eq!(
        guide(":scope ", &context).unwrap().target,
        "the plays of Repeat contents (open them with Enter first)"
    );
    context.original = true;
    assert_eq!(
        guide(":retime ", &context).unwrap().target,
        "the selected beat Hello (Source, 48 f), in Your edit"
    );
}

#[test]
fn variants_add_their_own_values() {
    let guide = guided(":scope play ");
    assert!(
        guide.next.starts_with("play one-based play number"),
        "{}",
        guide.next
    );
    assert_eq!(
        guided(":scope plays 2 ").problem.as_deref(),
        Some("plays 2 is not a play list: use 2-3 or 1,3, at least two plays")
    );
}

#[test]
fn usage_lines_are_generated_from_the_schema() {
    assert_eq!(
        declared("hold").unwrap().usage("hold"),
        ":hold DURATION [video=freeze|black|ai] [audio=silence]"
    );
    assert_eq!(
        declared("retime").unwrap().usage("retime"),
        ":retime SPEED pitch=preserve|tape|pitch-shift"
    );
    assert_eq!(
        declared("zoom").unwrap().usage("zoom"),
        ":zoom SCALE [target=target] [curve=step|linear|smoothstep|smooth] | :zoom off"
    );
    assert_eq!(
        declared("caption").unwrap().usage("caption"),
        ":caption TEXT… [at=bottom|top|center|middle] [delay=duration] [reveal=whole-count] | :caption clear"
    );
}

#[test]
fn inferred_verbs_still_check_units() {
    assert!(declared("bleep").is_none());
    let bleep = guided(":bleep 880Hz level=x ");
    assert!(bleep.problem.is_some(), "{bleep:?}");
    assert!(guide(":ho", &empty()).is_none());
    assert!(guide(":unknown 1", &empty()).is_none());
}

#[test]
fn spoken_text_carries_every_part() {
    let spoken = guided(":ramp 1 2 pitch=").spoken();
    assert!(spoken.starts_with("Next: pitch= keep pitch"), "{spoken}");
    assert!(
        spoken.contains("Choices: pitch=preserve, pitch=tape."),
        "{spoken}"
    );
    assert!(
        spoken.contains("Acts on the selected beat (none selected)."),
        "{spoken}"
    );
    assert!(
        spoken.ends_with("Usage: :ramp FROM TO [pitch=preserve|tape] [steps=whole-count]."),
        "{spoken}"
    );
}
