//! Argument schemas derived from each registry usage, so completion, typed-unit
//! checks and the default target come from the one action table. A usage like
//! `:hold 0.5s [video=freeze|black|ai] [audio=silence]` declares a positional
//! duration, two choice options and, through its contexts, what it acts on.

use crate::navigation::registry::{Contexts, SPECS, Spec};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Seconds, milliseconds or project frames: `0.5s`, `120ms`, `12f`.
    Duration,
    Decibels,
    Semitones,
    Number,
    Choice(Vec<&'static str>),
    /// Free text, names and placeholders. Never rejected here.
    Text,
}

impl Kind {
    fn infer(example: &'static str) -> Self {
        if example.contains('|') {
            return Self::Choice(
                example
                    .split('|')
                    .filter(|choice| !choice.is_empty())
                    .collect(),
            );
        }
        let unsigned = example.trim_start_matches(['+', '-']);
        let digits = unsigned.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        let unit = &unsigned[digits.len()..];
        if digits.is_empty() || !is_number(digits) {
            return Self::Text;
        }
        match unit {
            "s" | "ms" | "f" => Self::Duration,
            "dB" => Self::Decibels,
            "st" => Self::Semitones,
            "" => Self::Number,
            _ => Self::Text,
        }
    }

    fn hint(&self) -> String {
        match self {
            Self::Duration => "duration in s, ms or f".into(),
            Self::Decibels => "level in dB".into(),
            Self::Semitones => "semitones, st".into(),
            Self::Number => "number".into(),
            Self::Choice(choices) => choices.join("|"),
            Self::Text => "text".into(),
        }
    }

    /// Whether a complete typed value fits. Text and placeholders always fit.
    fn accepts(&self, value: &str) -> bool {
        let unsigned = value.trim_start_matches(['+', '-']);
        let unit = |suffixes: &[&str]| {
            suffixes
                .iter()
                .any(|suffix| unsigned.strip_suffix(suffix).is_some_and(is_number))
        };
        match self {
            Self::Duration => unit(&["ms", "s", "f"]),
            Self::Decibels => unit(&["dB", "db"]),
            Self::Semitones => unit(&["st"]),
            Self::Number => is_number(unsigned),
            Self::Choice(choices) => choices
                .iter()
                .any(|choice| choice.eq_ignore_ascii_case(value)),
            Self::Text => true,
        }
    }
}

/// Decimal or rational (`7/8`) digits without sign or unit.
fn is_number(text: &str) -> bool {
    let decimal = |part: &str| {
        let mut halves = part.splitn(2, '.');
        let whole = halves.next().unwrap_or_default();
        let fraction = halves.next();
        !whole.is_empty()
            && whole.bytes().all(|byte| byte.is_ascii_digit())
            && fraction.is_none_or(|fraction| {
                !fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())
            })
    };
    match text.split_once('/') {
        Some((numerator, denominator)) => decimal(numerator) && decimal(denominator),
        None => decimal(text),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    /// `None` for a positional argument.
    pub name: Option<&'static str>,
    pub kind: Kind,
    pub example: &'static str,
}

#[derive(Clone, Debug)]
pub struct Schema {
    pub verb: &'static str,
    pub params: Vec<Param>,
    pub contexts: Contexts,
}

impl Schema {
    fn from_usage(verb: &'static str, usage: &'static str, contexts: Contexts) -> Self {
        // A usage may list whole alternative forms (`:gain +3dB | mute`); the
        // first form defines the arguments.
        let syntax = usage.split("  ").next().unwrap_or(usage);
        let syntax = syntax.split(" | ").next().unwrap_or(syntax);
        let params = syntax
            .split_whitespace()
            .skip(1)
            .map(|token| token.trim_matches(['[', ']']))
            .filter(|token| !token.is_empty() && !token.contains("..."))
            .map(|token| match token.split_once('=') {
                Some((name, example)) => Param {
                    name: Some(name),
                    kind: Kind::infer(example),
                    example,
                },
                None => Param {
                    name: None,
                    kind: Kind::infer(token),
                    example: token,
                },
            })
            .collect();
        Self {
            verb,
            params,
            contexts,
        }
    }

    fn option(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|param| {
            param
                .name
                .is_some_and(|known| known.eq_ignore_ascii_case(name))
        })
    }

    fn options(&self) -> impl Iterator<Item = &Param> {
        self.params.iter().filter(|param| param.name.is_some())
    }
}

/// The schema of `verb`, from the first registry usage that names it.
pub fn schema(verb: &str) -> Option<Schema> {
    SPECS.iter().find_map(|spec: &'static Spec| {
        spec.commands
            .iter()
            .find(|command| command.verb.eq_ignore_ascii_case(verb))
            .map(|command| Schema::from_usage(command.verb, command.usage, spec.contexts))
    })
}

/// What a command acts on when nothing more specific is selected, from the
/// contexts its registry entry admits.
pub fn default_target(contexts: Contexts) -> &'static str {
    let your_edit = contexts.intersects(Contexts::YOUR_EDIT);
    if contexts.contains(Contexts::VISUAL) && your_edit {
        "the Visual selection, else the Edit cursor"
    } else if your_edit {
        "the Edit cursor"
    } else if contexts.contains(Contexts::CONTENTS) {
        "the selected beat inside Repeat contents"
    } else if contexts.contains(Contexts::ORIGINAL) {
        "the Original cursor"
    } else if contexts.contains(Contexts::PLACED) {
        "the selected placed sound"
    } else if contexts.contains(Contexts::CATALOG) {
        "the selected catalog sound"
    } else {
        "this project"
    }
}

/// Guidance for a command line whose verb is complete: the next argument or
/// matching options, the first value that does not fit its typed unit, and
/// the default target. `None` before the verb is complete or for unknown verbs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guide {
    pub shown: String,
    pub problem: Option<String>,
    pub target: &'static str,
}

pub fn guide(input: &str) -> Option<Guide> {
    let input = input.trim_start();
    let input = input.strip_prefix(':').unwrap_or(input);
    let (verb, rest) = input.split_once(char::is_whitespace)?;
    let schema = schema(verb)?;
    let ends_open = !rest.is_empty() && !rest.ends_with(char::is_whitespace);
    let mut tokens: Vec<&str> = rest.split_whitespace().collect();
    let partial = if ends_open { tokens.pop() } else { None };

    let mut positionals = schema.params.iter().filter(|param| param.name.is_none());
    let mut used = Vec::new();
    let mut problem = None;
    for token in &tokens {
        let param = match token.split_once('=') {
            Some((name, value)) => match schema.option(name) {
                Some(param) => {
                    used.push(param.name);
                    Some((param, value))
                }
                None => {
                    problem.get_or_insert_with(|| unknown_option(&schema, name));
                    None
                }
            },
            None => positionals.next().map(|param| (param, *token)),
        };
        if let Some((param, value)) = param
            && !param.kind.accepts(value)
        {
            problem.get_or_insert_with(|| format!("{value} is not {}", describe(param)));
        }
    }

    let shown = match partial.map(|partial| (partial, partial.split_once('='))) {
        Some((_, Some((name, value)))) => match schema.option(name) {
            Some(param) => match &param.kind {
                Kind::Choice(choices) => {
                    let matching: Vec<_> = choices
                        .iter()
                        .filter(|choice| {
                            choice
                                .to_ascii_lowercase()
                                .starts_with(&value.to_ascii_lowercase())
                        })
                        .copied()
                        .collect();
                    if matching.is_empty() {
                        problem
                            .get_or_insert_with(|| format!("{value} is not {}", describe(param)));
                    }
                    format!("{name}={}", matching.join("|"))
                }
                kind => format!("{name}=  {}, e.g. {}", kind.hint(), param.example),
            },
            None => {
                problem.get_or_insert_with(|| unknown_option(&schema, name));
                options_left(&schema, &used)
            }
        },
        Some((partial, None)) => {
            let matching: Vec<_> = schema
                .options()
                .filter(|param| !used.contains(&param.name))
                .filter_map(|param| param.name)
                .filter(|name| name.starts_with(&partial.to_ascii_lowercase()))
                .map(|name| format!("{name}="))
                .collect();
            match positionals.next() {
                Some(param) if matching.is_empty() => next_positional(param),
                _ if !matching.is_empty() => matching.join("  "),
                _ => options_left(&schema, &used),
            }
        }
        None => match positionals.next() {
            Some(param) => {
                let mut shown = next_positional(param);
                let options = options_left(&schema, &used);
                if !options.is_empty() {
                    shown.push_str("  ·  ");
                    shown.push_str(&options);
                }
                shown
            }
            None => options_left(&schema, &used),
        },
    };
    Some(Guide {
        shown,
        problem,
        target: default_target(schema.contexts),
    })
}

fn describe(param: &Param) -> String {
    match (&param.kind, param.name) {
        (Kind::Choice(choices), Some(name)) => format!("a {name} choice: {}", choices.join(", ")),
        (Kind::Choice(choices), None) => format!("one of {}", choices.join(", ")),
        (kind, _) => format!("a {}", kind.hint()),
    }
}

fn next_positional(param: &Param) -> String {
    format!("{}, e.g. {}", param.kind.hint(), param.example)
}

fn options_left(schema: &Schema, used: &[Option<&'static str>]) -> String {
    schema
        .options()
        .filter(|param| !used.contains(&param.name))
        .map(|param| {
            let value = match param.kind {
                Kind::Text => param.example.to_owned(),
                ref kind => kind.hint(),
            };
            format!("{}={value}", param.name.unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn unknown_option(schema: &Schema, name: &str) -> String {
    let known: Vec<_> = schema.options().filter_map(|param| param.name).collect();
    if known.is_empty() {
        format!(":{} takes no {name}= option", schema.verb)
    } else {
        format!(
            "Unknown option {name}=; :{} takes {}",
            schema.verb,
            known.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usages_declare_typed_positionals_and_choice_options() {
        let hold = schema("hold").unwrap();
        assert_eq!(hold.params[0].kind, Kind::Duration);
        assert_eq!(
            hold.option("video").unwrap().kind,
            Kind::Choice(vec!["freeze", "black", "ai"])
        );
        assert_eq!(schema("gain").unwrap().params[0].kind, Kind::Decibels);
        assert_eq!(schema("pitch").unwrap().params[0].kind, Kind::Semitones);
    }

    #[test]
    fn every_registry_usage_yields_a_schema() {
        for spec in SPECS {
            for command in spec.commands {
                assert!(schema(command.verb).is_some(), "{}", command.verb);
            }
        }
    }

    #[test]
    fn completion_follows_the_typed_position() {
        let guide = guide(":hold ").unwrap();
        assert!(
            guide.shown.starts_with("duration in s, ms or f, e.g. 0.5s"),
            "{}",
            guide.shown
        );
        assert!(
            guide.shown.contains("video=freeze|black|ai"),
            "{}",
            guide.shown
        );
        assert_eq!(guide.target, "the Edit cursor");

        assert_eq!(guide_shown(":hold 1s v"), "video=");
        assert_eq!(guide_shown(":hold 1s video=b"), "video=black");
        assert_eq!(guide_shown(":hold 1s video=black "), "audio=silence");
    }

    #[test]
    fn values_that_do_not_fit_their_unit_are_named_before_enter() {
        assert_eq!(
            guide(":hold 1.5x ").unwrap().problem.as_deref(),
            Some("1.5x is not a duration in s, ms or f")
        );
        assert!(guide(":hold 12f ").unwrap().problem.is_none());
        assert!(guide(":hold 120ms ").unwrap().problem.is_none());
        assert_eq!(
            guide(":hold 1s video=grey").unwrap().problem.as_deref(),
            Some("grey is not a video choice: freeze, black, ai")
        );
        assert_eq!(
            guide(":hold 1s colour=red ").unwrap().problem.as_deref(),
            Some("Unknown option colour=; :hold takes video, audio")
        );
        assert!(guide(":gain -6dB ").unwrap().problem.is_none());
        assert!(guide(":gain loud ").unwrap().problem.is_some());
    }

    #[test]
    fn incomplete_verbs_have_no_argument_guide() {
        assert!(guide(":ho").is_none());
        assert!(guide(":unknown 1").is_none());
    }

    fn guide_shown(input: &str) -> String {
        guide(input).unwrap().shown
    }
}
