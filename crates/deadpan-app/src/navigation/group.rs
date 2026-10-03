//! Named Group entry uses the specification's JSON string grammar.

use super::command::Entry;

const USAGE: &str = "Use :group name=\"the uncomfortable answer\" with one JSON-quoted name.";
// A valid 1024-byte label can encode every byte as a six-byte JSON escape.
// Bound raw input before JSON allocation, then enforce the core label policy.
const MAX_ENCODED_LABEL_BYTES: usize = 1024 * 6 + 2;

pub(super) fn parse(arguments: &str) -> Result<Entry, String> {
    let encoded = arguments.trim().strip_prefix("name=").ok_or(USAGE)?;
    if encoded.len() > MAX_ENCODED_LABEL_BYTES {
        return Err("The group name exceeds the supported encoded length.".into());
    }
    if !encoded.starts_with('"') {
        return Err(USAGE.into());
    }
    let label: String =
        serde_json::from_str(encoded).map_err(|error| format!("{USAGE} {error}"))?;
    deadpan_core::validate_group_label(&label).map_err(|error| error.to_string())?;
    Ok(Entry::Group { label })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::{Action, command};

    #[test]
    fn group_names_preserve_unicode_whitespace_and_json_escapes() {
        for label in [
            "the uncomfortable answer",
            "  café 🎬  ",
            "",
            "a\n\t\r\u{8}\u{c} slash/ quote\" backslash\\",
            &"é".repeat(512),
        ] {
            let encoded = serde_json::to_string(label).unwrap();
            assert_eq!(
                command::parse(&format!(":GROUP name={encoded}")),
                Ok(Entry::Group {
                    label: label.into()
                })
            );
        }
        assert_eq!(
            command::parse(r#"group name="\uD83C\uDFAC\/\u00e9""#),
            Ok(Entry::Group {
                label: "🎬/é".into()
            })
        );
        let encoded = format!("group name=\"{}\"", "\\u0061".repeat(1024));
        assert!(command::parse(&encoded).is_ok());
    }

    #[test]
    fn malformed_duplicate_extra_and_oversized_names_refuse() {
        for input in [
            "group",
            "group answer",
            "group name=answer",
            "group name=",
            "group other=\"x\"",
            "group name=\"x\" name=\"y\"",
            "group name=\"x\" extra",
            "group name=\"unterminated",
            r#"group name="\x20""#,
            r#"group name="\uD800""#,
            r#"group name="\u0000""#,
            "group name=\"raw\nnewline\"",
        ] {
            assert!(command::parse(input).is_err(), "{input}");
        }
        for label in ["a".repeat(1025), "é".repeat(513)] {
            assert!(
                command::parse(&format!(
                    "group name={}",
                    serde_json::to_string(&label).unwrap()
                ))
                .is_err()
            );
        }
        assert!(command::parse(&format!("group name=\"{}\"", "\\u0061".repeat(1025))).is_err());
        assert_eq!(
            command::parse(":ungroup"),
            Ok(Entry::Action(Action::Ungroup))
        );
        assert!(command::parse("ungroup extra").is_err());
    }
}
