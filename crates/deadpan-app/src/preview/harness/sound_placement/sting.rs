//! A sting attached to a beat with `:sound-attach`: its card badge and inspector
//! entry, `:sound-gain` on it, Split of its fragment, `:sound-detach`, then Repeat, Undo and a
//! whole-beat delete through the real keys: the sound follows its beat.

use super::*;

pub(super) fn run(d: &mut Driver<'_>, catalog_label: &str) -> Result<(), String> {
    d.command("sequence")?;
    d.settled()?;
    // Choose the first root beat long enough for the complete click.
    let (target, start) = {
        let document = document(d)?;
        let durations = document.durations().map_err(|error| error.to_string())?;
        let deadpan_core::NodeKind::Sequence { children } = &document.nodes()[document.root()].kind
        else {
            return Err("The sting replay needs a root Sequence".into());
        };
        let mut start = 0;
        let mut found = None;
        for child in children {
            let frames = durations[child].frames();
            if frames >= 8 && found.is_none() {
                found = Some((child.clone(), start));
            }
            start += frames;
        }
        found.ok_or("No root beat is long enough for the sting")?
    };
    d.chord(&[Key::G, Key::G])?;
    for digit in start.to_string().chars() {
        d.key(digit_key(digit)?)?;
    }
    if start > 0 {
        d.key(Key::L)?;
    }
    d.settled()?;
    d.check(
        "The Edit cursor selects the sting's beat at its start",
        d.app().sequence_cursor == u64::try_from(start).map_err(|e| e.to_string())?
            && d.app().selected_beat.as_ref() == Some(&target),
        json!({"cursor":start,"beat":target}),
        json!({"cursor":d.app().sequence_cursor,"beat":d.app().selected_beat}),
    )?;
    // Choosing the catalog sound keeps both cursors and the selected beat.
    d.click(catalog_label)?;
    d.step("Paint controls for the selected catalog sound", true)?;
    d.check(
        "Selecting the catalog sound retains the Edit cursor and beat",
        d.app().selected_sound.is_some()
            && d.app().sequence_cursor == u64::try_from(start).map_err(|e| e.to_string())?
            && d.app().selected_beat.as_ref() == Some(&target),
        json!({"cursor":start,"beat":target}),
        json!({"cursor":d.app().sequence_cursor,"beat":d.app().selected_beat}),
    )?;
    let nodes = document(d)?.nodes().clone();
    let sounds = document(d)?.sounds().clone();
    commit(d, "sound-attach")?;
    d.command("sequence")?;
    let attached = document(d)?.beat_sounds().clone();
    d.check(
        "Attaching authors one beat-owned sound without changing picture or placed sounds",
        attached.len() == 1
            && attached
                .get(&target)
                .is_some_and(|events| events.len() == 1)
            && document(d)?.nodes() == &nodes
            && document(d)?.sounds() == &sounds,
        json!({"owner":target,"sounds":1}),
        json!({"beat_sounds":attached}),
    )?;
    d.capture("A sting attached to the selected beat")?;
    inspect_split_and_detach(d, &target, start, &attached, &nodes)?;
    go_to(d, start)?;

    // Repeat the whole beat: its sting sounds in every play.
    let before = d.revision();
    d.chord(&[Key::R, Key::R])?;
    d.changed(&before)?;
    let repeated = document(d)?.clone();
    let wrapped = repeated.nodes().values().any(|node| {
        matches!(&node.kind, deadpan_core::NodeKind::Repeat { child, .. } if child == &target)
    });
    d.check(
        "Repeating the sting's beat keeps the sting on the repeated child",
        wrapped && repeated.beat_sounds() == &attached,
        json!({"repeated_child":target,"beat_sounds":attached}),
        json!({"wrapped":wrapped,"beat_sounds":repeated.beat_sounds()}),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "Undo restores the single unrepeated beat and its sting",
        document(d)?.beat_sounds() == &attached && document(d)?.nodes() == &nodes,
        json!(attached),
        json!(document(d)?.beat_sounds()),
    )?;

    // Deleting the whole beat deletes its sting; Undo restores both.
    d.chord(&[Key::G, Key::G])?;
    for digit in start.to_string().chars() {
        d.key(digit_key(digit)?)?;
    }
    if start > 0 {
        d.key(Key::L)?;
    }
    d.settled()?;
    let before = d.revision();
    d.chord(&[Key::D, Key::D])?;
    d.changed(&before)?;
    d.check(
        "Deleting the sting's beat removes its sound with it",
        document(d)?.beat_sounds().is_empty() && !document(d)?.nodes().contains_key(&target),
        json!({"beat_sounds":{}}),
        json!(document(d)?.beat_sounds()),
    )?;
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "Undo restores the beat and its sting",
        document(d)?.beat_sounds() == &attached && document(d)?.nodes() == &nodes,
        json!(attached),
        json!(document(d)?.beat_sounds()),
    )
}

/// `gg`, then a counted `l` to Edit frame `frame`.
fn go_to(d: &mut Driver<'_>, frame: i64) -> Result<(), String> {
    d.chord(&[Key::G, Key::G])?;
    for digit in frame.to_string().chars() {
        d.key(digit_key(digit)?)?;
    }
    if frame > 0 {
        d.key(Key::L)?;
    }
    d.settled()
}

/// The one visible accessible label starting with `prefix`.
fn label_with(d: &Driver<'_>, prefix: &str) -> Option<String> {
    let labels: Vec<String> = d
        .harness
        .root()
        .children_recursive()
        .filter_map(|node| node.accesskit_node().label())
        .filter(|label| label.starts_with(prefix))
        .collect();
    match labels.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// The card badge and inspector list show the sting; selecting it lets
/// `:sound-gain` apply; `s` splits the fragment that carries it (both pieces
/// keep the complete recipe); `:sound-detach` removes one piece's copy; Undo
/// restores each step back to the plain attachment.
fn inspect_split_and_detach(
    d: &mut Driver<'_>,
    target: &deadpan_core::NodeId,
    start: i64,
    attached: &std::collections::BTreeMap<
        deadpan_core::NodeId,
        std::collections::BTreeMap<SoundId, deadpan_core::BeatSound>,
    >,
    nodes: &std::collections::BTreeMap<deadpan_core::NodeId, deadpan_core::BeatNode>,
) -> Result<(), String> {
    let fragment = matches!(
        document(d)?.nodes()[target].kind,
        deadpan_core::NodeKind::Retime {
            purpose: deadpan_core::RetimePurpose::Partition,
            ..
        }
    );
    let (index, beat_label) = d
        .app()
        .beat_rows
        .iter()
        .enumerate()
        .find(|(_, beat)| &beat.id == target)
        .map(|(index, beat)| (index, beat.label.clone()))
        .ok_or("The sting's beat has no card")?;
    let spoken = label_with(d, &format!("Beat {}: {beat_label}, Fragment,", index + 1));
    d.check(
        "The fragment's card speaks its attached-sound count beside the painted badge",
        spoken
            .as_deref()
            .is_some_and(|label| label.ends_with(", 1 attached sound"))
            && fragment,
        json!({"label":"Beat N: …, 1 attached sound","fragment":true}),
        json!({"label":spoken,"fragment":fragment}),
    )?;
    let entry = label_with(d, "Attached sound 1 of 1: ");
    d.check(
        "The beat inspector lists the attached sound with its start and length",
        entry
            .as_ref()
            .is_some_and(|label| label.contains("Starts 0 f into the beat")),
        json!("Attached sound 1 of 1: … Starts 0 f into the beat …"),
        json!(entry),
    )?;
    d.click(&entry.ok_or("No inspector entry")?)?;
    d.settled()?;
    let id = attached[target]
        .keys()
        .next()
        .ok_or("The sting has no identity")?
        .clone();
    d.check(
        "Selecting the entry selects that attached sound",
        d.app().selected_attached == Some((target.clone(), id.clone())),
        json!([target, id]),
        json!(d.app().selected_attached),
    )?;
    d.capture("The beat inspector lists and selects the attached sound")?;
    commit(d, "sound-gain -6")?;
    let louder = document(d)?.beat_sounds()[target][&id].clone();
    d.check(
        ":sound-gain applies to the selected attached sound in place",
        louder.treatments.trim().millidecibels() == -6000
            && louder.mapping == attached[target][&id].mapping
            && document(d)?.nodes() == nodes,
        json!({"gain_mdb":-6000,"mapping":attached[target][&id].mapping}),
        json!(louder),
    )?;

    d.check(
        "An attached-sound edit keeps the inspector focused, not Placed sounds",
        d.app().pane == Pane::Inspector,
        json!("Inspector"),
        json!(format!("{:?}", d.app().pane)),
    )?;
    // Split the fragment inside the beat, after the sting's start.
    d.command("sequence")?;
    go_to(d, start + 6)?;
    let before = d.revision();
    d.key(Key::S)?;
    d.changed(&before)?;
    let split = document(d)?.beat_sounds().clone();
    d.check(
        "Splitting a fragment that carries a sting keeps the complete sting on both pieces",
        split.len() == 2
            && split.contains_key(target)
            && split
                .values()
                .all(|events| events.len() == 1 && events.get(&id) == Some(&louder)),
        json!({"owners":2,"event":louder}),
        json!(split),
    )?;
    d.capture("The split fragment keeps its sting")?;
    // The cursor sits on the right piece; one frame back is the left piece,
    // where the sting starts. Its only attached sound detaches.
    d.key(Key::H)?;
    d.settled()?;
    let before = d.revision();
    d.command("sound-detach")?;
    d.changed(&before)?;
    let remaining = document(d)?.beat_sounds().clone();
    d.check(
        ":sound-detach removes the left piece's sound only, as one edit",
        remaining.len() == 1 && !remaining.contains_key(target),
        json!({"owners":1,"removed":target}),
        json!(remaining),
    )?;
    d.capture("The sound detached from the left piece")?;
    let louder_only = {
        let mut map = attached.clone();
        map.insert(target.clone(), [(id.clone(), louder.clone())].into());
        map
    };
    for (label, expected, same_picture) in [
        ("Undo restores the detached sound", &split, false),
        (
            "Undo restores the unsplit fragment and its louder sting",
            &louder_only,
            true,
        ),
        ("Undo restores the sting's gain", attached, true),
    ] {
        let before = d.revision();
        d.key(Key::U)?;
        d.changed(&before)?;
        d.check(
            label,
            document(d)?.beat_sounds() == expected
                && (!same_picture || document(d)?.nodes() == nodes),
            json!(expected),
            json!(document(d)?.beat_sounds()),
        )?;
    }
    Ok(())
}

fn digit_key(digit: char) -> Result<Key, String> {
    Ok(match digit {
        '0' => Key::Num0,
        '1' => Key::Num1,
        '2' => Key::Num2,
        '3' => Key::Num3,
        '4' => Key::Num4,
        '5' => Key::Num5,
        '6' => Key::Num6,
        '7' => Key::Num7,
        '8' => Key::Num8,
        '9' => Key::Num9,
        _ => return Err(format!("not a digit: {digit}")),
    })
}
