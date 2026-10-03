//! Development audit of the real routers against an evaluated Kestrel registry.
//!
//! The fixture includes every exact modifier combination and app scope. A live
//! registry check compares its source digest, failing closed on any drift; it
//! never guesses at Swift syntax or runs source supplied to the Rust harness.

use eframe::egui::{Key, Modifiers};
use sha2::{Digest, Sha256};

use super::{Bindings, Pane, camera::CameraKey};

use super::keymap_config::reservations::{FIXTURE, Reservation, parse_fixture};

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

/// Check every global reservation against production routing, including pending
/// operators and counts. App-scoped Kestrel entries for other apps are excluded.
pub fn audit() -> Result<ShortcutAudit, String> {
    audit_bindings(&Bindings::default())
}

/// Audit every structural branch of this complete immutable candidate.
pub fn audit_bindings(template: &Bindings) -> Result<ShortcutAudit, String> {
    let (source_sha256, reservations) = parse_fixture(FIXTURE)?;
    let mut report = ShortcutAudit {
        reserved_bindings: reservations.len(),
        routing_cases: 0,
        conflicts: Vec::new(),
        source_sha256,
        live_source_sha256: None,
    };
    for reservation in reservations {
        for domain in [
            super::RoutingDomain::Edit,
            super::RoutingDomain::Original,
            super::RoutingDomain::Sound,
        ] {
            for recording in [false, true] {
                let mut template = template.clone();
                template.clear();
                template.set_routing_domain(domain);
                template.set_macro_recording(recording);
                audit_reservation_for(
                    &template,
                    &reservation,
                    &mut report,
                    |bindings, key, modifiers, text, ime| bindings.key(key, modifiers, text, ime),
                );
                audit_layout_reservation(&template, &reservation, &mut report);
            }
        }
    }
    Ok(report)
}

/// Layout translation must never disguise a reserved physical chord as a
/// logical punctuation key, native menu key, or mark-name completion.
fn audit_layout_reservation(
    template: &Bindings,
    reservation: &Reservation,
    report: &mut ShortcutAudit,
) {
    for prefix in audit_prefixes_for(template) {
        for selection in [
            super::EditSelection::None,
            super::EditSelection::Empty,
            super::EditSelection::Range,
        ] {
            for (text, ime) in [(false, false), (true, false), (false, true), (true, true)] {
                for (logical, logical_text) in [
                    (Key::Comma, None),
                    (Key::Period, None),
                    (Key::Colon, None),
                    (Key::N, None),
                    (Key::A, None),
                    (Key::Quote, None),
                    (Key::Num2, Some("@")),
                ] {
                    let mut bindings = template.clone();
                    bindings.clear();
                    for stroke in &prefix {
                        bindings.audit_stroke(*stroke, selection);
                    }
                    let context = format!(
                        "Layout domain={:?} physical={:?} logical={logical:?} logical_text={logical_text:?} recording={} selection={selection:?} prefix={:?} text={text} ime={ime}",
                        template.domain,
                        reservation.key,
                        template.macro_recording,
                        bindings.pending()
                    );
                    let action = bindings.route_event_with_logical_text(
                        logical,
                        Some(reservation.key),
                        reservation.modifiers,
                        text,
                        ime,
                        false,
                        true,
                        selection,
                        logical_text,
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
            }
        }
    }
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

#[cfg(test)]
fn audit_reservation(
    reservation: &Reservation,
    report: &mut ShortcutAudit,
    route: impl Fn(&mut Bindings, Key, Modifiers, bool, bool) -> Option<super::Action>,
) {
    audit_reservation_for(&Bindings::default(), reservation, report, route);
}

fn audit_reservation_for(
    template: &Bindings,
    reservation: &Reservation,
    report: &mut ShortcutAudit,
    route: impl Fn(&mut Bindings, Key, Modifiers, bool, bool) -> Option<super::Action>,
) {
    // Derive every pending branch from the compiled production grammar, then
    // exercise absent, positive, zero and overflow counts before that branch.
    let prefixes = audit_prefixes_for(template);
    for (text, ime) in [(false, false), (true, false), (false, true), (true, true)] {
        for prefix in &prefixes {
            let mut bindings = template.clone();
            bindings.clear();
            for stroke in prefix {
                bindings.audit_stroke(*stroke, super::EditSelection::None);
            }
            let context = format!(
                "Normal domain={:?} recording={} prefix={:?} text={text} ime={ime}",
                template.domain,
                template.macro_recording,
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
            for selection in [super::EditSelection::Empty, super::EditSelection::Range] {
                let mut bindings = template.clone();
                bindings.clear();
                for stroke in prefix {
                    bindings.audit_stroke(*stroke, selection);
                }
                let context = format!(
                    "Visual domain={:?} recording={} selection={selection:?} prefix={:?} text={text} ime={ime}",
                    template.domain,
                    template.macro_recording,
                    bindings.pending()
                );
                let action = bindings.key_with_selection(
                    reservation.key,
                    reservation.modifiers,
                    text,
                    ime,
                    selection,
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
        }
        for repeat in [false, true] {
            for background in [false, true] {
                let action = super::room_tone::route_key(
                    reservation.key,
                    reservation.modifiers,
                    text,
                    background,
                    ime,
                    repeat,
                );
                record(
                    report,
                    reservation,
                    format!(
                        "Room tone text={text} background={background} ime={ime} repeat={repeat}"
                    ),
                    action.map(|action| format!("room-tone={action:?}")),
                );
                let gain = super::gain::route_key(
                    reservation.key,
                    reservation.modifiers,
                    text,
                    background,
                    ime,
                    repeat,
                );
                record(
                    report,
                    reservation,
                    format!("Gain text={text} background={background} ime={ime} repeat={repeat}"),
                    gain.map(|action| format!("gain={action:?}")),
                );
                let slip = super::slip::route_key(
                    reservation.key,
                    reservation.modifiers,
                    text,
                    background,
                    ime,
                    repeat,
                );
                record(
                    report,
                    reservation,
                    format!("Slip text={text} background={background} ime={ime} repeat={repeat}"),
                    slip.map(|action| format!("slip={action:?}")),
                );
                let splice = super::splice::route_key(
                    reservation.key,
                    reservation.modifiers,
                    text,
                    background,
                    ime,
                    repeat,
                );
                record(
                    report,
                    reservation,
                    format!(
                        "Place slice text={text} background={background} ime={ime} repeat={repeat}"
                    ),
                    splice.map(|action| format!("splice={action:?}")),
                );
                let trim = super::trim::route_key(
                    reservation.key,
                    reservation.modifiers,
                    text,
                    background,
                    ime,
                    repeat,
                );
                record(
                    report,
                    reservation,
                    format!("Trim text={text} background={background} ime={ime} repeat={repeat}"),
                    trim.map(|action| format!("trim={action:?}")),
                );
            }
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

#[cfg(test)]
fn audit_prefixes() -> Vec<Vec<super::editor_map::Stroke>> {
    audit_prefixes_for(&Bindings::default())
}

fn audit_prefixes_for(template: &Bindings) -> Vec<Vec<super::editor_map::Stroke>> {
    use super::editor_map::Stroke;
    let counts = [
        Vec::new(),
        vec![Stroke::Key(Key::Num3, false)],
        vec![Stroke::Key(Key::Num0, false)],
        vec![Stroke::Key(Key::Num9, false); 11],
    ];
    let mut cases = Vec::new();
    for prefix in template.map.prefix_paths() {
        for count in &counts {
            let mut path = count.clone();
            path.extend_from_slice(&prefix);
            cases.push(path);
        }
        for depth in 1..=prefix.len() {
            if !template
                .map
                .prefix(
                    &prefix[..depth],
                    super::EditSelection::None,
                    super::RoutingDomain::Edit,
                )
                .is_some_and(super::editor_map::PrefixKind::is_operator)
            {
                continue;
            }
            for count in &counts[1..] {
                for outer in [&counts[0], &counts[1]] {
                    let mut path = outer.clone();
                    path.extend_from_slice(&prefix[..depth]);
                    path.extend_from_slice(count);
                    path.extend_from_slice(&prefix[depth..]);
                    cases.push(path);
                }
            }
        }
    }
    cases
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_routers_never_claim_a_kestrel_global_chord_or_prefix() {
        let report = audit().unwrap();
        assert_eq!(report.reserved_bindings, 62);
        let prefixes = audit_prefixes();
        let prefix_cases = prefixes.len();
        use super::super::editor_map::Stroke;
        for keys in [
            vec![Key::Y],
            vec![Key::D, Key::G],
            vec![Key::D, Key::Num3],
            vec![Key::Y, Key::Num0, Key::G],
            vec![Key::Num3, Key::Y, Key::Num3],
            vec![Key::R, Key::G],
            vec![Key::R, Key::Num3],
            vec![Key::R, Key::Num0, Key::G],
            vec![Key::Num3, Key::R, Key::Num3],
        ] {
            assert!(
                prefixes.contains(
                    &keys
                        .into_iter()
                        .map(|key| Stroke::Key(key, false))
                        .collect()
                )
            );
        }
        let editor_cases = prefix_cases * 3 * 4; // Normal/Empty/Range × text/IME.
        // Five drafts × repeat/background, Camera × repeat, and native input,
        // each under all four text/IME combinations.
        let mode_cases = 4 * (5 * 2 * 2 + 2 + 1);
        let layout_cases = prefix_cases * 3 * 4 * 7; // Includes paired logical @.
        assert_eq!(
            report.routing_cases,
            62 * 3 * 2 * (editor_cases + mode_cases + layout_cases)
        );
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
    fn audit_rejects_reserved_keys_that_only_resolve_a_mark_prefix() {
        let (_, reservations) = parse_fixture(FIXTURE).unwrap();
        let reservation = reservations
            .iter()
            .find(|entry| entry.key == Key::H && entry.modifiers == Modifiers::ALT)
            .unwrap();
        let mut report = audit().unwrap();
        audit_reservation(
            reservation,
            &mut report,
            |bindings, key, modifiers, text, ime| {
                if bindings.mark_prefix().is_some() && !text && !ime {
                    bindings.key(key, Modifiers::NONE, false, false)
                } else {
                    bindings.key(key, modifiers, text, ime)
                }
            },
        );
        assert!(!report.passed());
        assert!(report.conflicts.iter().any(|conflict| {
            conflict.context.contains("prefix=\"m\"") && conflict.response.contains("SetMark('h')")
        }));
        assert!(report.conflicts.iter().any(|conflict| {
            conflict.context.contains("prefix=\"'\"") && conflict.response.contains("JumpMark('h')")
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
