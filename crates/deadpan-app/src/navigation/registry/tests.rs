use super::baseline;

/// Rewrite a checked-in generated file on request, then fail so the rewrite
/// is reviewed and rerun; CI never rewrites.
fn rewrite_requested(variable: &str, path: &str, contents: &str) {
    if std::env::var_os(variable).is_none() {
        return;
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "{variable} rewrites checked-in files and is refused under CI"
    );
    std::fs::write(path, contents).unwrap();
    panic!("{path} rewritten; review the diff, then rerun without {variable}");
}

/// Every shipped binding, label, teaching line, former verb and its usage,
/// parse result and fixed mode-router decision resolves exactly as it did
/// before the registry (see `baseline.rs` for intended changes).
#[test]
fn routing_matches_the_pre_registry_baseline() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/navigation/registry/baseline-routing.txt"
    );
    let current = baseline::current(super::super::command::commands());
    rewrite_requested("DEADPAN_UPDATE_ROUTING_BASELINE", path, &current);
    let commands = super::super::command::commands();
    let old = baseline::old_commands();
    assert_eq!(old.len(), 115, "the baseline keeps every former verb");
    for (verb, usage) in &old {
        assert!(
            commands.contains(&(verb.as_str(), usage.as_str())),
            "the former verb {verb} or its usage changed"
        );
    }
    if current != baseline::BASELINE {
        let now: Vec<_> = current.lines().collect();
        let before: Vec<_> = baseline::BASELINE.lines().collect();
        let first = now
            .iter()
            .zip(&before)
            .position(|(now, before)| now != before)
            .unwrap_or(now.len().min(before.len()));
        panic!(
            "routing differs from the baseline: {} lines / {} bytes now, {} lines / {} bytes before; first difference at line {}:\n now:    {:?}\n before: {:?}",
            now.len(),
            current.len(),
            before.len(),
            baseline::BASELINE.len(),
            first + 1,
            now.get(first),
            before.get(first)
        );
    }
}

use eframe::egui::{Key, Modifiers};

use super::super::editor_map::{Compiled, KeyMode};
use super::super::{BindingId, EditSelection, RoutingDomain, camera, panels};
use super::super::{corrections, gain, room_tone, slip, splice, trim};
use super::{Contexts, KeyLabels, Keys, Mode, SPECS, VERBS};

struct Shipped(Compiled);

impl KeyLabels for Shipped {
    fn all(&self, id: BindingId) -> String {
        self.0.labels(id).join(" / ")
    }
    fn primary(&self, id: BindingId) -> String {
        self.0.labels(id).into_iter().next().unwrap_or_default()
    }
}

fn shipped() -> Shipped {
    Shipped(Compiled::compile(KeyMode::Logical, Vec::new()).unwrap())
}

#[test]
fn every_configurable_action_and_verb_is_described_exactly_once() {
    for id in BindingId::ALL {
        let owners: Vec<_> = SPECS
            .iter()
            .filter(|spec| spec.editor_keys().any(|candidate| candidate == id))
            .map(|spec| spec.id)
            .collect();
        assert_eq!(owners.len(), 1, "{} is owned by {owners:?}", id.as_str());
    }
    let mut verbs: Vec<_> = VERBS.iter().map(|(verb, _)| *verb).collect();
    let count = verbs.len();
    verbs.dedup();
    assert_eq!(verbs.len(), count, "a verb belongs to one action");
    let mut ids: Vec<_> = SPECS.iter().map(|spec| spec.id).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "action ids are unique");
    for spec in SPECS {
        assert!(
            !spec.name.is_empty() && !spec.help.is_empty(),
            "{}",
            spec.id
        );
        assert!(!spec.contexts.is_empty(), "{}", spec.id);
        assert!(
            !spec.keys.is_empty() || !spec.commands.is_empty(),
            "{} has neither keys nor a command",
            spec.id
        );
        assert_eq!(
            super::unknown_placeholders(spec.help),
            Vec::<String>::new(),
            "{}",
            spec.id
        );
        for command in spec.commands {
            assert!(
                command.usage.starts_with(&format!(":{}", command.verb)),
                "{}: {}",
                command.verb,
                command.usage
            );
        }
        for keys in spec.keys {
            if let Keys::Mode {
                mode,
                chords,
                label,
            } = keys
            {
                assert!(!chords.is_empty() && !label.is_empty(), "{}", spec.id);
                assert!(spec.contexts.contains(mode.context()), "{}", spec.id);
            }
        }
    }
    // Configurable actions keep their keymap names as ids where they stand
    // alone, so the reference and keymap.json name the same thing.
    for spec in SPECS {
        let ids: Vec<_> = spec.editor_keys().collect();
        if let [id] = ids.as_slice() {
            assert_eq!(spec.id, id.as_str(), "single-binding actions use its name");
        }
    }
}

/// A key only claims a timeline context where the compiled trie routes it.
#[test]
fn declared_timeline_contexts_are_routed_by_the_trie() {
    let compiled = shipped().0;
    for spec in SPECS {
        if spec.editor_keys().next().is_none() {
            continue;
        }
        for (context, selection, domain) in [
            (
                Contexts::ORIGINAL,
                EditSelection::None,
                RoutingDomain::Original,
            ),
            (Contexts::EDIT, EditSelection::None, RoutingDomain::Edit),
            (Contexts::VISUAL, EditSelection::Range, RoutingDomain::Edit),
        ] {
            if !spec.contexts.contains(context) {
                continue;
            }
            let routed = spec.editor_keys().any(|id| {
                compiled
                    .definition_paths(id)
                    .iter()
                    .any(|path| compiled.routes(path, selection, domain, id))
            });
            // An action with no shipped path (Ungroup), or whose verb also
            // serves that context (`:delete` over a Visual range), is reached
            // by its command there.
            let unbound = spec
                .editor_keys()
                .all(|id| compiled.definition_paths(id).is_empty());
            assert!(
                routed || unbound || !spec.commands.is_empty(),
                "{} claims {} but no key routes there",
                spec.id,
                context.label()
            );
        }
    }
}

const MODIFIER_CASES: [Modifiers; 11] = [
    Modifiers::NONE,
    Modifiers::SHIFT,
    Modifiers::CTRL,
    Modifiers::ALT,
    Modifiers::COMMAND,
    Modifiers {
        alt: false,
        ctrl: false,
        shift: false,
        mac_cmd: true,
        command: true,
    },
    Modifiers {
        alt: false,
        ctrl: false,
        shift: true,
        mac_cmd: false,
        command: true,
    },
    Modifiers {
        alt: false,
        ctrl: false,
        shift: true,
        mac_cmd: true,
        command: true,
    },
    Modifiers {
        alt: false,
        ctrl: true,
        shift: true,
        mac_cmd: false,
        command: false,
    },
    Modifiers {
        alt: true,
        ctrl: false,
        shift: true,
        mac_cmd: false,
        command: false,
    },
    Modifiers {
        alt: true,
        ctrl: false,
        shift: false,
        mac_cmd: true,
        command: true,
    },
];

/// Whether a mode router acts on this press in any of its states.
fn routes(mode: Mode, key: Key, modifiers: Modifiers) -> bool {
    let flags = (0..16u8).map(|flags| {
        [
            flags & 1 != 0,
            flags & 2 != 0,
            flags & 4 != 0,
            flags & 8 != 0,
        ]
    });
    let mut any = false;
    for [first, second, ime, repeat] in flags {
        any |= match mode {
            Mode::Camera => !matches!(
                camera::route_camera_key(key, modifiers, first, ime, repeat),
                None | Some(
                    camera::CameraKey::ClearCount
                        | camera::CameraKey::Ignore
                        | camera::CameraKey::Other
                )
            ),
            Mode::Trim => trim::route_key(key, modifiers, first, second, ime, repeat).is_some(),
            Mode::Slip => slip::route_key(key, modifiers, first, second, ime, repeat).is_some(),
            Mode::Splice => splice::route_key(key, modifiers, first, second, ime, repeat).is_some(),
            Mode::RoomTone => {
                room_tone::route_key(key, modifiers, first, second, ime, repeat).is_some()
            }
            Mode::Gain => gain::route_key(key, modifiers, first, second, ime, repeat).is_some(),
            Mode::Corrections => {
                corrections::route_key(key, modifiers, first, second, ime, repeat).is_some()
            }
            Mode::Marks => panels::marks_key(key, modifiers).is_some(),
            Mode::Jobs => panels::jobs_key(key, modifiers).is_some(),
            Mode::Storage => panels::storage_key(key, modifiers).is_some(),
            Mode::Help => panels::help_key(key, modifiers).is_some(),
        };
    }
    any
}

/// The registered action each router decision belongs to.
fn owner_of(mode: Mode, key: Key, modifiers: Modifiers, flags: [bool; 4]) -> Option<&'static str> {
    use camera::CameraKey as Cam;
    let [first, second, ime, repeat] = flags;
    Some(match mode {
        Mode::Camera => match camera::route_camera_key(key, modifiers, first, ime, repeat)? {
            Cam::Digit(_) | Cam::RefreshTargets => "camera.targets",
            Cam::Pan { .. } => "camera.pan",
            Cam::Scale(_) => "camera.scale",
            Cam::Reset => "camera.reset",
            Cam::NewRegion | Cam::Correct => "camera.region",
            Cam::Follow => "camera.follow",
            Cam::Track => "track",
            Cam::Tab { .. } | Cam::Arrow(_) => "camera.fields",
            Cam::Commit => "camera.apply",
            Cam::Cancel => "camera.cancel",
            Cam::ClearCount | Cam::Ignore | Cam::Other => return None,
        },
        Mode::Trim => match trim::route_key(key, modifiers, first, second, ime, repeat)? {
            trim::TrimKey::Cycle { .. } | trim::TrimKey::In | trim::TrimKey::Out => "trim.cycle",
            trim::TrimKey::Nudge(_) => "trim.nudge",
            trim::TrimKey::TogglePolicy => "trim.policy",
            trim::TrimKey::Compare => "trim.compare",
            trim::TrimKey::Play | trim::TrimKey::Loop => "trim.audition",
            trim::TrimKey::FocusAmount => "trim.amount",
            trim::TrimKey::Apply => "trim.apply",
            trim::TrimKey::Cancel => "trim.cancel",
        },
        Mode::Slip => match slip::route_key(key, modifiers, first, second, ime, repeat)? {
            slip::SlipKey::Nudge(_) => "slip.nudge",
            slip::SlipKey::Inspect(_) | slip::SlipKey::First | slip::SlipKey::Last => {
                "slip.inspect"
            }
            slip::SlipKey::Compare => "slip.compare",
            slip::SlipKey::Apply => "slip.apply",
            slip::SlipKey::Cancel => "slip.cancel",
        },
        Mode::Splice => match splice::route_key(key, modifiers, first, second, ime, repeat)? {
            splice::SpliceKey::Move | splice::SpliceKey::Replace => "splice.mode",
            splice::SpliceKey::In | splice::SpliceKey::Out => "splice.endpoints",
            splice::SpliceKey::Destination
            | splice::SpliceKey::Boundary(_)
            | splice::SpliceKey::Count(_) => "splice.destination",
            splice::SpliceKey::Removal
            | splice::SpliceKey::Picture
            | splice::SpliceKey::Step(_)
            | splice::SpliceKey::Compare => "splice.inspect",
            splice::SpliceKey::Play | splice::SpliceKey::Loop => "splice.audition",
            splice::SpliceKey::Apply => "splice.apply",
            splice::SpliceKey::Cancel => "splice.cancel",
        },
        Mode::RoomTone => match room_tone::route_key(key, modifiers, first, second, ime, repeat)? {
            room_tone::RoomToneKey::Play | room_tone::RoomToneKey::Loop => "room-tone.audition",
            room_tone::RoomToneKey::Apply => "room-tone.apply",
            room_tone::RoomToneKey::Cancel => "room-tone.cancel",
        },
        Mode::Gain => match gain::route_key(key, modifiers, first, second, ime, repeat)? {
            gain::GainKey::Play => "gain.draft.audition",
            gain::GainKey::Apply => "gain.draft.apply",
            gain::GainKey::Cancel => "gain.draft.cancel",
        },
        Mode::Corrections => {
            use corrections::CorrectionKey as K;
            match corrections::route_key(key, modifiers, first, second, ime, repeat)? {
                K::Previous | K::Next | K::Nudge { .. } | K::Snap { .. } => "corrections.step",
                K::StartEdge | K::EndEdge => "corrections.edge",
                K::EditText | K::Apply => "corrections.edit",
                K::Join => "corrections.join",
                K::Remove => "corrections.remove",
                K::Discard => "corrections.discard",
                K::AddPause => "corrections.pause",
                K::Undo | K::Redo => "corrections.history",
                K::Cancel => "corrections.close",
            }
        }
        // The panels look up the registry itself; their typed tables are
        // proven to cover exactly its chords in `panels.rs`.
        Mode::Marks | Mode::Jobs | Mode::Storage | Mode::Help => return None,
    })
}

/// Every chord a mode router acts on is declared in the registry, and every
/// declared chord reaches its router: the registry is the mode's key map.
/// Each chord has one owner, and that owner is the action the router takes.
#[test]
fn registered_mode_chords_are_exactly_what_each_router_handles() {
    for mode in Mode::ALL {
        let declared: Vec<_> = super::mode_chords(mode).collect();
        for key in Key::ALL {
            for modifiers in MODIFIER_CASES {
                let owners: Vec<_> = declared
                    .iter()
                    .filter(|(_, chord)| chord.key == *key && chord.mods.accepts(modifiers))
                    .map(|(spec, _)| spec.id)
                    .collect();
                let routed = routes(mode, *key, modifiers);
                assert_eq!(
                    routed,
                    !owners.is_empty(),
                    "{mode:?} {key:?} {modifiers:?}: routed={routed}, declared by {owners:?}"
                );
                let mut unique = owners.clone();
                unique.dedup();
                assert!(
                    unique.len() <= 1,
                    "{mode:?} {key:?} {modifiers:?}: {owners:?}"
                );
                for flags in 0..16u8 {
                    let flags = [0, 1, 2, 3].map(|bit| flags & (1 << bit) != 0);
                    if let Some(owner) = owner_of(mode, *key, modifiers, flags) {
                        assert_eq!(
                            unique.first(),
                            Some(&owner),
                            "{mode:?} {key:?} {modifiers:?} {flags:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn templates_follow_a_personal_keymap() {
    let custom = Compiled::compile(
        KeyMode::Logical,
        vec![(
            BindingId::CutOperator,
            vec![vec![super::super::editor_map::Stroke::Key(Key::X, true)]],
        )],
    )
    .unwrap();
    let help = super::render(super::by_id("object.pause").unwrap().help, &Shipped(custom));
    assert!(help.contains("Xip cuts the pause"), "{help}");
    let help = super::render(super::by_id("object.pause").unwrap().help, &shipped());
    assert!(help.contains("dip cuts the pause"), "{help}");
}

#[test]
fn help_search_ranks_exact_keys_then_verbs_then_names() {
    let labels = shipped();
    let rank = |id: &str, query: &str| {
        let spec = super::by_id(id).unwrap();
        super::search_rank(
            spec,
            &super::key_text(spec, &labels),
            &super::render(spec.help, &labels),
            query,
        )
    };
    assert_eq!(rank("cut.beat", "dd"), Some(0));
    assert_eq!(rank("hold", ",h"), Some(0));
    assert_eq!(rank("hold", ":hold"), Some(1));
    assert_eq!(rank("hold", "hold"), Some(1));
    assert_eq!(rank("hold", "pause"), Some(2));
    assert_eq!(rank("bounds", "G"), Some(0));
    assert_eq!(rank("bounds", "gg"), Some(0));
    // A command term never matches prose, and every term must match.
    assert_eq!(rank("split", ":hold"), None);
    assert_eq!(rank("hold", "pause zebra"), None);
    assert_eq!(rank("camera.pan", "camera"), Some(3));
    // Short key-like terms never match prose or names.
    assert_eq!(rank("sound.import", "dd"), None);
    assert_eq!(rank("repeat.set", "dd"), None);
    assert_eq!(rank("hold", "pa"), None);
    // Mixed queries keep their short words.
    assert_eq!(rank("repeat.operator", "Repeat a motion"), Some(2));
    // `:` alone lists every verb; a bare letter is a key, not a verb prefix.
    assert_eq!(rank("hold", ":"), Some(1));
    assert_eq!(rank("split", "s"), Some(0));
    assert_eq!(rank("sound.place", "s"), None);
}

/// The generated reference is current. Rewrite it with
/// `DEADPAN_UPDATE_COMMANDS_MD=1`.
#[test]
fn commands_reference_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/COMMANDS.md");
    let generated = super::reference_markdown(&shipped());
    rewrite_requested("DEADPAN_UPDATE_COMMANDS_MD", path, &generated);
    let current = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        current == generated,
        "docs/COMMANDS.md is stale; run DEADPAN_UPDATE_COMMANDS_MD=1 cargo test -p deadpan-app --bin deadpan-app commands_reference"
    );
}

#[test]
fn mode_labels_keep_the_footer_and_button_text() {
    // Footers and buttons read these labels; their painted text is unchanged.
    for (id, label) in [
        ("gain.draft.audition", "Space"),
        ("gain.draft.apply", "Enter"),
        ("gain.draft.cancel", "Esc"),
        ("camera.pan", "h j k l"),
        ("camera.scale", "+ −"),
        ("camera.targets", "f"),
        ("camera.follow", "t"),
        ("track", "T"),
        ("camera.fields", "Tab"),
        ("camera.apply", "Enter"),
        ("camera.reset", "r"),
        ("camera.cancel", "Esc"),
        ("slip.apply", "Enter"),
        ("slip.cancel", "Esc"),
        ("trim.apply", "Enter"),
        ("trim.cancel", "Esc"),
        ("splice.apply", "Enter"),
        ("splice.cancel", "Esc"),
        ("room-tone.apply", "Enter"),
        ("room-tone.cancel", "Esc"),
        ("help.search", "/"),
        ("help.scroll", "j / k · PgUp / PgDn · Home / End"),
        ("help.close", "Esc"),
    ] {
        assert_eq!(super::mode_label(id), label, "{id}");
    }
    assert_eq!(super::mode_label_at("camera.region", 0), "n");
    assert_eq!(super::mode_label_at("camera.region", 1), "c");
}

/// Every mode label the interface paints names a registered key.
#[test]
fn every_painted_mode_label_is_registered() {
    let mut used = Vec::new();
    let mut pending = vec![std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/preview"
    ))];
    pending.push(concat!(env!("CARGO_MANIFEST_DIR"), "/src/preview.rs").into());
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(
                std::fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for call in ["mode_label(\"", "mode_label_at(\"", " key(\"", "key_at(\""] {
            for (start, _) in source.match_indices(call) {
                let rest = &source[start + call.len()..];
                let id = &rest[..rest.find('"').unwrap()];
                let index = rest[id.len() + 1..]
                    .strip_prefix(", ")
                    .and_then(|tail| tail[..1].parse::<usize>().ok())
                    .unwrap_or(0);
                used.push((path.display().to_string(), id.to_owned(), index));
            }
        }
    }
    assert!(used.len() > 20, "found {used:?}");
    for (path, id, index) in used {
        assert_ne!(
            super::mode_label_at(&id, index),
            "?",
            "{path}: {id} {index}"
        );
    }
    assert_eq!(
        super::mode_label("no.such.action"),
        "?",
        "painting never panics"
    );
}

/// Placeholders resolve to one visible path under the shipped map, and an
/// unbound action shows a marker instead of vanishing from the prose.
#[test]
fn every_placeholder_resolves_to_a_visible_path() {
    let labels = shipped();
    for spec in SPECS {
        let mut rest = spec.help;
        while let Some(start) = rest.find('{') {
            let after = &rest[start + 1..];
            let end = after.find('}').unwrap();
            let name = &after[..end];
            let (name, primary) = match name.strip_suffix('!') {
                Some(name) => (name, true),
                None => (name, false),
            };
            let id = BindingId::from_name(name).unwrap();
            let label = if primary {
                labels.primary(id)
            } else {
                labels.all(id)
            };
            assert!(!label.is_empty(), "{}: {{{name}}} is unbound", spec.id);
            if primary {
                assert!(
                    !label.contains(" / "),
                    "{}: {{{name}!}} is one path",
                    spec.id
                );
            }
            rest = &after[end + 1..];
        }
    }
    assert_eq!(
        super::render("Use {group.ungroup}.", &labels),
        "Use (unbound group.ungroup)."
    );
}

/// Every headless link names a PARITY.md section.
#[test]
fn parity_anchors_resolve() {
    let parity =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/PARITY.md"))
            .unwrap();
    let anchors: Vec<String> = parity
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .map(|heading| {
            heading
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
                .collect::<String>()
                .replace(' ', "-")
        })
        .collect();
    for spec in SPECS {
        assert!(
            anchors.iter().any(|anchor| anchor == spec.headless.anchor),
            "{}: #{} is not a PARITY.md section ({anchors:?})",
            spec.id,
            spec.headless.anchor
        );
    }
}
