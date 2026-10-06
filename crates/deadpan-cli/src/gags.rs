//! Gag inspection shared with the native `:gag-inspect`: the exact ordinary
//! steps a recipe expands to, without applying anything.

use std::path::Path;

use serde_json::json;

/// The ordinary steps a recipe expands to, one readable line each, with
/// every resolved parameter: what `:gag-inspect` shows before applying.
pub fn expansion_rows(
    recipe: &deadpan_core::GagRecipe,
    visual: bool,
    rate: deadpan_core::FrameRate,
) -> Result<Vec<String>, String> {
    use deadpan_core::{PauseLength, SemanticInstruction as I, SemanticSelector};
    let length = |length: &PauseLength| -> String {
        let text = match length {
            PauseLength::Frames { frames } => format!("{frames}f"),
            PauseLength::Milliseconds { milliseconds } => format!("{milliseconds}ms"),
        };
        match length.resolve(rate) {
            Ok(frames) if !matches!(length, PauseLength::Frames { .. }) => {
                format!("{text} ({} f)", frames.frames())
            }
            _ => text,
        }
    };
    let target = |selector: &SemanticSelector| match selector {
        SemanticSelector::VisualSelection => "the Visual range".to_owned(),
        SemanticSelector::SelectedBeat => "the selected beat".to_owned(),
        SemanticSelector::Motion { .. } => "the frames just added".to_owned(),
        _ => "the selection".to_owned(),
    };
    let instructions = recipe.expand(visual, rate).map_err(|error| error.message)?;
    Ok(instructions
        .iter()
        .map(|instruction| match instruction {
            I::InsertPause {
                length: pause,
                black,
            } => format!(
                "Insert a {} silent {} pause at the cursor",
                length(pause),
                if *black { "black" } else { "freeze" }
            ),
            I::SetFraming { framing } => match framing.as_deref().map(|framing| &framing.value) {
                Some(deadpan_core::FramingValue::Envelope { envelope }) => {
                    let end = envelope
                        .segments
                        .last()
                        .map_or(envelope.initial.scale, |segment| segment.pose.scale);
                    format!(
                        "Creep in on it to {:.3}× ({} segment)",
                        end.numerator() as f64 / end.denominator() as f64,
                        envelope.segments.len()
                    )
                }
                Some(_) => "Frame it with a fixed pose".into(),
                None => "Remove its framing".into(),
            },
            I::Repeat {
                selector,
                plays,
                escalation,
            } => format!(
                "Repeat {} for {plays} plays{}",
                target(selector),
                escalation.map_or(String::new(), |escalation| format!(
                    ", each play {:+} dB{}",
                    f64::from(escalation.gain_step.millidecibels()) / 1000.0,
                    escalation.zoom.map_or(String::new(), |zoom| format!(
                        " and {:+.3} scale",
                        zoom.step.numerator() as f64 / zoom.step.denominator() as f64
                    ))
                ))
            ),
            I::SetRepeat {
                gaps: Some(gaps), ..
            } => format!(
                "Silent freeze gaps after each play but the last: {}",
                gaps.iter().map(length).collect::<Vec<_>>().join(", ")
            ),
            I::Paste { register, .. } => {
                format!("Paste register {} at the cursor", register.as_char())
            }
            I::SetRoomTone { register } => format!(
                "Loop room tone from the Original moment in register {}",
                register.as_char()
            ),
            I::MoveFrames { forward, count } => format!(
                "Move the cursor {count} frames {}",
                if *forward { "forward" } else { "back" }
            ),
            I::Tail { effect, .. } => {
                format!(
                    "Ring a {} tail of the sound before it through the pause",
                    effect.name()
                )
            }
            I::SetCutaway { register, .. } => format!(
                "Show the reaction in register {} over the pause; its sound continues",
                register.as_char()
            ),
            I::Group { selector, label } => format!("Group {} as “{label}”", target(selector)),
            other => format!("{other:?}"),
        })
        .collect())
}

/// `gag-inspect <project> --json <recipe.json> [--visual]`: the steps
/// `:gag-inspect` lists for this recipe at the project's frame rate. With
/// `--visual` the content is a Visual range, as when one is selected. Writes
/// nothing; the recipe is the `gag` instruction's, given explicitly because
/// saved presets live in the app's personal library.
pub fn run(arguments: &[&str]) -> Result<(), crate::CliError> {
    let (package, path, visual) = match arguments {
        [package, "--json", path] => (*package, *path, false),
        [package, "--json", path, "--visual"] => (*package, *path, true),
        _ => {
            return Err(crate::CliError::Usage(
                "usage: gag-inspect <project.deadpan> --json <recipe.json> [--visual]".into(),
            ));
        }
    };
    let recipe: deadpan_core::GagRecipe =
        serde_json::from_str(&crate::read_request(Path::new(path))?)?;
    let store =
        deadpan_store::ProjectStore::open(Path::new(package), deadpan_store::AccessMode::ReadOnly)?;
    let rate = store.snapshot()?.presentation_basis().frame_rate;
    let steps = expansion_rows(&recipe, visual, rate)
        .map_err(|message| crate::live_project::LiveError::new("InvalidCommand", message))?;
    let instructions = recipe
        .expand(visual, rate)
        .map_err(|error| crate::live_project::LiveError::new(error.code.as_str(), error.message))?;
    crate::write_json(&json!({
        "protocol": 1,
        "name": recipe.name(),
        "visual": visual,
        "steps": steps,
        "instructions": instructions,
    }))
}
