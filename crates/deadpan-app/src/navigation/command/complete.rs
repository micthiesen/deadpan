//! Tab completion in the command line. The candidates are the ones the
//! completion rows show: matching verbs while the verb is typed, then the
//! schema guide's values and parameter names. Tab inserts the highlighted
//! candidate in place of the word being typed; repeated Tab (or Down) moves
//! to the next and Shift+Tab (or Up) to the previous, replacing that same
//! word. Typing anything else ends the cycle.

use super::schema::{self, Context};

/// What Tab can put in place of the word being typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidates {
    /// Byte offset where the replaced word starts (the end for a new word).
    pub start: usize,
    /// Insertions, in the order the rows show them.
    pub items: Vec<String>,
}

/// The candidates for the current command text, if any.
pub fn candidates(input: &str, context: &Context) -> Option<Candidates> {
    let leading = input.len() - input.trim_start().len();
    let body = &input[leading..];
    let (colon, body) = match body.strip_prefix(':') {
        Some(rest) => (1, rest),
        None => (0, body),
    };
    let verb_start = leading + colon;
    if !body.contains(char::is_whitespace) {
        let typed = body.to_ascii_lowercase();
        if typed.is_empty() {
            return None;
        }
        let mut verbs: Vec<&str> = super::commands()
            .iter()
            .map(|(verb, _)| *verb)
            .filter(|verb| verb.starts_with(&typed))
            .collect();
        // An exact verb comes first, then shorter verbs before longer ones.
        verbs.sort_by_key(|verb| (*verb != typed, verb.len()));
        verbs.dedup();
        return (!verbs.is_empty()).then(|| Candidates {
            start: verb_start,
            items: verbs.into_iter().map(|verb| format!("{verb} ")).collect(),
        });
    }
    let guide = schema::guide(input, context)?;
    if guide.inserts.is_empty() {
        return None;
    }
    // The guide completes the last word unless the text ends with a space.
    let start = if input.ends_with(char::is_whitespace) {
        input.len()
    } else {
        last_word_start(input)
    };
    Some(Candidates {
        start,
        items: guide
            .inserts
            .into_iter()
            .map(|insert| {
                if insert.ends_with('=') {
                    insert
                } else {
                    format!("{insert} ")
                }
            })
            .collect(),
    })
}

/// The start of the last word, keeping `"quoted text"` together like the
/// guide does.
fn last_word_start(input: &str) -> usize {
    let mut start = 0;
    let mut quoted = false;
    let mut in_word = false;
    for (index, character) in input.char_indices() {
        if character == '"' {
            quoted = !quoted;
        }
        let space = character.is_whitespace() && !quoted;
        if !space && !in_word {
            start = index;
        }
        in_word = !space;
    }
    start
}

/// An active Tab cycle: the text before the replaced word, the candidates
/// and the one inserted now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cycle {
    prefix: String,
    items: Vec<String>,
    index: usize,
    /// The command text this cycle produced; any other text ends it.
    applied: String,
}

impl Cycle {
    /// The candidate inserted now, when `input` is still the text it produced.
    pub fn current(&self, input: &str) -> Option<usize> {
        (self.applied == input).then_some(self.index)
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }
}

/// One Tab press. Continues `cycle` while the text is still what it inserted;
/// otherwise starts a new cycle at the first (or, backwards, the last)
/// candidate. Returns the new command text, or `None` when nothing completes.
pub fn step(
    cycle: &mut Option<Cycle>,
    input: &str,
    context: &Context,
    reverse: bool,
) -> Option<String> {
    if let Some(active) = cycle.as_mut().filter(|active| active.applied == input) {
        let count = active.items.len();
        active.index = if reverse {
            (active.index + count - 1) % count
        } else {
            (active.index + 1) % count
        };
        active.applied = format!("{}{}", active.prefix, active.items[active.index]);
        return Some(active.applied.clone());
    }
    let Candidates { start, items } = candidates(input, context)?;
    let index = if reverse { items.len() - 1 } else { 0 };
    let prefix = input[..start].to_owned();
    let applied = format!("{prefix}{}", items[index]);
    *cycle = Some(Cycle {
        prefix,
        items,
        index,
        applied: applied.clone(),
    });
    Some(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context::default()
    }

    fn tab(cycle: &mut Option<Cycle>, input: &str) -> String {
        step(cycle, input, &context(), false).unwrap_or_else(|| input.to_owned())
    }

    #[test]
    fn tab_completes_a_verb_with_a_space() {
        let mut cycle = None;
        assert_eq!(tab(&mut cycle, ":rep"), ":repeat ");
        assert_eq!(tab(&mut None, "  ramp"), "  ramp ");
        assert_eq!(step(&mut None, ":zzz", &context(), false), None);
        assert_eq!(step(&mut None, ":", &context(), false), None);
    }

    #[test]
    fn tab_inserts_a_parameter_name_with_its_equals_sign() {
        assert_eq!(tab(&mut None, ":repeat 3 g"), ":repeat 3 gap=");
        // A new word after a space starts from the next value or parameter.
        let mut cycle = None;
        let first = tab(&mut cycle, ":repeat 3 ");
        assert_eq!(first, ":repeat 3 gap=");
    }

    #[test]
    fn repeated_tab_and_shift_tab_cycle_enum_values() {
        let mut cycle = None;
        let input = ":ramp 1 2 pitch=";
        let first = tab(&mut cycle, input);
        assert_eq!(first, ":ramp 1 2 pitch=preserve ");
        let second = tab(&mut cycle, &first);
        assert_eq!(second, ":ramp 1 2 pitch=tape ");
        let wrapped = tab(&mut cycle, &second);
        assert_eq!(wrapped, first);
        let back = step(&mut cycle, &wrapped, &context(), true).unwrap();
        assert_eq!(back, second);
        assert_eq!(cycle.as_ref().unwrap().current(&back), Some(1));
        // Shift+Tab first chooses the last candidate.
        assert_eq!(
            step(&mut None, input, &context(), true).unwrap(),
            ":ramp 1 2 pitch=tape "
        );
    }

    #[test]
    fn typing_ends_the_cycle() {
        let mut cycle = None;
        let first = tab(&mut cycle, ":ramp 1 2 pitch=");
        let edited = format!("{first}steps=");
        assert_eq!(cycle.as_ref().unwrap().current(&edited), None);
        // A fresh cycle starts from the edited text; steps= has no listed values.
        assert_eq!(step(&mut cycle, &edited, &context(), false), None);
    }

    #[test]
    fn known_names_insert_their_value_not_their_label() {
        let mut context = context();
        context.registers = vec![('q', "macro")];
        let mut cycle = None;
        assert_eq!(
            step(&mut cycle, ":macro ", &context, false).as_deref(),
            Some(":macro q ")
        );
    }

    #[test]
    fn quoted_words_stay_one_word() {
        assert_eq!(last_word_start(":group name=\"a b"), 7);
        assert_eq!(last_word_start(":hold 1s v"), 9);
    }
}
