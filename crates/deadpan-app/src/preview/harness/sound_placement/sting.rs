//! A sting attached to a beat with `:sound-attach`, then Repeat, Undo and a
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

    // Earlier pauses cut the Original into Partition fragments; refining a
    // fragment that carries a sting still refuses (core tests cover Split of
    // whole beats). Repeat the whole beat: its sting sounds in every play.
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
