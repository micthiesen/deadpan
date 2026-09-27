//! Development audit of the real routers against an evaluated Kestrel registry.
//!
//! The fixture includes every exact modifier combination and app scope. A live
//! registry check compares its source digest, failing closed on any drift; it
//! never guesses at Swift syntax or runs source supplied to the Rust harness.

use std::collections::BTreeSet;

use eframe::egui::{Key, Modifiers};
use sha2::{Digest, Sha256};

use super::{Bindings, Pane, camera::CameraKey};

const FIXTURE: &str = include_str!("kestrel-reserved.tsv");

#[derive(Debug)]
pub struct ShortcutConflict {
    pub chord: String,
    pub context: String,
    pub response: String,
}

#[derive(Debug)]
pub struct ShortcutAudit {
    pub reserved_bindings: usize,
    pub routing_cases: usize,
    pub conflicts: Vec<ShortcutConflict>,
    pub source_sha256: String,
    pub live_source_sha256: Option<String>,
}

impl ShortcutAudit {
    pub fn passed(&self) -> bool {
        self.conflicts.is_empty()
            && self
                .live_source_sha256
                .as_ref()
                .is_none_or(|live| live == &self.source_sha256)
    }
}

struct Reservation {
    key: Key,
    modifiers: Modifiers,
    chord: String,
}

/// Check every global reservation against production routing, including pending
/// operators and counts. App-scoped Kestrel entries for other apps are excluded.
pub fn audit() -> Result<ShortcutAudit, String> {
    let (source_sha256, reservations) = parse_fixture(FIXTURE)?;
    let mut report = ShortcutAudit {
        reserved_bindings: reservations.len(),
        routing_cases: 0,
        conflicts: Vec::new(),
        source_sha256,
        live_source_sha256: None,
    };
    for reservation in reservations {
        audit_reservation(
            &reservation,
            &mut report,
            |bindings, key, modifiers, text, ime| bindings.key(key, modifiers, text, ime),
        );
    }
    Ok(report)
}

/// Check a caller-read UTF-8 Shortcuts.swift against the qualified registry.
/// No filesystem path is assumed and no external process is executed.
pub fn audit_with_kestrel_source(source: &str) -> Result<ShortcutAudit, String> {
    let mut report = audit()?;
    report.live_source_sha256 = Some(
        Sha256::digest(source.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    );
    Ok(report)
}

fn audit_reservation(
    reservation: &Reservation,
    report: &mut ShortcutAudit,
    route: impl Fn(&mut Bindings, Key, Modifiers, bool, bool) -> Option<super::Action>,
) {
    // Cover every state in the currently shipped prefix parser, including its
    // overflow branch. Prefixes and the final key both use production routing.
    let prefixes: &[&[Key]] = &[
        &[],
        &[Key::G],
        &[Key::R],
        &[Key::D],
        &[Key::Comma],
        &[Key::Num3],
        &[Key::Num3, Key::G],
        &[Key::Num3, Key::R],
        &[Key::Num3, Key::D],
        &[Key::Num3, Key::Comma],
        &[Key::Num9; 11],
    ];
    for (text, ime) in [(false, false), (true, false), (false, true), (true, true)] {
        for prefix in prefixes {
            let mut bindings = Bindings::default();
            for key in *prefix {
                bindings.key(*key, Modifiers::NONE, false, false);
            }
            let context = format!(
                "Normal prefix={:?} text={text} ime={ime}",
                bindings.pending()
            );
            let action = route(
                &mut bindings,
                reservation.key,
                reservation.modifiers,
                text,
                ime,
            );
            let pending = bindings.pending();
            record(
                report,
                reservation,
                context,
                (action.is_some() || !pending.is_empty())
                    .then(|| format!("action={action:?}, pending={pending:?}")),
            );
        }
        for repeat in [false, true] {
            let camera =
                super::route_camera_key(reservation.key, reservation.modifiers, text, ime, repeat);
            record(
                report,
                reservation,
                format!("Camera text={text} ime={ime} repeat={repeat}"),
                (!matches!(
                    camera,
                    None | Some(CameraKey::ClearCount | CameraKey::Ignore | CameraKey::Other)
                ))
                .then(|| format!("camera={camera:?}")),
            );
        }
        let text_action = super::text_action(reservation.key, reservation.modifiers, text, ime);
        let inspector = super::inspector_parameter_key(
            reservation.key,
            reservation.modifiers,
            Pane::Inspector,
            text,
            ime,
        );
        record(
            report,
            reservation,
            format!("Native text/inspector text={text} ime={ime}"),
            (text_action.is_some() || inspector)
                .then(|| format!("text={text_action:?}, inspector={inspector}")),
        );
    }
}

fn record(
    report: &mut ShortcutAudit,
    reservation: &Reservation,
    context: String,
    response: Option<String>,
) {
    report.routing_cases += 1;
    if let Some(response) = response {
        report.conflicts.push(ShortcutConflict {
            chord: reservation.chord.clone(),
            context,
            response,
        });
    }
}

fn parse_fixture(input: &str) -> Result<(String, Vec<Reservation>), String> {
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
/// positions to egui logical keys for the audit, not for production routing.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_routers_never_claim_a_kestrel_global_chord_or_prefix() {
        let report = audit().unwrap();
        assert_eq!(report.reserved_bindings, 62);
        assert_eq!(report.routing_cases, 62 * 56);
        assert!(report.passed(), "{report:#?}");
    }

    #[test]
    fn audit_rejects_the_old_command_return_binding() {
        let (_, reservations) = parse_fixture(FIXTURE).unwrap();
        let reservation = reservations
            .iter()
            .find(|entry| {
                entry.key == Key::Enter
                    && entry.modifiers == Modifiers::MAC_CMD | Modifiers::COMMAND
            })
            .unwrap();
        let mut report = audit().unwrap();
        audit_reservation(
            reservation,
            &mut report,
            |bindings, key, modifiers, text, ime| {
                if key == Key::Enter && modifiers.mac_cmd && !text && !ime {
                    Some(super::super::Action::Insert)
                } else {
                    bindings.key(key, modifiers, text, ime)
                }
            },
        );
        assert!(!report.passed());
        assert!(report.conflicts.iter().any(|conflict| {
            conflict.chord.contains("CMD+Enter") && conflict.response.contains("Insert")
        }));
    }

    #[test]
    fn audit_rejects_reserved_keys_that_only_start_a_count() {
        let (_, reservations) = parse_fixture(FIXTURE).unwrap();
        let reservation = reservations
            .iter()
            .find(|entry| entry.key == Key::Num1 && entry.modifiers == Modifiers::ALT)
            .unwrap();
        let mut report = audit().unwrap();
        audit_reservation(reservation, &mut report, |bindings, key, _, text, ime| {
            bindings.key(key, Modifiers::NONE, text, ime)
        });
        assert!(!report.passed());
        assert!(report.conflicts.iter().any(|conflict| {
            conflict.context.contains("prefix=\"\"") && conflict.response.contains("pending=\"1\"")
        }));
    }

    #[test]
    fn changed_live_registry_is_reported_as_drift() {
        let report = audit_with_kestrel_source("changed Swift registry").unwrap();
        assert!(report.conflicts.is_empty());
        assert_ne!(
            report.live_source_sha256.as_ref(),
            Some(&report.source_sha256)
        );
        assert!(!report.passed());
    }

    #[test]
    fn fixture_validation_cannot_silently_skip_unknown_bindings() {
        assert!(parse_fixture("").is_err());
        assert!(parse_fixture(&FIXTURE.replace("4\t2\t*", "999\t2\t*")).is_err());
        assert!(parse_fixture(&FIXTURE.replace("4\t2\t*", "4\t16\t*")).is_err());
        assert!(parse_fixture(&FIXTURE.replace("com.mitchellh.ghostty", "new.app")).is_err());
    }
}
