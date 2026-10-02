//! One reviewed physical reservation registry shared by routing and auditing.
use eframe::egui::{Key, Modifiers};
use std::{collections::BTreeSet, sync::LazyLock};

pub(crate) const FIXTURE: &str = include_str!("../kestrel-reserved.tsv");
pub(crate) struct Reservation {
    pub key: Key,
    pub modifiers: Modifiers,
    #[cfg(any(test, feature = "ui-harness"))]
    pub chord: String,
}

static REGISTRY: LazyLock<Result<(String, Vec<Reservation>), String>> =
    LazyLock::new(|| parse_fixture(FIXTURE));

pub(crate) fn validate() -> Result<(), String> {
    REGISTRY.as_ref().map(|_| ()).map_err(Clone::clone)
}

pub(crate) fn reserved(key: Key, modifiers: Modifiers) -> bool {
    // An unqualified embedded registry never grants an editor chord.
    match REGISTRY.as_ref() {
        Err(_) => true,
        Ok((_, entries)) => entries.iter().any(|entry| {
            entry.key == key
                && entry.modifiers.ctrl == modifiers.ctrl
                && entry.modifiers.alt == modifiers.alt
                && entry.modifiers.shift == modifiers.shift
                && entry.modifiers.mac_cmd
                    == (modifiers.mac_cmd || modifiers.command && !modifiers.ctrl)
        }),
    }
}

pub(crate) fn parse_fixture(input: &str) -> Result<(String, Vec<Reservation>), String> {
    let mut digest = None;
    let mut reservations = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, line) in input.lines().enumerate() {
        if let Some(value) = line.strip_prefix("# source-sha256=") {
            if digest.is_some()
                || value.len() != 64
                || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err("Kestrel fixture has an invalid or repeated source digest".into());
            }
            digest = Some(value.to_owned());
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 5 || fields.iter().any(|field| field.is_empty()) {
            return Err(format!("Malformed Kestrel fixture line {}", index + 1));
        }
        let keycode: u16 = fields[0]
            .parse()
            .map_err(|_| format!("Invalid Kestrel keycode on line {}", index + 1))?;
        let mask: u8 = fields[1]
            .parse()
            .map_err(|_| format!("Invalid Kestrel modifier mask on line {}", index + 1))?;
        if mask > 15 || !seen.insert((keycode, mask, fields[2])) {
            return Err(format!(
                "Invalid or duplicate Kestrel binding on line {}",
                index + 1
            ));
        }
        // Kestrel's Ghostty-only Cmd+N is intentionally available to Deadpan.
        // If Kestrel adds an app scope for Deadpan, this audit must learn its
        // exact bundle ID rather than treating all scoped bindings as global.
        if fields[2] != "*" {
            if fields[2] != "com.mitchellh.ghostty" {
                return Err(format!("Unqualified Kestrel app scope {:?}", fields[2]));
            }
            continue;
        }
        let key = ansi_key(keycode)
            .ok_or_else(|| format!("Unqualified Kestrel physical keycode {keycode}"))?;
        let modifiers = Modifiers {
            ctrl: mask & 1 != 0,
            alt: mask & 2 != 0,
            shift: mask & 4 != 0,
            mac_cmd: mask & 8 != 0,
            command: mask & 8 != 0,
        };
        reservations.push(Reservation {
            key,
            modifiers,
            #[cfg(any(test, feature = "ui-harness"))]
            chord: format!("{} [{key:?}, modifiers={mask}, {}]", fields[3], fields[4]),
        });
    }
    if reservations.is_empty() {
        return Err("Kestrel fixture contains no global bindings".into());
    }
    Ok((
        digest.ok_or("Kestrel fixture has no source digest")?,
        reservations,
    ))
}

/// The qualified registry uses macOS physical ANSI positions. This maps those
/// positions to egui physical keys for production admission and the audit.
fn ansi_key(code: u16) -> Option<Key> {
    Some(match code {
        0 => Key::A,
        1 => Key::S,
        2 => Key::D,
        3 => Key::F,
        4 => Key::H,
        5 => Key::G,
        6 => Key::Z,
        7 => Key::X,
        8 => Key::C,
        9 => Key::V,
        11 => Key::B,
        12 => Key::Q,
        13 => Key::W,
        14 => Key::E,
        15 => Key::R,
        16 => Key::Y,
        17 => Key::T,
        18 => Key::Num1,
        19 => Key::Num2,
        20 => Key::Num3,
        21 => Key::Num4,
        22 => Key::Num6,
        23 => Key::Num5,
        24 => Key::Equals,
        25 => Key::Num9,
        26 => Key::Num7,
        27 => Key::Minus,
        28 => Key::Num8,
        29 => Key::Num0,
        30 => Key::CloseBracket,
        31 => Key::O,
        32 => Key::U,
        33 => Key::OpenBracket,
        34 => Key::I,
        35 => Key::P,
        36 => Key::Enter,
        37 => Key::L,
        38 => Key::J,
        39 => Key::Quote,
        40 => Key::K,
        41 => Key::Semicolon,
        42 => Key::Backslash,
        43 => Key::Comma,
        44 => Key::Slash,
        45 => Key::N,
        46 => Key::M,
        47 => Key::Period,
        48 => Key::Tab,
        49 => Key::Space,
        50 => Key::Backtick,
        51 => Key::Backspace,
        53 => Key::Escape,
        123 => Key::ArrowLeft,
        124 => Key::ArrowRight,
        125 => Key::ArrowDown,
        126 => Key::ArrowUp,
        _ => return None,
    })
}
