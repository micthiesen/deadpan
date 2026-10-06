//! `:cutaway [register=r] [fit=hold|loop|bounce|gap] [audio=keep]` and
//! `:cutaway clear`: picture-only cutaways over part of the selected beat.

use deadpan_core::CutawayFit;

pub const USAGE: &str = "Use :cutaway to show the copied Original moment over the selected range of this beat while its sound continues; register=r chooses a register and fit=hold|loop|bounce|gap what follows a short moment (bounce plays it forward and back, a micro-loop with no jump at its seam). :cutaway clear removes cutaways there.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutawayInput {
    Place {
        register: Option<char>,
        fit: CutawayFit,
    },
    Clear,
}

pub fn parse(arguments: &[&str]) -> Result<CutawayInput, String> {
    if arguments == ["clear"] {
        return Ok(CutawayInput::Clear);
    }
    let mut register = None;
    let mut fit = None;
    for argument in arguments {
        let (key, value) = argument.split_once('=').ok_or(USAGE)?;
        match key {
            "register" if register.is_none() => {
                let mut chars = value.chars();
                register = Some(
                    chars
                        .next()
                        .filter(|name| {
                            (name.is_ascii_alphabetic() || *name == '"') && chars.next().is_none()
                        })
                        .ok_or("register= takes one letter or \".")?,
                );
            }
            "fit" if fit.is_none() => {
                fit = Some(match value {
                    "hold" => CutawayFit::Hold,
                    "loop" => CutawayFit::Loop,
                    "gap" => CutawayFit::Gap,
                    "bounce" => CutawayFit::Bounce,
                    _ => return Err("fit is hold, loop, bounce or gap.".into()),
                });
            }
            "audio" => {
                if value != "keep" {
                    return Err(
                        "A cutaway replaces picture only; its beat's sound continues (audio=keep)."
                            .into(),
                    );
                }
            }
            "register" | "fit" => return Err(format!("{key} is given twice.")),
            _ => return Err(format!("Unknown :cutaway parameter {key}. {USAGE}")),
        }
    }
    Ok(CutawayInput::Place {
        register,
        fit: fit.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worked_example_and_choices_parse() {
        assert_eq!(
            parse(&["register=r", "audio=keep"]).unwrap(),
            CutawayInput::Place {
                register: Some('r'),
                fit: CutawayFit::Hold
            }
        );
        assert_eq!(
            parse(&["fit=loop"]).unwrap(),
            CutawayInput::Place {
                register: None,
                fit: CutawayFit::Loop
            }
        );
        assert_eq!(
            parse(&["fit=bounce"]).unwrap(),
            CutawayInput::Place {
                register: None,
                fit: CutawayFit::Bounce
            }
        );
        assert_eq!(parse(&["clear"]).unwrap(), CutawayInput::Clear);
        for bad in [
            vec!["audio=cutaway"],
            vec!["register=rr"],
            vec!["fit=stretch"],
            vec!["size=2"],
            vec!["fit=hold", "fit=loop"],
            vec!["clear", "fit=hold"],
        ] {
            assert!(parse(&bad).is_err(), "{bad:?}");
        }
    }
}
