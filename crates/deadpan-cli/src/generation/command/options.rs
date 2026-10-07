use deadpan_core::{NodeId, ScopedNodeTarget};
use deadpan_jobs::{GenerationOptions, GenerationTarget, HoldInstructions};

use crate::CliError;

pub(super) struct Arguments<'a> {
    pub path: &'a str,
    pub hold: NodeId,
    pub scope: ScopedNodeTarget,
    pub seed: Option<u64>,
    pub variants: u32,
    pub another: bool,
    pub options: Option<GenerationOptions>,
}

pub(super) fn parse<'a>(arguments: &[&'a str]) -> Result<Arguments<'a>, CliError> {
    let usage = || {
        CliError::Usage("usage: generate-hold <project.deadpan> --hold <node-id> [--scope JSON] [--seed N] [--variants 1-4] [--motion still|subtle|moderate] [--target ID|none] [--instructions TEXT] [--another]".into())
    };
    let [path, rest @ ..] = arguments else {
        return Err(usage());
    };
    let mut hold = None;
    let mut scope = None;
    let mut seed = None;
    let mut variants = 1;
    let mut another = false;
    let mut controls = GenerationOptions::default();
    let mut controls_given = false;
    let mut seen = std::collections::BTreeSet::new();
    let mut options = rest.iter();
    while let Some(option) = options.next() {
        if !seen.insert(*option) {
            return Err(usage());
        }
        if *option == "--another" {
            another = true;
            continue;
        }
        let value = options.next().ok_or_else(usage)?;
        match *option {
            "--hold" => hold = Some(NodeId::new(*value)?),
            "--scope" => {
                if value.len() > 128 * 1024 {
                    return Err(CliError::Usage("--scope exceeds its size limit".into()));
                }
                scope = Some(
                    serde_json::from_str::<ScopedNodeTarget>(value).map_err(|error| {
                        CliError::Usage(format!("--scope must be a scoped target: {error}"))
                    })?,
                );
            }
            "--seed" => {
                seed = Some(
                    value
                        .parse()
                        .ok()
                        .filter(|seed| *seed < 1 << 32)
                        .ok_or_else(|| CliError::Usage("--seed must be below 2^32".into()))?,
                )
            }
            "--variants" => {
                variants = value
                    .parse()
                    .ok()
                    .filter(|count| (1..=super::MAX_VARIANTS).contains(count))
                    .ok_or_else(|| {
                        CliError::Usage(format!("--variants must be 1 to {}", super::MAX_VARIANTS))
                    })?
            }
            "--motion" => {
                controls.motion = value
                    .parse()
                    .map_err(|error: &str| CliError::Usage(error.into()))?;
                controls_given = true;
            }
            "--target" => {
                controls.region_target = GenerationTarget::parse(value).map_err(CliError::Usage)?;
                controls_given = true;
            }
            "--instructions" => {
                controls.instructions = Some(
                    HoldInstructions::new(*value)
                        .map_err(|error| CliError::Usage(error.to_string()))?,
                );
                controls_given = true;
            }
            _ => return Err(usage()),
        }
    }
    if another && (seed.is_some() || controls_given) {
        return Err(CliError::Usage("--another retains the current request's seed, motion, target and instructions; omit it to change those controls.".into()));
    }
    let hold = hold.ok_or_else(usage)?;
    let scope = scope.unwrap_or_else(|| ScopedNodeTarget {
        node: hold.clone(),
        repeats: Vec::new(),
    });
    if scope.node != hold {
        return Err(CliError::Usage(
            "--scope must name the same Hold as --hold".into(),
        ));
    }
    Ok(Arguments {
        path,
        hold,
        scope,
        seed,
        variants,
        another,
        options: controls_given.then_some(controls),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_jobs::MotionAmount;

    #[test]
    fn scope_is_strict_bounded_and_names_the_requested_hold() {
        let scope = r#"{"node":"pause","repeats":[{"repeat":"repeat","branch":{"type":"play","iteration":{"allocation":"r","ordinal":1}}}]}"#;
        let result = parse(&["p.deadpan", "--hold", "pause", "--scope", scope]).unwrap();
        assert_eq!(result.scope.node, result.hold);
        assert_eq!(result.scope.repeats.len(), 1);
        assert!(parse(&["p.deadpan", "--hold", "other", "--scope", scope]).is_err());
        assert!(
            parse(&[
                "p.deadpan",
                "--hold",
                "pause",
                "--scope",
                scope,
                "--scope",
                scope
            ])
            .is_err()
        );
        for invalid in [
            r#"{"node":"pause","repeats":[],"ignored":true}"#.to_owned(),
            " ".repeat(128 * 1024 + 1),
            "null".to_owned(),
        ] {
            assert!(parse(&["p.deadpan", "--hold", "pause", "--scope", &invalid]).is_err());
        }
        assert!(
            parse(&["p.deadpan", "--hold", "pause"])
                .unwrap()
                .scope
                .repeats
                .is_empty()
        );
    }

    #[test]
    fn explicit_controls_are_bounded_and_cannot_change_another_request() {
        fn read<'a>(tail: &[&'a str]) -> Result<Arguments<'a>, CliError> {
            parse(&[["p.deadpan", "--hold", "pause"].as_slice(), tail].concat())
        }
        let result = read(&[
            "--motion",
            "subtle",
            "--instructions",
            "Keep the eyes open.",
            "--variants",
            "3",
        ])
        .unwrap();
        assert_eq!(result.variants, 3);
        let options = result.options.unwrap();
        assert_eq!(options.motion, MotionAmount::Subtle);
        assert_eq!(
            options.instructions.unwrap().as_str(),
            "Keep the eyes open."
        );
        for tail in [
            vec!["--motion", "fast"],
            vec!["--instructions", ""],
            vec!["--instructions", "a\nb"],
            vec!["--another", "--motion", "still"],
            vec!["--another", "--instructions", "still"],
            vec!["--another", "--seed", "1"],
            vec!["--another", "--target", "none"],
            vec!["--target", ""],
            vec!["--target", "one", "--target", "two"],
            vec!["--motion", "still", "--motion", "subtle"],
        ] {
            assert!(read(&tail).is_err(), "{tail:?}");
        }
        assert!(read(&["--instructions", &"界".repeat(171)]).is_err());
        assert!(read(&["--another"]).unwrap().options.is_none());
        assert!(read(&[]).unwrap().options.is_none());
        assert_eq!(
            read(&["--motion", "still"])
                .unwrap()
                .options
                .unwrap()
                .region_target,
            GenerationTarget::Inherit
        );
        assert_eq!(
            read(&["--target", "none"])
                .unwrap()
                .options
                .unwrap()
                .region_target,
            GenerationTarget::None
        );
        assert_eq!(
            read(&["--target", "subject"])
                .unwrap()
                .options
                .unwrap()
                .region_target,
            GenerationTarget::Saved(deadpan_core::TargetId::new("subject").unwrap())
        );
    }
}
