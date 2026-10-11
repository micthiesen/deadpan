//! Explicit corrections of a registered video's decoding interpretation.

use deadpan_core::{
    SourceChromaLocation as Chroma, SourceColorRange as Range, SourceInterpretation,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Show,
    Reset,
    Set {
        range: Option<Option<Range>>,
        chroma_location: Option<Option<Chroma>>,
    },
}

pub fn parse<'a>(mut words: impl Iterator<Item = &'a str>) -> Result<Input, String> {
    const USAGE: &str = "Use :source-interpret range=auto|limited|full chroma=auto|left|center|top-left|top|bottom-left|bottom, or :source-interpret reset.";
    let Some(first) = words.next() else {
        return Ok(Input::Show);
    };
    if first == "reset" {
        return if words.next().is_none() {
            Ok(Input::Reset)
        } else {
            Err(USAGE.into())
        };
    }
    let mut range = None;
    let mut chroma_location = None;
    for word in std::iter::once(first).chain(words) {
        match word.split_once('=') {
            Some(("range", value)) if range.is_none() => {
                range = Some(match value {
                    "auto" => None,
                    "limited" => Some(Range::Limited),
                    "full" => Some(Range::Full),
                    _ => return Err(USAGE.into()),
                })
            }
            Some(("chroma", value)) if chroma_location.is_none() => {
                chroma_location = Some(match value {
                    "auto" => None,
                    "left" => Some(Chroma::Left),
                    "center" => Some(Chroma::Center),
                    "top-left" => Some(Chroma::TopLeft),
                    "top" => Some(Chroma::Top),
                    "bottom-left" => Some(Chroma::BottomLeft),
                    "bottom" => Some(Chroma::Bottom),
                    _ => return Err(USAGE.into()),
                })
            }
            _ => return Err(USAGE.into()),
        }
    }
    Ok(Input::Set {
        range,
        chroma_location,
    })
}

impl Input {
    pub fn apply(&self, current: Option<SourceInterpretation>) -> Option<SourceInterpretation> {
        match self {
            Self::Show => current,
            Self::Reset => None,
            Self::Set {
                range,
                chroma_location,
            } => {
                let choice = SourceInterpretation {
                    range: range.unwrap_or_else(|| current.and_then(|v| v.range)),
                    chroma_location: chroma_location
                        .unwrap_or_else(|| current.and_then(|v| v.chroma_location)),
                };
                (choice.range.is_some() || choice.chroma_location.is_some()).then_some(choice)
            }
        }
    }
}

pub fn parameters(choice: Option<SourceInterpretation>) -> String {
    let range = match choice.and_then(|v| v.range) {
        None => "auto",
        Some(Range::Limited) => "limited",
        Some(Range::Full) => "full",
    };
    let chroma = match choice.and_then(|v| v.chroma_location) {
        None => "auto",
        Some(Chroma::Left) => "left",
        Some(Chroma::Center) => "center",
        Some(Chroma::TopLeft) => "top-left",
        Some(Chroma::Top) => "top",
        Some(Chroma::BottomLeft) => "bottom-left",
        Some(Chroma::Bottom) => "bottom",
    };
    format!("range={range} chroma={chroma}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_merge_explicit_fields_reset_and_reject_ambiguous_input() {
        let choice = Some(SourceInterpretation {
            range: Some(Range::Full),
            chroma_location: Some(Chroma::Center),
        });
        for input in [
            "",
            "range=full",
            "chroma=center",
            "chroma=center range=full",
        ] {
            assert_eq!(
                parse(input.split_whitespace()).unwrap().apply(choice),
                choice
            );
        }
        let chroma_only = parse("range=auto".split_whitespace())
            .unwrap()
            .apply(choice);
        assert_eq!(chroma_only.unwrap().chroma_location, Some(Chroma::Center));
        assert_eq!(chroma_only.unwrap().range, None);
        for input in ["reset", "range=auto chroma=auto"] {
            assert_eq!(parse(input.split_whitespace()).unwrap().apply(choice), None);
        }
        for input in [
            "reset range=full",
            "range=full range=auto",
            "chroma=left chroma=left",
            "range=guess",
            "chroma=unspecified",
            "full",
            "foo=full",
        ] {
            assert!(parse(input.split_whitespace()).is_err(), "{input}");
        }
        for chroma in [
            Chroma::Left,
            Chroma::Center,
            Chroma::TopLeft,
            Chroma::Top,
            Chroma::BottomLeft,
            Chroma::Bottom,
        ] {
            let choice = Some(SourceInterpretation {
                range: Some(Range::Limited),
                chroma_location: Some(chroma),
            });
            let text = parameters(choice);
            let super::super::Entry::SourceInterpretation(input) =
                super::super::parse(&format!(":source-interpret {text}")).unwrap()
            else {
                panic!("wrong command")
            };
            assert_eq!(input.apply(None), choice);
        }
    }
}
