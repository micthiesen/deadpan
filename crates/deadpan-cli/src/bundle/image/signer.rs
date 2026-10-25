//! Bind native images to the checked host's signing identity, including the
//! owner's self-signed certificate. Never interpret arbitrary requirement text.

use std::io;
use std::path::Path;

use super::invalid;

fn leaf_requirement(requirements: &str) -> io::Result<String> {
    let mut definitions = requirements
        .lines()
        .filter_map(|line| line.trim().strip_prefix("designated => "));
    let definition = definitions
        .next()
        .ok_or_else(|| invalid("signed host has no supported designated requirement"))?;
    if definitions.next().is_some() {
        return Err(invalid("ambiguous host designated requirement"));
    }
    let after_identifier = definition
        .strip_prefix("identifier \"")
        .and_then(|text| text.split_once('"'))
        .filter(|(identifier, _)| {
            !identifier.is_empty()
                && identifier.len() <= 255
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
        })
        .map(|(_, rest)| rest)
        .ok_or_else(|| invalid("unsupported signed host designated requirement"))?;
    // The anchor form is admitted only when the caller also verifies the host
    // against this leaf requirement, proving a self-signed leaf equals it.
    let hash = after_identifier
        .strip_prefix(" and certificate leaf = H\"")
        .or_else(|| after_identifier.strip_prefix(" and anchor H\""))
        .and_then(|text| text.strip_suffix('"'))
        .filter(|hash| hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| invalid("unsupported signed host certificate requirement"))?;
    Ok(format!(
        "certificate leaf = H\"{}\"",
        hash.to_ascii_uppercase()
    ))
}

fn requirement_from(details: &str, requirements: &str) -> io::Result<Option<String>> {
    let values = |prefix: &str| -> Vec<&str> {
        details
            .lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .collect()
    };
    let signature = values("Signature=");
    if signature == ["adhoc"] {
        if !values("Authority=").is_empty() {
            return Err(invalid("ambiguous ad hoc host signature"));
        }
        return Ok(None);
    }
    let teams = values("TeamIdentifier=");
    if teams.len() == 1 {
        let team = teams[0];
        if !team.is_empty() && team.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Ok(Some(format!(
                "anchor apple generic and certificate leaf[subject.OU] = \"{team}\""
            )));
        }
    }
    if !signature.is_empty() || teams.len() > 1 {
        return Err(invalid("unsupported host signature identity"));
    }
    leaf_requirement(requirements).map(Some)
}

#[cfg(any(target_os = "macos", test))]
mod command;

#[cfg(target_os = "macos")]
pub(super) fn application_requirement(
    current: &Path,
    control: super::Control<'_>,
) -> io::Result<Option<String>> {
    let mut command = std::process::Command::new("/usr/bin/codesign");
    command.args(["-d", "--verbose=2", "-r-"]).arg(current);
    let text = command::capture(&mut command, control)?;
    requirement_from(&text, &text)
}

#[cfg(target_os = "macos")]
pub(super) fn verify(
    path: &Path,
    requirement: Option<&str>,
    control: super::Control<'_>,
) -> io::Result<()> {
    let mut command = std::process::Command::new("/usr/bin/codesign");
    command.args(["--verify", "--strict"]);
    if let Some(requirement) = requirement {
        command.arg(format!("-R={requirement}"));
    }
    command.arg(path);
    command::capture(&mut command, control).map(|_| ())
}

#[cfg(not(target_os = "macos"))]
pub(super) fn application_requirement(
    _: &Path,
    _: super::Control<'_>,
) -> io::Result<Option<String>> {
    Err(invalid("native image signing requires macOS"))
}

#[cfg(not(target_os = "macos"))]
pub(super) fn verify(_: &Path, _: Option<&str>, _: super::Control<'_>) -> io::Result<()> {
    Err(invalid("native image signing requires macOS"))
}

pub(super) fn checked_application_requirement(
    current: &Path,
    control: super::Control<'_>,
) -> io::Result<Option<String>> {
    // Never derive signer authority from unchecked signature metadata. The
    // second verification also proves an admitted anchor hash is the leaf.
    verify(current, None, control)?;
    let expected = application_requirement(current, control)?;
    verify(current, expected.as_deref(), control)?;
    Ok(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn only_explicit_adhoc_or_a_bound_supported_signer_is_accepted() {
        assert_eq!(
            requirement_from("Signature=adhoc\nTeamIdentifier=not set", "").unwrap(),
            None
        );
        assert!(requirement_from("", "# designated => cdhash H\"abc\"").is_err());
        assert!(requirement_from("Signature=adhoc\nAuthority=somebody", "").is_err());
        assert_eq!(
            requirement_from("Authority=Developer ID\nTeamIdentifier=DEADPAN123", "").unwrap(),
            Some("anchor apple generic and certificate leaf[subject.OU] = \"DEADPAN123\"".into())
        );
        for clause in ["certificate leaf =", "anchor"] {
            let source =
                format!("designated => identifier \"dev.deadpan.test\" and {clause} H\"{HASH}\"");
            assert_eq!(
                requirement_from("Authority=Deadpan Local\nTeamIdentifier=not set", &source)
                    .unwrap(),
                Some(format!(
                    "certificate leaf = H\"{}\"",
                    HASH.to_ascii_uppercase()
                ))
            );
        }
    }

    #[test]
    fn local_requirement_rejects_extra_expressions_malformed_hashes_and_ambiguous_definitions() {
        let good = format!(
            "designated => identifier \"dev.deadpan.test\" and certificate leaf = H\"{HASH}\""
        );
        for wrong in [
            format!("{good} or true"),
            format!("{good} and true"),
            good.replace(HASH, "abc"),
            good.replace(HASH, &"g".repeat(40)),
            good.replace("dev.deadpan.test", "a\\\" or true"),
            good.replace("certificate leaf", "certificate 1"),
            format!("{good}\n{good}"),
        ] {
            assert!(
                requirement_from("Authority=Deadpan Local", &wrong).is_err(),
                "{wrong}"
            );
        }
    }
}
