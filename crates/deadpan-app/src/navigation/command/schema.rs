//! Declarative argument schemas for command verbs. Each schema names a verb's
//! positional values and `key=value` parameters with their types, units and
//! defaults, and the selector the command acts on. The command line completes
//! and checks arguments from it while typing, the generated reference prints
//! its usage line and parameter table, and the per-verb parsers that execute
//! a command are tested to accept exactly the names and values it declares.
//!
//! Every registry verb declares a schema; a test enforces it.

#[cfg(test)]
use crate::navigation::registry::SPECS;

mod table;
#[cfg(test)]
mod tests;
mod types;

pub use table::SCHEMAS;

/// A typed value. `check` delegates to the same value parser the verb's
/// execution path uses, so a value the guide accepts is one Enter accepts.
pub struct Type {
    pub name: &'static str,
    /// The accepted forms, with units and range: `12f, 250ms, 1.5s or 01:02.500`.
    pub units: &'static str,
    pub check: fn(&str) -> bool,
}

impl std::fmt::Debug for Type {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name)
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.units == other.units
    }
}

/// Names the project or the user's library already holds. The command line
/// offers the known ones as values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Names {
    /// Any register letter a–z, or `"` for the unnamed register.
    Register,
    /// A register holding a recorded macro.
    Macro,
    /// A register holding an edited copy (a local recipe).
    Recipe,
    /// A mark letter.
    Mark,
    /// A saved attention target, by id or label.
    Target,
    /// A built-in gag recipe or a saved preset.
    Gag,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Typed(&'static Type),
    Choice(&'static [&'static str]),
    /// One of the words, or a typed value: `pitch=preserve|tape|+2st`.
    ChoiceOr(&'static [&'static str], &'static Type),
    Name(Names),
    /// Free text; never rejected while typing.
    Text,
    /// Free text to the end of the line, spaces and `=` included
    /// (`text=Keep the hands still.`); only as a parameter, and last.
    Rest,
}

/// What omitting a parameter means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Default {
    Required,
    Value(&'static str),
    /// Omitted parameters keep the target's current value.
    Keeps,
    Means(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub default: Default,
    pub help: &'static str,
}

pub const fn param(name: &'static str, kind: Kind, default: Default, help: &'static str) -> Param {
    Param {
        name,
        kind,
        default,
        help,
    }
}

/// Further values and parameters chosen by the first positional value, such
/// as a gag recipe's parameters or `:scope play N`.
#[derive(Clone, Copy, Debug)]
pub struct Variant {
    pub value: &'static str,
    pub positional: &'static [Param],
    pub options: &'static [Param],
}

/// What a command acts on when nothing more specific is named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector {
    /// The selected beat in Your edit.
    Beat,
    /// The Visual range, else the selected beat.
    VisualOrBeat,
    /// The selected Repeat, or the selected beat wrapped in a new one.
    RepeatOrBeat,
    /// A new Repeat around the Visual range or selected beat.
    WrapVisualOrBeat,
    /// Inserts at the Edit cursor.
    Cursor,
    /// The beat under the Edit cursor, cut at the cursor.
    BeatAtCursor,
    /// The selected beat, else the Edit cursor.
    BeatOrCursor,
    /// The selected pause.
    Pause,
    /// The selected pause, else a new pause at the Edit cursor.
    PauseOrCursor,
    /// The selected Source beat (and its right neighbour for Roll).
    SourceBeat,
    /// The enclosing Repeat contents' plays.
    Plays,
    /// The selected inserted gag.
    Gag,
    /// The cursor of the focused timeline.
    TimelineCursor,
    /// The next copy, cut or paste.
    NextRegister,
    /// The Edit cursor and selection, as the macro recorded them.
    Macro,
    /// The selected placed sound.
    Sound,
    /// The whole project.
    Project,
    /// The view or panel only; nothing is edited.
    View,
}

/// A rule between parameters that each value check alone cannot see. The
/// command line reports a broken rule while typing; tests prove the parser
/// refuses the same combination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Constraint {
    /// The first parameter cannot be combined with any of the others; the
    /// text says why.
    Excludes(&'static str, &'static [&'static str], &'static str),
    /// The first parameter needs the second.
    Requires(&'static str, &'static str),
}

impl Constraint {
    /// The first broken rule among the given parameter names.
    fn broken(self, used: &[&str]) -> Option<String> {
        match self {
            Self::Excludes(name, others, why) if used.contains(&name) => others
                .iter()
                .find(|other| used.contains(other))
                .map(|other| format!("{name}= cannot be combined with {other}=: {why}")),
            Self::Requires(name, needed) if used.contains(&name) && !used.contains(&needed) => {
                Some(format!("{name}= needs {needed}="))
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn describe(self) -> String {
        match self {
            Self::Excludes(name, others, why) => format!(
                "`{name}=` cannot be combined with {}: {why}",
                others
                    .iter()
                    .map(|other| format!("`{other}=`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Requires(name, needed) => format!("`{name}=` needs `{needed}=`"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArgSchema {
    pub selector: Selector,
    pub positional: &'static [Param],
    pub options: &'static [Param],
    /// Whole alternative forms (`off`, `clear`) and what they do.
    pub forms: &'static [(&'static str, &'static str)],
    /// The last positional takes every remaining plain word (text, labels).
    pub rest_text: bool,
    pub variants: &'static [Variant],
    /// Rules between parameters.
    pub constraints: &'static [Constraint],
    /// Complete valid commands; tests run each through the parser.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the parser agreement tests")
    )]
    pub examples: &'static [&'static str],
}

impl ArgSchema {
    pub const fn none(selector: Selector) -> Self {
        Self {
            selector,
            positional: &[],
            options: &[],
            forms: &[],
            rest_text: false,
            variants: &[],
            constraints: &[],
            examples: &[],
        }
    }

    #[cfg(test)]
    pub fn takes_arguments(&self) -> bool {
        !self.positional.is_empty() || !self.options.is_empty() || !self.forms.is_empty()
    }

    fn variant(&self, value: &str) -> Option<&'static Variant> {
        self.variants.iter().find(|variant| variant.value == value)
    }

    /// Every parameter name the verb accepts, including every variant's.
    pub fn option_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = self.options.iter().map(|param| param.name).collect();
        for variant in self.variants {
            for param in variant.options {
                if !names.contains(&param.name) {
                    names.push(param.name);
                }
            }
        }
        names
    }

    /// The usage line: `:repeat [PLAYS] [gap=DURATION] …`.
    pub fn usage(&self, verb: &str) -> String {
        let mut parts = vec![format!(":{verb}")];
        for param in self.positional {
            let name = param.name.to_ascii_uppercase();
            let name = if self.rest_text && std::ptr::eq(param, self.positional.last().unwrap()) {
                format!("{name}…")
            } else {
                name
            };
            parts.push(match param.default {
                Default::Required => name,
                _ => format!("[{name}]"),
            });
        }
        if !self.variants.is_empty() && self.variants.iter().any(|v| !v.options.is_empty()) {
            parts.push("[key=value …]".into());
        }
        for param in self.options {
            let value = format!("{}={}", param.name, short_hint(&param.kind));
            parts.push(match param.default {
                Default::Required => value,
                _ => format!("[{value}]"),
            });
        }
        let mut usage = parts.join(" ");
        for (form, _) in self.forms {
            usage.push_str(&format!(" | :{verb} {form}"));
        }
        usage
    }
}

/// A verb's schema. Every registry verb declares one.
pub fn lookup(verb: &str) -> Option<&'static ArgSchema> {
    declared(verb)
}

pub fn declared(verb: &str) -> Option<&'static ArgSchema> {
    SCHEMAS
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(verb))
        .map(|(_, schema)| *schema)
}

/// Parameter names a migrated verb's parser admits. Parsers read this rather
/// than keeping their own list.
pub fn option_names(verb: &str) -> Vec<&'static str> {
    declared(verb).map_or_else(Vec::new, ArgSchema::option_names)
}

/// The live state a command would act on, and the names it may use.
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// The Original pane has focus: Edit commands still act on Your edit.
    pub original: bool,
    /// The Visual range, as the footer names it.
    pub visual: Option<String>,
    pub beat: Option<Beat>,
    /// The Edit cursor position, readable.
    pub cursor: Option<String>,
    /// Repeat contents scope, when open.
    pub contents: Option<String>,
    pub sound: Option<String>,
    /// Filled registers with what they hold: `edited`, `Original`, `macro`.
    pub registers: Vec<(char, &'static str)>,
    /// Saved attention targets: id and label.
    pub targets: Vec<(String, String)>,
    /// Saved gag presets; `None` when they could not be read.
    pub gag_presets: Option<Vec<String>>,
    pub marks: Vec<char>,
}

#[derive(Clone, Debug, Default)]
pub struct Beat {
    pub label: String,
    /// The footer's kind text: `Source`, `Pause · freeze · silence`, `Repeat · 3 total plays`.
    pub kind: String,
    pub frames: u64,
    /// The label pins a gag recipe.
    pub gag: bool,
}

impl Beat {
    fn text(&self) -> String {
        format!("{} ({}, {} f)", self.label, self.kind, self.frames)
    }
    fn is(&self, kind: &str) -> bool {
        self.kind.starts_with(kind)
    }
}

impl Selector {
    /// The selector without live context, for the reference.
    pub fn generic(self) -> &'static str {
        match self {
            Self::Beat => "the selected beat",
            Self::VisualOrBeat => "the Visual range, else the selected beat",
            Self::RepeatOrBeat => {
                "the selected Repeat, else the selected beat wrapped in a new Repeat"
            }
            Self::WrapVisualOrBeat => {
                "a new Repeat around the Visual range, else the selected beat"
            }
            Self::Cursor => "the Edit cursor",
            Self::BeatAtCursor => "the beat under the Edit cursor, cut at the cursor",
            Self::BeatOrCursor => "the selected beat, else the Edit cursor",
            Self::Pause => "the selected pause",
            Self::PauseOrCursor => "the selected pause, else a new pause at the Edit cursor",
            Self::SourceBeat => "the selected Source beat",
            Self::Plays => "the plays of the open Repeat contents",
            Self::Gag => "the selected inserted gag",
            Self::TimelineCursor => "the cursor of the focused timeline",
            Self::NextRegister => "the next copy, cut or paste",
            Self::Macro => "the Edit cursor and selection, as recorded",
            Self::Sound => "the selected placed sound",
            Self::Project => "this project",
            Self::View => "the view only; nothing is edited",
        }
    }

    /// What this command acts on now.
    pub fn resolve(self, context: &Context) -> String {
        let beat = context.beat.as_ref();
        let none = "none selected";
        let cursor = || match &context.cursor {
            Some(cursor) => format!("the Edit cursor at {cursor}"),
            None => "the Edit cursor".to_owned(),
        };
        let selected = |what: &str| match beat {
            Some(beat) => format!("{what} {}", beat.text()),
            None => format!("{what} ({none})"),
        };
        let text = match self {
            Self::Beat => selected("the selected beat"),
            Self::VisualOrBeat => match &context.visual {
                Some(range) => format!("the Visual range {range}"),
                None => selected("the selected beat"),
            },
            Self::RepeatOrBeat => match beat {
                Some(beat) if beat.is("Repeat") => format!("the selected Repeat {}", beat.text()),
                Some(beat) => format!("{}, wrapped in a new Repeat", beat.text()),
                None => format!("the selected Repeat or beat ({none})"),
            },
            Self::WrapVisualOrBeat => match (&context.visual, beat) {
                (Some(range), _) => format!("a new Repeat around the Visual range {range}"),
                (None, Some(beat)) => format!("a new Repeat around {}", beat.text()),
                (None, None) => format!("a new Repeat around the selected beat ({none})"),
            },
            Self::Cursor => cursor(),
            Self::BeatAtCursor => format!("the beat under {}", cursor()),
            Self::BeatOrCursor => match beat {
                Some(beat) => format!("the selected beat {}", beat.text()),
                None => cursor(),
            },
            Self::Pause => match beat {
                Some(beat) if beat.is("Pause") => format!("the selected pause {}", beat.text()),
                Some(beat) => format!(
                    "the selected pause (the selected beat {} is not a pause)",
                    beat.label
                ),
                None => format!("the selected pause ({none})"),
            },
            Self::PauseOrCursor => match beat {
                Some(beat) if beat.is("Pause") => format!("the selected pause {}", beat.text()),
                _ => format!("a new pause at {}", cursor()),
            },
            Self::SourceBeat => match beat {
                Some(beat) if beat.is("Source") || beat.is("Fragment") => {
                    format!("the selected Source beat {}", beat.text())
                }
                Some(beat) => format!(
                    "the selected Source beat (the selected beat {} is a {})",
                    beat.label, beat.kind
                ),
                None => format!("the selected Source beat ({none})"),
            },
            Self::Plays => match &context.contents {
                Some(scope) => format!("the plays of {scope}"),
                None => "the plays of Repeat contents (open them with Enter first)".to_owned(),
            },
            Self::Gag => match beat {
                Some(beat) if beat.gag => format!("the selected gag {}", beat.label),
                Some(beat) => format!(
                    "the selected inserted gag (the selected beat {} is not one)",
                    beat.label
                ),
                None => format!("the selected inserted gag ({none})"),
            },
            Self::Sound => match &context.sound {
                Some(sound) => format!("the placed sound {sound}"),
                None => format!("the selected placed sound ({none})"),
            },
            other => other.generic().to_owned(),
        };
        let edit = !matches!(
            self,
            Self::TimelineCursor
                | Self::NextRegister
                | Self::Project
                | Self::View
                | Self::Sound
                | Self::Plays
        );
        if context.original && edit {
            format!("{text}, in Your edit")
        } else {
            text
        }
    }
}

/// Guidance for a command line whose verb is complete. `None` before the
/// verb is complete or for unknown verbs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guide {
    /// The expected next value: name, meaning, type and units, default.
    pub next: String,
    /// Values or parameters to type next, as shown.
    pub candidates: Vec<String>,
    /// For each candidate, the text Tab puts in place of the word being
    /// typed (or after the last word): `gap=`, `preserve`, `q`.
    pub inserts: Vec<String>,
    /// The first value or parameter that does not fit, naming it.
    pub problem: Option<String>,
    /// What the command acts on now.
    pub target: String,
    /// The verb's usage line.
    pub usage: String,
}

impl Guide {
    /// One sentence for assistive technology and hover.
    pub fn spoken(&self) -> String {
        let mut parts = Vec::new();
        if let Some(problem) = &self.problem {
            parts.push(format!("Problem: {problem}."));
        }
        parts.push(format!("Next: {}.", self.next));
        if !self.candidates.is_empty() {
            parts.push(format!("Choices: {}.", self.candidates.join(", ")));
        }
        parts.push(format!("Acts on {}.", self.target));
        parts.push(format!("Usage: {}.", self.usage));
        parts.join(" ")
    }
}

fn short_hint(kind: &Kind) -> String {
    match kind {
        Kind::Typed(ty) => ty.name.replace(' ', "-"),
        Kind::Choice(choices) => choices.join("|"),
        Kind::ChoiceOr(choices, ty) => {
            format!("{}|{}", choices.join("|"), ty.name.replace(' ', "-"))
        }
        Kind::Name(names) => names.hint().to_owned(),
        Kind::Text | Kind::Rest => "text".into(),
    }
}

impl Names {
    fn hint(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Macro => "macro",
            Self::Recipe => "recipe",
            Self::Mark => "mark",
            Self::Target => "target",
            Self::Gag => "gag",
        }
    }

    fn long(self) -> &'static str {
        match self {
            Self::Register => "register letter a–z",
            Self::Macro => "macro register a–z",
            Self::Recipe => "recipe register a–z",
            Self::Mark => "mark letter a–z",
            Self::Target => "target id or label, current, center, face:N or region:N",
            Self::Gag => "built-in gag or saved preset",
        }
    }

    fn known(self, context: &Context) -> Vec<(String, String)> {
        let registers = |kind: Option<&str>| {
            context
                .registers
                .iter()
                .filter(|(_, held)| kind.is_none_or(|kind| *held == kind))
                .map(|(name, held)| (name.to_string(), format!("{name} ({held})")))
                .collect()
        };
        match self {
            Self::Register => registers(None),
            Self::Macro => registers(Some("macro")),
            Self::Recipe => registers(Some("edited")),
            Self::Mark => context
                .marks
                .iter()
                .map(|mark| (mark.to_string(), mark.to_string()))
                .collect(),
            Self::Target => ["current", "center"]
                .into_iter()
                .map(|word| (word.to_owned(), word.to_owned()))
                .chain(context.targets.iter().map(|(id, label)| {
                    let value = if label.contains(char::is_whitespace) || label.is_empty() {
                        id.clone()
                    } else {
                        label.clone()
                    };
                    (value, format!("{id} ({label})"))
                }))
                .collect(),
            Self::Gag => crate::navigation::gag::BUILT_IN
                .iter()
                .map(|name| ((*name).to_owned(), (*name).to_owned()))
                .chain(
                    context
                        .gag_presets
                        .iter()
                        .flatten()
                        .map(|name| (name.clone(), format!("{name} (preset)"))),
                )
                .collect(),
        }
    }

    fn check(self, value: &str, context: &Context) -> Result<(), String> {
        let letter = |quote: bool| {
            let mut chars = value.chars();
            matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_alphabetic() || (quote && c == '"'))
        };
        match self {
            Self::Register if letter(true) => Ok(()),
            Self::Macro | Self::Recipe | Self::Mark if letter(false) => Ok(()),
            Self::Register | Self::Macro | Self::Recipe | Self::Mark => {
                Err(format!("is not a {}", self.long()))
            }
            Self::Target => crate::navigation::zoom::target(value)
                .map(drop)
                .map_err(|_| format!("is not a {}", self.long())),
            Self::Gag => {
                if crate::navigation::gag::BUILT_IN.contains(&value) {
                    return Ok(());
                }
                match &context.gag_presets {
                    Some(presets) if !presets.iter().any(|name| name == value) => Err(format!(
                        "is not a built-in gag or saved preset: {}",
                        self.known(context)
                            .into_iter()
                            .map(|(value, _)| value)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                    _ => Ok(()),
                }
            }
        }
    }
}

impl Kind {
    fn long(&self) -> String {
        match self {
            Self::Typed(ty) => format!("{}: {}", ty.name, ty.units),
            Self::Choice(choices) => format!("one of {}", choices.join(", ")),
            Self::ChoiceOr(choices, ty) => {
                format!("{}, or {}: {}", choices.join(", "), ty.name, ty.units)
            }
            Self::Name(names) => names.long().to_owned(),
            Self::Text => "text".into(),
            Self::Rest => "text to the end of the line".into(),
        }
    }

    fn check(&self, value: &str, context: &Context) -> Result<(), String> {
        match self {
            Self::Typed(ty) if (ty.check)(value) => Ok(()),
            Self::Typed(ty) => Err(format!("is not a {}: use {}", ty.name, ty.units)),
            Self::Choice(choices) if choices.contains(&value) => Ok(()),
            Self::Choice(choices) => Err(format!("is not one of {}", choices.join(", "))),
            Self::ChoiceOr(choices, ty) if choices.contains(&value) || (ty.check)(value) => Ok(()),
            Self::ChoiceOr(choices, ty) => Err(format!(
                "is not {} or a {}: use {}",
                choices.join(", "),
                ty.name,
                ty.units
            )),
            Self::Name(names) => names.check(value, context),
            Self::Text | Self::Rest => Ok(()),
        }
    }

    /// Complete values to offer for a typed prefix: value and label.
    fn candidates(&self, prefix: &str, context: &Context) -> Vec<(String, String)> {
        let words: Vec<(String, String)> = match self {
            Self::Choice(choices) | Self::ChoiceOr(choices, _) => choices
                .iter()
                .map(|choice| ((*choice).to_owned(), (*choice).to_owned()))
                .collect(),
            Self::Name(names) => names.known(context),
            Self::Typed(_) | Self::Text | Self::Rest => Vec::new(),
        };
        words
            .into_iter()
            .filter(|(value, _)| value.starts_with(prefix))
            .collect()
    }

    fn lists_values(&self) -> bool {
        matches!(self, Self::Choice(_) | Self::ChoiceOr(..) | Self::Name(_))
    }
}

impl Default {
    fn short(self) -> String {
        match self {
            Self::Required => "required".into(),
            Self::Value(value) => value.into(),
            Self::Keeps => "keeps".into(),
            Self::Means(meaning) => meaning.into(),
        }
    }

    pub fn long(self) -> String {
        match self {
            Self::Required => "required".into(),
            Self::Value(value) => format!("default {value}"),
            Self::Keeps => "omitted keeps the current value".into(),
            Self::Means(meaning) => format!("omitted: {meaning}"),
        }
    }
}

impl Param {
    fn describe(&self, option: bool) -> String {
        let name = if option {
            format!("{}=", self.name)
        } else {
            self.name.to_owned()
        };
        format!(
            "{name} {} · {} · {}",
            self.help,
            self.kind.long(),
            self.default.long()
        )
    }

    fn chip(&self) -> String {
        format!(
            "{}={} ({})",
            self.name,
            short_hint(&self.kind),
            self.default.short()
        )
    }
}

/// Split arguments into words, keeping `"quoted text"` (also after `key=`)
/// together. The second value is whether the last word is still being typed.
fn words(rest: &str) -> (Vec<&str>, bool) {
    let mut words = Vec::new();
    let mut start = None;
    let mut quoted = false;
    for (index, character) in rest.char_indices() {
        if character == '"' {
            quoted = !quoted;
        }
        match (start, character.is_whitespace() && !quoted) {
            (None, false) => start = Some(index),
            (Some(begin), true) => {
                words.push(&rest[begin..index]);
                start = None;
            }
            _ => {}
        }
    }
    let open = start.is_some();
    if let Some(begin) = start {
        words.push(&rest[begin..]);
    }
    (words, open)
}

/// `key=value` when `key` looks like a parameter name.
fn split_option(word: &str) -> Option<(&str, &str)> {
    let (key, value) = word.split_once('=')?;
    (!key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && key.as_bytes()[0].is_ascii_alphabetic())
    .then_some((key, value))
}

struct Walk<'s> {
    verb: &'s str,
    schema: &'static ArgSchema,
    variant: Option<&'static Variant>,
    /// A saved gag preset takes its own recipe's parameters.
    free_options: bool,
    index: usize,
    used: Vec<&'static str>,
    /// A parameter that takes the rest of the line has begun.
    rest_option: Option<&'static Param>,
    problem: Option<String>,
}

impl<'s> Walk<'s> {
    fn positional(&self) -> Vec<&'static Param> {
        self.schema
            .positional
            .iter()
            .chain(
                self.variant
                    .into_iter()
                    .flat_map(|variant| variant.positional),
            )
            .collect()
    }

    /// A chosen variant's own parameters come first.
    fn options(&self) -> Vec<&'static Param> {
        self.variant
            .into_iter()
            .flat_map(|variant| variant.options)
            .chain(self.schema.options.iter())
            .collect()
    }

    fn option(&self, name: &str) -> Option<&'static Param> {
        self.options().into_iter().find(|param| param.name == name)
    }

    fn rest(&self) -> bool {
        self.schema.rest_text && self.index + 1 >= self.positional().len()
    }

    fn fail(&mut self, problem: String) {
        self.problem.get_or_insert(problem);
    }

    fn unknown(&self, name: &str) -> String {
        let known: Vec<_> = self.options().iter().map(|param| param.name).collect();
        if known.is_empty() {
            format!("{name}= is not a parameter of :{}", self.verb)
        } else {
            format!(
                "{name}= is not a parameter of :{}; it takes {}",
                self.verb,
                known.join(", ")
            )
        }
    }

    fn take(&mut self, word: &str, option_like: bool, context: &Context) {
        if option_like && let Some((name, value)) = split_option(word) {
            if self.free_options {
                return;
            }
            match self.option(name) {
                None => {
                    let problem = self.unknown(name);
                    self.fail(problem);
                }
                Some(param) if self.used.contains(&param.name) => {
                    self.fail(format!("{name}= is given twice"));
                }
                Some(param) => {
                    self.used.push(param.name);
                    if param.kind == Kind::Rest {
                        self.rest_option = Some(param);
                    }
                    if let Err(why) = param.kind.check(value, context) {
                        self.fail(format!("{name}={value} {why}"));
                    }
                }
            }
            return;
        }
        let positional = self.positional();
        let Some(param) = positional.get(self.index).copied() else {
            let problem = if positional.is_empty() {
                format!(":{} takes no value {word}", self.verb)
            } else {
                format!(
                    "{word} is extra; :{} takes {}",
                    self.verb,
                    positional
                        .iter()
                        .map(|param| param.name)
                        .collect::<Vec<_>>()
                        .join(" then ")
                )
            };
            self.fail(problem);
            return;
        };
        if let Err(why) = param.kind.check(word, context) {
            self.fail(format!("{} {word} {why}", param.name));
        }
        if self.index == 0 && !self.schema.variants.is_empty() {
            self.variant = self.schema.variant(word);
            // Only a saved gag preset brings parameters of its own.
            self.free_options = self.variant.is_none() && param.kind == Kind::Name(Names::Gag);
        }
        if !self.rest() {
            self.index += 1;
        }
    }

    /// Parameters not given yet that the given ones still permit.
    fn options_left(&self) -> Vec<&'static Param> {
        self.options()
            .into_iter()
            .filter(|param| !self.used.contains(&param.name))
            .filter(|param| {
                let mut used = self.used.clone();
                used.push(param.name);
                self.schema
                    .constraints
                    .iter()
                    .all(|constraint| match constraint {
                        Constraint::Excludes(..) => constraint.broken(&used).is_none(),
                        Constraint::Requires(..) => true,
                    })
            })
            .collect()
    }
}

pub fn guide(input: &str, context: &Context) -> Option<Guide> {
    let input = input.trim_start();
    let input = input.strip_prefix(':').unwrap_or(input);
    let (verb, rest) = input.split_once(char::is_whitespace)?;
    let schema = lookup(verb)?;
    let verb = verb.to_ascii_lowercase();
    let verb = verb.as_str();
    let target = schema.selector.resolve(context);
    let (mut all, open) = words(rest);
    let partial = if open { all.pop() } else { None };

    // A whole alternative form such as `:zoom off` takes nothing else.
    if let Some(form) = all
        .first()
        .and_then(|first| schema.forms.iter().find(|(word, _)| word == first))
    {
        let extra = all.len() > 1 || partial.is_some();
        return Some(Guide {
            next: format!(":{verb} {} {}", form.0, form.1),
            candidates: Vec::new(),
            inserts: Vec::new(),
            problem: extra.then(|| format!(":{verb} {} takes nothing else", form.0)),
            target,
            usage: schema.usage(verb),
        });
    }

    // Before a verb's trailing text ends, `at=` and the like are words of it.
    let text_until = if schema.rest_text {
        all.iter()
            .rposition(|word| {
                split_option(word).is_none_or(|(name, _)| !schema.option_names().contains(&name))
            })
            .map_or(0, |last| last + 1)
    } else {
        0
    };
    let mut walk = Walk {
        verb,
        schema,
        variant: None,
        free_options: false,
        index: 0,
        used: Vec::new(),
        rest_option: None,
        problem: None,
    };
    for (position, word) in all.iter().enumerate() {
        if walk.rest_option.is_some() {
            break;
        }
        walk.take(word, position >= text_until, context);
    }
    let partial = match partial {
        Some(partial) if walk.rest_option.is_none() => {
            // The word being typed may itself begin the rest of the line.
            if let Some((name, _)) = split_option(partial)
                && let Some(param) = walk.option(name)
                && param.kind == Kind::Rest
                && !walk.used.contains(&param.name)
            {
                walk.used.push(param.name);
                walk.rest_option = Some(param);
            }
            Some(partial)
        }
        _ => partial,
    };
    if let Some(param) = walk.rest_option {
        let mut used = walk.used.clone();
        used.push(param.name);
        let problem = walk.problem.take().or_else(|| {
            schema
                .constraints
                .iter()
                .find_map(|constraint| constraint.broken(&used))
        });
        return Some(Guide {
            next: param.describe(true),
            candidates: Vec::new(),
            inserts: Vec::new(),
            problem,
            target,
            usage: schema.usage(verb),
        });
    }

    let positional = walk.positional();
    let next_positional = positional.get(walk.index).copied();
    // Each offer is what the row shows and what Tab inserts.
    let mut offers: Vec<(String, String)> = Vec::new();
    let mut typed_option = None;
    let next = match partial {
        Some(partial) if split_option(partial).is_some() && !walk.free_options => {
            let (name, prefix) = split_option(partial).expect("checked");
            match walk.option(name) {
                Some(param) if !walk.used.contains(&param.name) => {
                    typed_option = Some(param.name);
                    offers = param
                        .kind
                        .candidates(prefix, context)
                        .into_iter()
                        .map(|(value, label)| {
                            (format!("{name}={label}"), format!("{name}={value}"))
                        })
                        .collect();
                    if offers.is_empty() && param.kind.lists_values() && !prefix.is_empty() {
                        let why = param
                            .kind
                            .check(prefix, context)
                            .err()
                            .unwrap_or_else(|| format!("is not {}", param.kind.long()));
                        walk.fail(format!("{name}={prefix} {why}"));
                    }
                    param.describe(true)
                }
                Some(_) => {
                    walk.fail(format!("{name}= is given twice"));
                    remaining(&walk, &mut offers)
                }
                None if schema.rest_text && walk.rest() => text_next(&walk, &mut offers),
                None => {
                    let problem = walk.unknown(name);
                    walk.fail(problem);
                    remaining(&walk, &mut offers)
                }
            }
        }
        Some(partial) => {
            let options: Vec<(String, String)> = walk
                .options_left()
                .into_iter()
                .filter(|param| param.name.starts_with(partial))
                .map(|param| (format!("{}=", param.name), format!("{}=", param.name)))
                .collect();
            let forms = all.is_empty().then(|| {
                schema
                    .forms
                    .iter()
                    .filter(|(word, _)| word.starts_with(partial))
                    .map(|(word, _)| ((*word).to_owned(), (*word).to_owned()))
            });
            match next_positional {
                Some(param) => {
                    offers.extend(
                        param
                            .kind
                            .candidates(partial, context)
                            .into_iter()
                            .map(|(value, label)| (label, value)),
                    );
                    offers.extend(forms.into_iter().flatten());
                    offers.extend(options);
                    param.describe(false)
                }
                None if walk.free_options => {
                    format!("parameters of the saved preset {}", all[0])
                }
                None => {
                    offers.extend(options);
                    if offers.is_empty() {
                        let problem = format!(
                            "{partial} is extra; :{verb} {}",
                            if walk.options().is_empty() {
                                "takes nothing more".to_owned()
                            } else {
                                format!(
                                    "takes {}",
                                    walk.options_left()
                                        .iter()
                                        .map(|param| format!("{}=", param.name))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            }
                        );
                        if !(schema.rest_text && walk.rest()) {
                            walk.fail(problem);
                        }
                    }
                    "parameter name".into()
                }
            }
        }
        None => match next_positional {
            Some(param) if !(schema.rest_text && walk.rest() && walk.index > 0) => {
                offers.extend(
                    param
                        .kind
                        .candidates("", context)
                        .into_iter()
                        .map(|(value, label)| (label, value)),
                );
                if all.is_empty() {
                    offers.extend(
                        schema
                            .forms
                            .iter()
                            .map(|(word, _)| ((*word).to_owned(), (*word).to_owned())),
                    );
                }
                offers.extend(walk.options_left().iter().map(|param| chip(param)));
                param.describe(false)
            }
            _ if walk.free_options => format!("parameters of the saved preset {}", all[0]),
            _ => remaining(&walk, &mut offers),
        },
    };
    if walk.problem.is_none() {
        let mut used = walk.used.clone();
        used.extend(typed_option);
        if let Some(broken) = schema
            .constraints
            .iter()
            .find_map(|constraint| constraint.broken(&used))
        {
            walk.fail(broken);
        }
    }
    let (candidates, inserts) = offers.into_iter().unzip();
    Some(Guide {
        next,
        candidates,
        inserts,
        problem: walk.problem,
        target,
        usage: schema.usage(verb),
    })
}

fn chip(param: &Param) -> (String, String) {
    (param.chip(), format!("{}=", param.name))
}

fn text_next(walk: &Walk<'_>, offers: &mut Vec<(String, String)>) -> String {
    offers.extend(walk.options_left().iter().map(|param| chip(param)));
    walk.positional()
        .last()
        .map_or_else(|| "text".into(), |param| param.describe(false))
}

/// The parameters left, required ones first.
fn remaining(walk: &Walk<'_>, offers: &mut Vec<(String, String)>) -> String {
    let left = walk.options_left();
    let required: Vec<_> = left
        .iter()
        .filter(|param| param.default == Default::Required)
        .collect();
    offers.extend(left.iter().map(|param| chip(param)));
    if let Some(param) = required.first() {
        return param.describe(true);
    }
    if left.is_empty() {
        "nothing more; Enter applies".into()
    } else if walk.schema.rest_text {
        "more text, or optional parameters".into()
    } else {
        "optional parameters, each with what omitting it means".into()
    }
}

/// The generated reference's argument section.
#[cfg(test)]
pub fn reference_markdown() -> String {
    use std::fmt::Write as _;
    let escape = |text: &str| text.replace('|', "\\|");
    let mut out = String::from("## Command arguments\n\n");
    out.push_str(
        "Generated from the declarative argument schemas in\n\
         [`command/schema/table.rs`](../crates/deadpan-app/src/navigation/command/schema/table.rs).\n\
         The command line completes and checks arguments from the same schema while\n\
         you type, and names what the command acts on in the current context.\n\n",
    );
    let mut verbs: Vec<_> = SCHEMAS.iter().collect();
    verbs.sort_by_key(|(verb, _)| *verb);
    let (plain, with): (Vec<_>, Vec<_>) = verbs
        .into_iter()
        .partition(|(_, schema)| !schema.takes_arguments());
    for (verb, schema) in with {
        let _ = writeln!(out, "### `:{verb}`\n");
        let _ = writeln!(out, "`{}`\n", schema.usage(verb));
        let _ = writeln!(out, "Acts on {}.\n", schema.selector.generic());
        let mut rows = Vec::new();
        for param in schema.positional {
            rows.push((param.name.to_ascii_uppercase(), param, ""));
        }
        for param in schema.options {
            rows.push((format!("{}=", param.name), param, ""));
        }
        for variant in schema.variants {
            for param in variant.positional {
                rows.push((param.name.to_ascii_uppercase(), param, variant.value));
            }
            for param in variant.options {
                rows.push((format!("{}=", param.name), param, variant.value));
            }
        }
        out.push_str("| Parameter | Type and units | Default | Meaning |\n");
        out.push_str("| --- | --- | --- | --- |\n");
        for (name, param, variant) in rows {
            let name = if variant.is_empty() {
                format!("`{name}`")
            } else {
                format!("`{name}` ({variant})")
            };
            let _ = writeln!(
                out,
                "| {name} | {} | {} | {} |",
                escape(&param.kind.long()),
                escape(&param.default.long()),
                escape(param.help)
            );
        }
        for (form, meaning) in schema.forms {
            let _ = writeln!(out, "| `{form}` | whole form | - | {} |", escape(meaning));
        }
        out.push('\n');
        for constraint in schema.constraints {
            let _ = writeln!(out, "- {}.", constraint.describe());
        }
        if !schema.constraints.is_empty() {
            out.push('\n');
        }
    }
    out.push_str("### Commands without arguments\n\n");
    for (verb, schema) in plain {
        let _ = writeln!(out, "- `:{verb}` acts on {}.", schema.selector.generic());
    }
    out
}
