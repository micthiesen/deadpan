//! Strict, bounded configuration parsing. Installation is atomic: callers only
//! receive a complete immutable map after both mode tries have compiled.

use std::sync::Arc;

use serde::Deserialize;

use super::editor_map::{BindingId, Compiled, KeyMode, Stroke};
use super::{DIGITS, Key};

pub(super) mod reservations;

const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_ALIASES: usize = 8;
const MAX_TOKEN_BYTES: usize = 32;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    version: u32,
    key_mode: KeyMode,
    bindings: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    action: String,
    keys: Vec<Vec<String>>,
}

pub(super) fn parse(bytes: &[u8]) -> Result<Arc<Compiled>, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Keymap exceeds the 256 KiB file limit".into());
    }
    // serde_json retains its default recursion limit. The file bound also
    // bounds allocations before the schema's individual limits are checked.
    let config: Config = serde_json::from_slice(bytes).map_err(|error| {
        format!(
            "Invalid keymap JSON: {}",
            error.to_string().chars().take(192).collect::<String>()
        )
    })?;
    if config.version != 1 {
        return Err("Unsupported keymap version; expected 1".into());
    }
    if config.bindings.len() > BindingId::ALL.len() {
        return Err("Too many action entries".into());
    }
    let mut overrides = Vec::new();
    for entry in config.bindings {
        if entry.action.len() > MAX_TOKEN_BYTES {
            return Err("Action ID exceeds 32 bytes".into());
        }
        let id = BindingId::ALL
            .into_iter()
            .find(|id| id.as_str() == entry.action)
            .ok_or_else(|| format!("Unknown action ID: {}", entry.action))?;
        if overrides.iter().any(|(prior, _)| *prior == id) {
            return Err(format!("Duplicate action ID: {}", id.as_str()));
        }
        if entry.keys.is_empty() || entry.keys.len() > MAX_ALIASES {
            return Err(format!("{} requires 1 to 8 aliases", id.as_str()));
        }
        let mut paths = Vec::new();
        for path in entry.keys {
            if path.is_empty() || path.len() > super::binding_trie::MAX_PATH_LEN {
                return Err(format!("{} requires paths of 1 to 16 keys", id.as_str()));
            }
            let mut strokes = Vec::new();
            for token in path {
                strokes.push(parse_stroke(&token, config.key_mode)?);
            }
            if strokes
                .iter()
                .any(|stroke| DIGITS.iter().any(|(key, _)| *key == stroke.0))
            {
                return Err("Digits are reserved for the count grammar at every depth".into());
            }
            if strokes
                .iter()
                .any(|stroke| matches!(stroke.0, Key::Escape | Key::Tab))
                && !id.fixed()
            {
                return Err("Escape and Tab are fixed native keys at every depth".into());
            }
            if paths.contains(&strokes) {
                return Err(format!("Duplicate alias for {}", id.as_str()));
            }
            paths.push(strokes);
        }
        overrides.push((id, paths));
    }
    Compiled::compile(config.key_mode, overrides).map(Arc::new)
}

fn parse_stroke(token: &str, mode: KeyMode) -> Result<Stroke, String> {
    if token.is_empty() || token.len() > MAX_TOKEN_BYTES {
        return Err("Key tokens require 1 to 32 bytes".into());
    }
    let (name, mut shift) = token
        .strip_prefix("Shift+")
        .map_or((token, false), |name| (name, true));
    if name.contains('+') && name != "+" {
        return Err(
            "Only Shift modifiers are configurable; native and Kestrel chords are reserved".into(),
        );
    }
    if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() {
        shift = true;
    }
    let key = Key::from_name(name).ok_or_else(|| format!("Unknown key token: {token}"))?;
    if modifier_key(key) {
        return Err("Modifier keys cannot be command path steps".into());
    }
    if mode == KeyMode::Physical
        && matches!(
            key,
            Key::Colon
                | Key::Questionmark
                | Key::Pipe
                | Key::Exclamationmark
                | Key::OpenCurlyBracket
                | Key::CloseCurlyBracket
                | Key::BrowserBack
        )
    {
        return Err("This key has no egui physical position; use an actual position such as Shift+Semicolon".into());
    }
    if mode == KeyMode::Logical && shift && logical_symbol(key) {
        return Err(
            "Logical punctuation uses the delivered symbol without an explicit Shift modifier"
                .into(),
        );
    }
    Ok(Stroke(key, shift))
}

/// These egui identities are symbols, independent of the Shift/Option keys a
/// layout used to produce them. Minus stays modifier-sensitive: egui can report
/// a shifted physical Minus when the logical underscore has no named key.
pub(super) fn logical_symbol(key: Key) -> bool {
    matches!(
        key,
        Key::Comma
            | Key::Quote
            | Key::Colon
            | Key::Slash
            | Key::Questionmark
            | Key::Plus
            | Key::Pipe
            | Key::Exclamationmark
            | Key::OpenCurlyBracket
            | Key::CloseCurlyBracket
    )
}

pub(super) fn modifier_key(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
    )
}

#[cfg(test)]
mod tests;
