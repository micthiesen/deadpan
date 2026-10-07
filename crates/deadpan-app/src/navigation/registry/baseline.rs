//! A textual snapshot of every shipped key path, label, command verb, parse
//! result and fixed mode-router decision. The checked-in copy was generated
//! before the registry existed and proves the move changed none of them.
//! Verb lines cover the former verbs only, so adding a verb leaves it intact;
//! the former verbs are read back from those lines. Later sections, appended
//! with additions only, pin the paths the app dispatches through as well:
//! `mode_key` companion text, Camera's `dispatch_key` and its text/IME
//! answers, held-key state in `Bindings::route_event`, more modifier
//! combinations and the registry-driven panels. A changed line is a changed
//! binding, verb or router decision: regenerate with
//! `DEADPAN_UPDATE_ROUTING_BASELINE=1` only for an intended change, and review
//! the diff (the test then fails until rerun without it).

use std::fmt::Write as _;

use eframe::egui::{Key, Modifiers};

use super::super::editor_map::{Compiled, Counts, KeyMode, Stroke};
use super::super::{BindingId, Bindings, EditSelection, RoutingDomain, command};

pub(super) const BASELINE: &str = include_str!("baseline-routing.txt");

/// The former verbs and usages, read back from the checked-in baseline's
/// `verb` lines (one copy of the old table).
pub(super) fn old_commands() -> Vec<(String, String)> {
    fn unquote(text: &str) -> String {
        let inner = &text[1..text.len() - 1];
        inner.replace("\\\"", "\"").replace("\\\\", "\\")
    }
    BASELINE
        .lines()
        .filter_map(|line| line.strip_prefix("verb "))
        .filter_map(|rest| {
            let (verb, usage) = rest.split_once(" usage ")?;
            Some((unquote(verb), unquote(usage)))
        })
        .collect()
}

/// Extra arguments chosen to reach each parser branch, besides the usage.
const EXTRA_INPUTS: &[&str] = &[
    ":hold 12f video=black",
    ":hold 1.5s video=freeze audio=silence",
    ":hold 1.5s video=ai",
    ":hold 1.5s video=ai audio=silence",
    ":hold 1.5s audio=silence video=ai",
    ":hold 0f video=ai audio=silence",
    ":hold 1s video=ai video=freeze",
    ":hold 1s video=ai audio=room-tone",
    ":hold 0s",
    ":hold -1s",
    ":repeat 3 gap=120ms gain-step=3dB zoom-step=0.08",
    ":repeat 3 role=audio overflow=trim",
    ":repeat 0",
    ":repeat 1 2",
    ":zoom 1.35 target=face:2 curve=step",
    ":zoom off",
    ":creep from=1 to=1.4 target=current",
    ":gain +6dB",
    ":gain +=3dB",
    ":gain -=3",
    ":gain mute",
    ":gain -6dB range=4-10f",
    ":retime 0.75 pitch=preserve",
    ":retime 3/4 pitch=tape",
    ":retime 1 pitch=-2st",
    ":wrap-retime 2 pitch=tape",
    ":pitch +3st",
    ":pitch 0",
    ":cutaway register=r audio=keep",
    ":cutaway register=r fit=bounce",
    ":cutaway clear",
    ":tail 400ms effect=reverb",
    ":tail 1s effect=delay",
    ":trim edge=out delta=-3f mode=ripple",
    ":trim edge=in",
    ":slip +5f",
    ":slip -3f extra",
    ":roll +2f",
    ":select role=audio",
    ":select role=linked",
    ":delete role=video",
    ":delete role=linked",
    ":group name=\"the uncomfortable answer\"",
    ":group",
    ":render now",
    ":scope play 2",
    ":scope plays 1,3-4",
    ":scope plays 2",
    ":scope none",
    ":macro a 3",
    ":macro A",
    ":macro a 0",
    ":record ab",
    ":register \"",
    ":register 1",
    ":mark Z",
    ":jump",
    ":generate 3",
    ":generate 9",
    ":pick-ai 0",
    ":delete-frames",
    ":delete-frames 0f",
    ":hold-duration 0f",
    ":hold-duration 2s",
    ":audio-lag +80ms",
    ":audio-lag -2f",
    ":audio-lag 0",
    ":audition-context lead=0ms follow=1s",
    ":monitor 0",
    ":monitor 12.5%",
    ":proxies retry",
    ":sound-channels stereo_left_right",
    ":sound-at 137",
    ":sound-gain -96",
    ":sound-edges hard",
    ":saturate off",
    ":saturate 30dB",
    ":edge hard plays",
    ":edge auto gaps",
    ":gag long-answer pause=1.5s",
    ":gag one-more-time vary=20% seed=7",
    ":gag stutter-4 plays=5",
    ":gag-inspect one-more-time vary=20%",
    ":gag-set plays=4 gap=400ms",
    ":gag-save a b",
    ":caption Are we done? at=top delay=12f reveal=3",
    ":caption clear",
    ":track target-1 through-shots",
    ":track through-shots target-1",
    ":track-cancel now",
    ":hold-provider ai",
    ":hold-provider fallback",
    ":hold-provider ai 2",
    ":ai-hold 1.5s",
    ":ai-hold 12f",
    ":ai-hold 0f",
    ":ai-hold 12f extra",
    ":revert-ai now",
    ":bleep 880Hz level=-6dB",
    ":jcut 6f",
    ":lcut 200ms",
    ":reverse 8f",
    ":ping-pong 12f",
    ":recipe-save a",
    ":recipe 1",
    ":framing-save Q",
    ":close now",
    ":help me",
    ":nonsense",
    "",
    "   ",
    ":",
    "HOLD 1s",
];

/// Repeated resolutions and teaching lines are written once, as `#n`.
fn intern(table: &mut Vec<String>, value: String) -> String {
    let index = table
        .iter()
        .position(|known| *known == value)
        .unwrap_or_else(|| {
            table.push(value);
            table.len() - 1
        });
    format!("#{index}")
}

fn editor_section(out: &mut String, mode: KeyMode, compiled: &Compiled, table: &mut Vec<String>) {
    for id in BindingId::ALL {
        writeln!(
            out,
            "label {mode:?} {} = {:?}",
            id.as_str(),
            compiled.labels(id)
        )
        .unwrap();
    }
    for (selection, domain, name) in [
        (EditSelection::None, RoutingDomain::Edit, "normal"),
        (EditSelection::Range, RoutingDomain::Edit, "visual"),
        (EditSelection::None, RoutingDomain::Original, "original"),
    ] {
        let mut pending: Vec<Vec<Stroke>> = vec![Vec::new()];
        let mut lines = Vec::new();
        while let Some(path) = pending.pop() {
            let trie = compiled.map(selection, domain);
            let node = trie.resolve(&path).expect("path from this trie");
            let label = compiled.path_label(&path);
            if let Some(binding) = node.terminal() {
                let rule = &binding.value;
                let mut counts = String::new();
                for (leading, motion) in [
                    (None, None),
                    (Some(1), None),
                    (Some(3), None),
                    (Some(0), None),
                    (None, Some(2)),
                    (Some(2), Some(2)),
                ] {
                    let resolved = format!("{:?}", rule.resolve_counts(Counts { leading, motion }));
                    write!(
                        counts,
                        " [{leading:?},{motion:?}]={}",
                        intern(table, resolved)
                    )
                    .unwrap();
                }
                lines.push(format!(
                    "{mode:?} {name} {label:?} -> {} repeatable={} interrupt={}{counts}",
                    rule.id().as_str(),
                    rule.repeatable,
                    rule.interrupt
                ));
            }
            if let Some(prefix) = node.prefix() {
                lines.push(format!(
                    "{mode:?} {name} {label:?} prefix {:?}",
                    prefix.value
                ));
            }
            if !path.is_empty() && node.children().next().is_some() {
                for (counts, recording) in [
                    (Counts::default(), false),
                    (Counts::default(), true),
                    (
                        Counts {
                            leading: Some(3),
                            motion: None,
                        },
                        false,
                    ),
                    (
                        Counts {
                            leading: Some(0),
                            motion: None,
                        },
                        false,
                    ),
                ] {
                    let teaching = format!(
                        "{:?} | {:?}",
                        compiled.hint(&path, selection, counts, recording, domain),
                        compiled.next_keys(&path, selection, counts, recording, domain),
                    );
                    lines.push(format!(
                        "{mode:?} {name} {label:?} teach leading={:?} recording={recording}: {}",
                        counts.leading,
                        intern(table, teaching)
                    ));
                }
            }
            for (stroke, _) in node.children() {
                let mut child = path.clone();
                child.push(*stroke);
                pending.push(child);
            }
        }
        lines.sort();
        for line in lines {
            writeln!(out, "{line}").unwrap();
        }
    }
}

const MODIFIER_CASES: [(&str, Modifiers); 15] = [
    ("none", Modifiers::NONE),
    ("shift", Modifiers::SHIFT),
    ("ctrl", Modifiers::CTRL),
    ("alt", Modifiers::ALT),
    ("command", Modifiers::COMMAND),
    (
        "mac-cmd",
        Modifiers {
            alt: false,
            ctrl: false,
            shift: false,
            mac_cmd: true,
            command: true,
        },
    ),
    (
        "shift-command",
        Modifiers {
            alt: false,
            ctrl: false,
            shift: true,
            mac_cmd: false,
            command: true,
        },
    ),
    (
        "shift-mac-cmd",
        Modifiers {
            alt: false,
            ctrl: false,
            shift: true,
            mac_cmd: true,
            command: true,
        },
    ),
    (
        "ctrl-shift",
        Modifiers {
            alt: false,
            ctrl: true,
            shift: true,
            mac_cmd: false,
            command: false,
        },
    ),
    (
        "ctrl-alt",
        Modifiers {
            alt: true,
            ctrl: true,
            shift: false,
            mac_cmd: false,
            command: false,
        },
    ),
    (
        "ctrl-command",
        Modifiers {
            alt: false,
            ctrl: true,
            shift: false,
            mac_cmd: false,
            command: true,
        },
    ),
    (
        "ctrl-mac-cmd",
        Modifiers {
            alt: false,
            ctrl: true,
            shift: false,
            mac_cmd: true,
            command: true,
        },
    ),
    (
        "alt-command",
        Modifiers {
            alt: true,
            ctrl: false,
            shift: false,
            mac_cmd: false,
            command: true,
        },
    ),
    (
        "alt-mac-cmd",
        Modifiers {
            alt: true,
            ctrl: false,
            shift: false,
            mac_cmd: true,
            command: true,
        },
    ),
    (
        "mac-cmd-only",
        Modifiers {
            alt: false,
            ctrl: false,
            shift: false,
            mac_cmd: true,
            command: false,
        },
    ),
];

fn mode_section(out: &mut String) {
    use super::super::{camera, corrections, gain, room_tone, slip, splice, trim};
    for key in Key::ALL {
        for (modifier_name, modifiers) in MODIFIER_CASES {
            for flags in 0..16u8 {
                let [first, second, ime, repeat] = [
                    flags & 1 != 0,
                    flags & 2 != 0,
                    flags & 4 != 0,
                    flags & 8 != 0,
                ];
                let case = format!("{key:?} {modifier_name} {first} {second} {ime} {repeat}");
                if let Some(action) =
                    room_tone::route_key(*key, modifiers, first, second, ime, repeat)
                {
                    writeln!(out, "room-tone {case} -> {action:?}").unwrap();
                }
                if let Some(action) = gain::route_key(*key, modifiers, first, second, ime, repeat) {
                    writeln!(out, "gain {case} -> {action:?}").unwrap();
                }
                if let Some(action) = slip::route_key(*key, modifiers, first, second, ime, repeat) {
                    writeln!(out, "slip {case} -> {action:?}").unwrap();
                }
                if let Some(action) = splice::route_key(*key, modifiers, first, second, ime, repeat)
                {
                    writeln!(out, "splice {case} -> {action:?}").unwrap();
                }
                if let Some(action) = trim::route_key(*key, modifiers, first, second, ime, repeat) {
                    writeln!(out, "trim {case} -> {action:?}").unwrap();
                }
                if let Some(action) =
                    corrections::route_key(*key, modifiers, first, second, ime, repeat)
                {
                    writeln!(out, "corrections {case} -> {action:?}").unwrap();
                }
                // Text and composition always clear the count (one line each).
                if !second && !(first || ime) || (!second && *key == Key::A && modifiers.is_none())
                {
                    let action = camera::route_camera_key(*key, modifiers, first, ime, repeat);
                    if !matches!(
                        action,
                        Some(camera::CameraKey::Other | camera::CameraKey::Ignore)
                    ) {
                        writeln!(out, "camera {case} -> {action:?}").unwrap();
                    }
                }
            }
        }
    }
}

fn command_section(out: &mut String, commands: &[(&str, &str)]) {
    let old = old_commands();
    for (verb, _) in &old {
        let usage = commands
            .iter()
            .find(|(current, _)| current == verb)
            .map(|(_, usage)| *usage);
        match usage {
            Some(usage) => writeln!(out, "verb {verb:?} usage {usage:?}").unwrap(),
            None => writeln!(out, "verb {verb:?} missing").unwrap(),
        }
    }
    let mut inputs: Vec<String> = Vec::new();
    for (verb, usage) in &old {
        inputs.push(format!(":{verb}"));
        inputs.push(format!(":{}", verb.to_ascii_uppercase()));
        inputs.push(format!(":{verb} x y z"));
        let example = usage
            .split("  ")
            .next()
            .unwrap_or(usage)
            .replace(['[', ']'], "");
        let example: String = example
            .split_whitespace()
            .map(|word| word.split('|').next().unwrap_or(word))
            .collect::<Vec<_>>()
            .join(" ");
        inputs.push(example);
    }
    inputs.extend(EXTRA_INPUTS.iter().map(|input| (*input).to_owned()));
    for input in inputs {
        writeln!(out, "parse {input:?} = {:?}", command::parse(&input)).unwrap();
    }
    for prefix in ["", "s", "so", "gag", "re", ":ho", "HO", "x", "hold 1s"] {
        let mut completions = command::completions(prefix);
        completions.retain(|usage| old.iter().any(|(_, former)| former == usage));
        writeln!(out, "complete {prefix:?} = {completions:?}").unwrap();
    }
}

/// The current routing, verbs and parses rendered with the same rules as the
/// checked-in baseline.
pub(super) fn current(commands: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut table = Vec::new();
    for mode in [KeyMode::Logical, KeyMode::Physical] {
        let compiled = Compiled::compile(mode, Vec::new()).expect("shipped grammar");
        editor_section(&mut out, mode, &compiled, &mut table);
    }
    for (index, value) in table.iter().enumerate() {
        writeln!(out, "#{index} = {value}").unwrap();
    }
    command_section(&mut out, commands);
    mode_section(&mut out);
    dispatch_section(&mut out);
    out
}

/// Typed characters a layout delivers with a press: none, letters and digits,
/// shifted punctuation, symbols no binding names and non-Latin letters.
const COMPANIONS: [Option<&str>; 16] = [
    None,
    Some("a"),
    Some("H"),
    Some("3"),
    Some("@"),
    Some("["),
    Some("]"),
    Some("/"),
    Some(":"),
    Some("\""),
    Some("'"),
    Some("&"),
    Some("é"),
    Some("ж"),
    Some("р"),
    Some("η"),
];

/// Cases with identical answers share one line of answers and one of cases,
/// in first-appearance order.
fn grouped(out: &mut String, prefix: &str, cases: Vec<(String, Vec<String>)>) {
    let mut groups: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    for (case, answers) in cases {
        match groups.iter_mut().find(|(known, _)| *known == answers) {
            Some((_, members)) => members.push(case),
            None => groups.push((answers, vec![case])),
        }
    }
    for (index, (answers, members)) in groups.iter().enumerate() {
        writeln!(
            out,
            "{prefix} group {index} answers: {}",
            answers.join(" | ")
        )
        .unwrap();
        writeln!(out, "{prefix} group {index} cases: {}", members.join(", ")).unwrap();
    }
}

/// One scripted press or release: key, modifiers, repeat, pressed.
type Step = (Key, Modifiers, bool, bool);

/// The paths the app dispatches through beyond the pure routers.
fn dispatch_section(out: &mut String) {
    use super::super::{camera, mode_key, panels};
    // Camera under text or composition always clears its count.
    let mut cleared = 0;
    for key in Key::ALL {
        for (name, modifiers) in MODIFIER_CASES {
            for (text, ime, repeat) in [
                (true, false, false),
                (false, true, false),
                (true, true, true),
            ] {
                match camera::route_camera_key(*key, modifiers, text, ime, repeat) {
                    Some(camera::CameraKey::ClearCount) => cleared += 1,
                    other => writeln!(
                        out,
                        "camera-text {key:?} {name} {text} {ime} {repeat} -> {other:?}"
                    )
                    .unwrap(),
                }
            }
        }
    }
    writeln!(out, "camera-text cleared {cleared}").unwrap();
    // Camera's dispatch around fields, activation and global chords: each
    // key and modifier case's 32 answers (text, ime, repeat, field,
    // activation as bits), grouped by identical answers.
    let mut cases = Vec::new();
    for key in Key::ALL {
        for (name, modifiers) in MODIFIER_CASES {
            let answers = (0..32u8)
                .map(|flags| {
                    let flags = [0, 1, 2, 3, 4].map(|bit| flags & (1 << bit) != 0);
                    crate::preview::camera_dispatch_debug(*key, modifiers, flags)
                })
                .collect();
            cases.push((format!("{key:?} {name}"), answers));
        }
    }
    grouped(out, "camera-dispatch", cases);
    // Companion text decides what the mode routers and panels see; answers
    // per companion in `COMPANIONS` order, grouped likewise.
    let mut cases = Vec::new();
    for key in Key::ALL {
        for (name, modifiers) in MODIFIER_CASES {
            let answers = COMPANIONS
                .iter()
                .map(|companion| {
                    let routed = mode_key(*key, modifiers, *companion);
                    if routed == Some((*key, modifiers)) {
                        "same".to_owned()
                    } else {
                        format!("{routed:?}")
                    }
                })
                .collect();
            cases.push((format!("{key:?} {name}"), answers));
        }
    }
    grouped(out, "mode-key", cases);
    // Panels read their keys from the registry.
    for key in Key::ALL {
        for (name, modifiers) in MODIFIER_CASES {
            let results = [
                panels::jobs_key(*key, modifiers).map(|action| format!("jobs {action:?}")),
                panels::storage_key(*key, modifiers).map(|action| format!("storage {action:?}")),
                panels::marks_key(*key, modifiers).map(|forward| format!("marks {forward}")),
                panels::help_key(*key, modifiers).map(|action| format!("keys-sheet {action:?}")),
            ];
            for result in results.into_iter().flatten() {
                writeln!(out, "panel {key:?} {name} -> {result}").unwrap();
            }
        }
    }
    // Held keys keep their resolved action and never enter a new prefix.
    let press = |key, modifiers, repeat| (key, modifiers, repeat, true);
    let release = |key| (key, Modifiers::NONE, false, false);
    let scripts: [(&str, Vec<Step>); 7] = [
        (
            "held motion",
            vec![
                press(Key::H, Modifiers::NONE, false),
                press(Key::H, Modifiers::NONE, true),
                press(Key::H, Modifiers::NONE, true),
                release(Key::H),
            ],
        ),
        (
            "counted held motion",
            vec![
                press(Key::Num3, Modifiers::NONE, false),
                press(Key::L, Modifiers::NONE, false),
                press(Key::L, Modifiers::NONE, true),
                release(Key::L),
            ],
        ),
        (
            "held motion then comma",
            vec![
                press(Key::H, Modifiers::NONE, false),
                press(Key::Comma, Modifiers::NONE, false),
                press(Key::H, Modifiers::NONE, true),
                release(Key::H),
                press(Key::H, Modifiers::NONE, false),
            ],
        ),
        (
            "held operator",
            vec![
                press(Key::D, Modifiers::NONE, false),
                press(Key::D, Modifiers::NONE, true),
                release(Key::D),
                press(Key::L, Modifiers::NONE, false),
            ],
        ),
        (
            "held motion into a mark",
            vec![
                press(Key::M, Modifiers::NONE, false),
                press(Key::H, Modifiers::NONE, true),
                press(Key::A, Modifiers::NONE, false),
            ],
        ),
        (
            "held beat after release",
            vec![
                press(Key::J, Modifiers::NONE, false),
                release(Key::J),
                press(Key::J, Modifiers::NONE, true),
            ],
        ),
        (
            "held undo",
            vec![
                press(Key::U, Modifiers::NONE, false),
                press(Key::U, Modifiers::NONE, true),
            ],
        ),
    ];
    for (name, script) in scripts {
        let mut bindings = Bindings::default();
        for (step, (key, modifiers, repeat, pressed)) in script.into_iter().enumerate() {
            let action = bindings.route_event(
                key,
                Some(key),
                modifiers,
                false,
                false,
                repeat,
                pressed,
                EditSelection::None,
            );
            writeln!(
                out,
                "held {name} {step} {key:?} repeat={repeat} pressed={pressed} -> {action:?} pending={:?}",
                bindings.pending()
            )
            .unwrap();
        }
    }
}
