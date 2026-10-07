//! DP-05 keyboard layouts and composition through the production router.
//!
//! Every press is delivered as egui-winit 0.36 delivers it on macOS: the
//! logical key, or the physical position when egui has no key for the
//! layout's character, its `physical_key`, and the immediate companion
//! `Text`. German QWERTZ and French AZERTY drive motions, counts, Visual copy,
//! marks, registers, Undo and commands; Russian ЙЦУКЕН letters reach the Vim
//! keys at their physical positions. IME composition then fills the command
//! line (including `:caption` text), the transcript word field and the YouTube
//! URL field. egui-winit 0.36 never emits `ImeEvent::Enabled`/`Disabled`
//! (both are deprecated in egui 0.36), so composition is Preedit then Commit,
//! with winit's empty Preedit immediately before each Commit.

use super::*;
use crate::preview::corrections::Item;
use deadpan_analysis::picture_at;
use deadpan_core::{NodeKind, ProjectDocument};
use egui::{Event, ImeEvent, Key, Modifiers};

#[derive(Clone, Copy, Debug)]
enum Layout {
    Qwertz,
    Azerty,
    Russian,
}

/// One macOS key press: delivered key, physical position, modifiers and its
/// immediate companion text.
#[derive(Clone, Debug)]
struct Press {
    key: Key,
    physical: Key,
    modifiers: Modifiers,
    text: String,
}

fn press(key: Key, physical: Key, modifiers: Modifiers, text: impl Into<String>) -> Press {
    Press {
        key,
        physical,
        modifiers,
        text: text.into(),
    }
}

fn letter(c: char) -> Key {
    Key::from_name(&c.to_ascii_lowercase().to_string()).expect("ASCII letter")
}

fn digit(c: char) -> Key {
    Key::from_name(&c.to_string()).expect("ASCII digit")
}

impl Layout {
    fn name(self) -> &'static str {
        match self {
            Self::Qwertz => "German QWERTZ",
            Self::Azerty => "French AZERTY",
            Self::Russian => "Russian ЙЦУКЕН",
        }
    }

    /// The press that produces `c` on this macOS layout.
    fn press(self, c: char) -> Press {
        let shift = if c.is_ascii_uppercase() {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        match (self, c) {
            (_, ' ') => press(Key::Space, Key::Space, Modifiers::NONE, " "),
            (Self::Qwertz, 'y' | 'Y') => press(Key::Y, Key::Z, shift, c),
            (Self::Qwertz, 'z' | 'Z') => press(Key::Z, Key::Y, shift, c),
            (Self::Qwertz, '0'..='9') => press(digit(c), digit(c), Modifiers::NONE, c),
            (Self::Qwertz, ':') => press(Key::Colon, Key::Period, Modifiers::SHIFT, ":"),
            // egui names none of these characters: egui-winit reports the
            // physical position, and only the text says what was typed.
            (Self::Qwertz, 'ü') => press(Key::OpenBracket, Key::OpenBracket, Modifiers::NONE, "ü"),
            (Self::Qwertz, 'ö') => press(Key::Semicolon, Key::Semicolon, Modifiers::NONE, "ö"),
            (Self::Qwertz, 'ä') => press(Key::Quote, Key::Quote, Modifiers::NONE, "ä"),
            (Self::Qwertz, 'ß') => press(Key::Minus, Key::Minus, Modifiers::NONE, "ß"),
            (Self::Qwertz, '"') => press(Key::Num2, Key::Num2, Modifiers::SHIFT, "\""),
            (Self::Qwertz, '\'') => press(Key::Quote, Key::Backslash, Modifiers::SHIFT, "'"),
            (Self::Qwertz, '@') => press(Key::L, Key::L, Modifiers::ALT, "@"),
            (Self::Qwertz, '[') => press(Key::OpenBracket, Key::Num5, Modifiers::ALT, "["),
            (Self::Qwertz, ']') => press(Key::CloseBracket, Key::Num6, Modifiers::ALT, "]"),
            (Self::Azerty, 'a' | 'A') => press(Key::A, Key::Q, shift, c),
            (Self::Azerty, 'q' | 'Q') => press(Key::Q, Key::A, shift, c),
            (Self::Azerty, 'w' | 'W') => press(Key::W, Key::Z, shift, c),
            (Self::Azerty, 'z' | 'Z') => press(Key::Z, Key::W, shift, c),
            (Self::Azerty, 'm' | 'M') => press(Key::M, Key::Semicolon, shift, c),
            // AZERTY digits need Shift; the unshifted row is symbols.
            (Self::Azerty, '0'..='9') => press(digit(c), digit(c), Modifiers::SHIFT, c),
            (Self::Azerty, '&') => press(Key::Num1, Key::Num1, Modifiers::NONE, "&"),
            (Self::Azerty, 'é') => press(Key::Num2, Key::Num2, Modifiers::NONE, "é"),
            (Self::Azerty, '"') => press(Key::Num3, Key::Num3, Modifiers::NONE, "\""),
            (Self::Azerty, '\'') => press(Key::Quote, Key::Num4, Modifiers::NONE, "'"),
            (Self::Azerty, '(') => press(Key::Num5, Key::Num5, Modifiers::NONE, "("),
            (Self::Azerty, ':') => press(Key::Colon, Key::Period, Modifiers::NONE, ":"),
            (Self::Azerty, '@') => press(Key::Backtick, Key::Backtick, Modifiers::NONE, "@"),
            (Self::Azerty, '[') => press(
                Key::OpenBracket,
                Key::Num5,
                Modifiers::ALT | Modifiers::SHIFT,
                "[",
            ),
            (Self::Russian, '0'..='9') => press(digit(c), digit(c), Modifiers::NONE, c),
            // Shift+3 types №, which egui cannot name: the Num3 fallback.
            (Self::Russian, '№') => press(Key::Num3, Key::Num3, Modifiers::SHIFT, "№"),
            // Cyrillic letters have no egui key; egui-winit delivers the
            // physical position as the key.
            (Self::Russian, c) if russian_position(c).is_some() => {
                let position = russian_position(c).expect("checked position");
                let shift = if c.is_uppercase() {
                    Modifiers::SHIFT
                } else {
                    Modifiers::NONE
                };
                press(position, position, shift, c)
            }
            (Self::Azerty, ']') => press(
                Key::CloseBracket,
                Key::Minus,
                Modifiers::ALT | Modifiers::SHIFT,
                "]",
            ),
            (_, c) if c.is_ascii_alphabetic() => press(letter(c), letter(c), shift, c),
            _ => panic!("{} has no scripted press for {c:?}", self.name()),
        }
    }

    fn events(self, text: &str) -> Vec<Event> {
        text.chars().flat_map(|c| events(self.press(c))).collect()
    }
}

/// Russian ЙЦУКЕН letters at the Vim positions this replay types.
fn russian_position(c: char) -> Option<Key> {
    Some(match c.to_lowercase().next()? {
        'р' => Key::H,
        'о' => Key::J,
        'л' => Key::K,
        'д' => Key::L,
        'п' => Key::G,
        'ц' => Key::W,
        'и' => Key::B,
        _ => return None,
    })
}

fn events(press: Press) -> Vec<Event> {
    let mut events = vec![Event::Key {
        key: press.key,
        physical_key: Some(press.physical),
        pressed: true,
        repeat: false,
        modifiers: press.modifiers,
    }];
    events.push(Event::Text(press.text));
    events.push(Event::Key {
        key: press.key,
        physical_key: Some(press.physical),
        pressed: false,
        repeat: false,
        modifiers: press.modifiers,
    });
    events
}

fn plain(key: Key) -> Vec<Event> {
    vec![
        key_event(key, Modifiers::NONE, true),
        key_event(key, Modifiers::NONE, false),
    ]
}

fn preedit(text: &str) -> Event {
    Event::Ime(ImeEvent::Preedit {
        text: text.into(),
        active_range_chars: (!text.is_empty()).then(|| 0..text.chars().count()),
    })
}

/// winit on macOS clears the marked text, then commits.
fn commit(text: &str) -> Vec<Event> {
    vec![preedit(""), Event::Ime(ImeEvent::Commit(text.into()))]
}

struct Fixture {
    length: u64,
    words: [u64; 2],
    pauses: Vec<u64>,
    baseline: ProjectDocument,
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let fixture = prepare(d)?;
    qwertz(d, &fixture)?;
    azerty(d, &fixture)?;
    russian(d, &fixture)?;
    command_composition(d, &fixture)?;
    correction_composition(d)?;
    youtube_composition(d)?;
    d.report.skipped.push("Layouts are replayed as the pinned egui-winit 0.36 macOS translation delivers them (logical key, physical fallback for characters egui cannot name, immediate Text companion). Physical keyboards, the macOS input-source switcher and a real Japanese IME are not driven; dead keys are documented, not replayed.".into());
    Ok(())
}

fn prepare(d: &mut Driver<'_>) -> Result<Fixture, String> {
    d.wait_for("Transcript status checked", |app| {
        app.transcription.status_name() != "unchecked"
    })?;
    let (key, transcript, index) = super::transcript::synthetic(d)?;
    let session = d.app().workspace.as_ref().ok_or("No project")?.session;
    d.app_mut().submit(ProjectRequest::SaveTranscript {
        expected_session: session,
        attempt: 0,
        key,
        transcript: Arc::new(transcript.clone()),
    });
    d.wait_for("Transcript saved", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.transcript.is_some())
    })?;
    super::corrections::save_activity(d)?;
    super::transcript::focus_your_edit(d)?;
    d.settled()?;
    let word = |word: usize| -> Result<u64, String> {
        let seconds = transcript
            .seconds(transcript.words()[word].start_cs)
            .map_err(|e| e.to_string())?;
        picture_at(&index, seconds)
            .map(|frame| frame as u64)
            .ok_or_else(|| "synthetic word lies outside the picture".to_owned())
    };
    let pauses = d
        .app_mut()
        .edit_analysis()?
        .pauses()
        .ok_or("pauses are not projected")?
        .iter()
        .map(|pause| pause.start().0 as u64)
        .collect::<Vec<_>>();
    let fixture = Fixture {
        length: d.app().sequence_length(),
        words: [word(0)?, word(1)?],
        pauses,
        baseline: document(d)?.clone(),
    };
    d.check(
        "Layout replay starts from one Original beat with two words and two pauses",
        fixture.length == 120
            && d.app().beat_rows.len() == 1
            && fixture.pauses.len() == 2
            && fixture.words[0] < fixture.words[1],
        json!({"frames":120,"beats":1,"pauses":2}),
        json!({"frames":fixture.length,"beats":d.app().beat_rows.len(),"pauses":fixture.pauses,"words":fixture.words}),
    )?;
    Ok(fixture)
}

fn typed(d: &mut Driver<'_>, layout: Layout, text: &str) -> Result<(), String> {
    d.events(
        &format!("{} types {text:?}", layout.name()),
        layout.events(text),
    )
}

fn settle_typed(d: &mut Driver<'_>, layout: Layout, text: &str) -> Result<(), String> {
    typed(d, layout, text)?;
    d.settled()
}

fn cursor(d: &mut Driver<'_>, name: &str, expected: u64) -> Result<(), String> {
    d.check(
        name,
        d.app().sequence_cursor == expected && d.app().bindings.pending().is_empty(),
        json!({"Edit":expected,"pending":""}),
        json!({"Edit":d.app().sequence_cursor,"pending":d.app().bindings.pending(),"message":d.app().message,"error":d.app().error}),
    )
}

/// A press that egui-winit can only report by position must neither act nor
/// leave a prefix, and must not author anything.
fn inert(d: &mut Driver<'_>, layout: Layout, c: char, why: &str) -> Result<(), String> {
    let revision = d.revision();
    let at = d.app().sequence_cursor;
    let copied = d.app().copied.content().map(copied::Content::label);
    typed(d, layout, &c.to_string())?;
    d.settled()?;
    let press = layout.press(c);
    d.check(
        &format!("{} {c} is inert in the editor: {why}", layout.name()),
        d.app().bindings.pending().is_empty()
            && d.app().sequence_cursor == at
            && d.revision() == revision
            && d.app().copied.content().map(copied::Content::label) == copied
            && !d.app().command_open
            && !d.app().help_open,
        json!({"pending":"","Edit":at,"revision":revision}),
        json!({"delivered":format!("{press:?}"),"pending":d.app().bindings.pending(),"Edit":d.app().sequence_cursor,"revision":d.revision(),"message":d.app().message,"error":d.app().error}),
    )
}

/// A Kestrel-reserved layout character leaves a visible teaching message.
fn taught(d: &mut Driver<'_>, what: &str, phrases: &[&str]) -> Result<(), String> {
    let message = d.app().message.clone().unwrap_or_default();
    let visible = scenarios::text_paint_visibility(d, "Kestrel reserves");
    d.check(
        &format!("{what} shows why it is ignored and what to use instead"),
        phrases.iter().all(|phrase| message.contains(phrase))
            && !visible.is_empty()
            && visible.iter().all(|paint| paint["fully_visible"] == true),
        json!({"message contains":phrases,"painted":true}),
        json!({"message":message,"paints":visible}),
    )
}

fn prefix(d: &mut Driver<'_>, layout: Layout, c: char, expected: &str) -> Result<(), String> {
    let at = d.app().sequence_cursor;
    typed(d, layout, &c.to_string())?;
    d.check(
        &format!(
            "{} {c} enters the {expected:?} prefix, not a count or another key",
            layout.name()
        ),
        d.app().bindings.pending() == expected && d.app().sequence_cursor == at,
        json!({"pending":expected,"Edit":at}),
        json!({"pending":d.app().bindings.pending(),"Edit":d.app().sequence_cursor,"message":d.app().message}),
    )
}

fn escape(d: &mut Driver<'_>) -> Result<(), String> {
    d.events("Escape", plain(Key::Escape))?;
    d.settled()
}

fn visual_copy(d: &mut Driver<'_>, layout: Layout, start: u64, frames: u64) -> Result<(), String> {
    settle_typed(d, layout, &format!("gg{start}l"))?;
    let revision = d.revision();
    typed(d, layout, &format!("v{frames}l"))?;
    let expected_range = [start as i64, (start + frames) as i64];
    d.check(
        &format!(
            "{} v plus a counted motion selects exactly the frames",
            layout.name()
        ),
        d.app()
            .selected_edit_range()
            .map(|range| [range.start().0, range.end().0])
            == Some(expected_range),
        json!(expected_range),
        json!(
            d.app()
                .selected_edit_range()
                .map(|range| [range.start().0, range.end().0])
        ),
    )?;
    typed(d, layout, "y")?;
    d.wait_for("Visual copy stored", |app| !app.copied.is_pending())?;
    d.settled()?;
    let label = format!(
        "Copied Edit [{start}..{}) · {frames} frames",
        start + frames
    );
    d.check(
        &format!(
            "{} logical y (physical {:?}) copies the Visual range without an edit",
            layout.name(),
            layout.press('y').physical
        ),
        d.app().copied.content().map(copied::Content::label) == Some(label.clone())
            && d.revision() == revision,
        json!({"copied":label,"revision":revision}),
        json!({"copied":d.app().copied.content().map(copied::Content::label),"revision":d.revision(),"error":d.app().error}),
    )?;
    escape(d)
}

fn qwertz(d: &mut Driver<'_>, fixture: &Fixture) -> Result<(), String> {
    let layout = Layout::Qwertz;
    settle_typed(d, layout, "gg3l")?;
    cursor(d, "QWERTZ gg then 3l lands on frame 3", 3)?;
    settle_typed(d, layout, "h")?;
    cursor(d, "QWERTZ h moves back one frame", 2)?;
    settle_typed(d, layout, "G")?;
    cursor(
        d,
        "QWERTZ Shift+G reaches the end of the edit",
        fixture.length,
    )?;
    settle_typed(d, layout, "ggw")?;
    let first = d.app().sequence_cursor;
    settle_typed(d, layout, "w")?;
    let second = d.app().sequence_cursor;
    settle_typed(d, layout, "b")?;
    d.check(
        "QWERTZ w, w, b land on the transcript's word starts",
        first == fixture.words[0]
            && second == fixture.words[1]
            && d.app().sequence_cursor == fixture.words[0],
        json!({"w":fixture.words[0],"ww":fixture.words[1],"b":fixture.words[0]}),
        json!({"w":first,"ww":second,"b":d.app().sequence_cursor,"message":d.app().message}),
    )?;
    settle_typed(d, layout, "gg]p")?;
    cursor(
        d,
        "QWERTZ Option+6 `]` then p reaches the first pause",
        fixture.pauses[0],
    )?;
    d.capture("QWERTZ ]p at the first pause")?;
    inert(
        d,
        layout,
        '[',
        "Kestrel owns Option+5, so ]p's partner [p is unavailable",
    )?;
    taught(
        d,
        "QWERTZ Option+5 [",
        &[":scope play N", "[p and [s have no command"],
    )?;
    d.capture("QWERTZ [ reserved by Kestrel teaches its alternatives")?;
    inert(
        d,
        layout,
        '@',
        "Kestrel owns Option+L, so @ cannot run macros",
    )?;
    taught(d, "QWERTZ Option+L @", &[":macro a"])?;
    inert(d, layout, 'ü', "the OpenBracket position is not [")?;
    inert(d, layout, 'ö', "the Semicolon position is not ;")?;
    inert(d, layout, 'ä', "the Quote position is not a mark jump")?;
    inert(d, layout, 'ß', "the Minus position is not gain down")?;
    inert(
        d,
        layout,
        'z',
        "logical z (physical Y) is unbound and never yanks",
    )?;
    prefix(d, layout, '"', "\"")?;
    d.capture("QWERTZ Shift+2 teaches the register prefix")?;
    escape(d)?;
    prefix(d, layout, '\'', "'")?;
    escape(d)?;
    visual_copy(d, layout, 20, 3)?;

    settle_typed(d, layout, "gg30l")?;
    let before = d.revision();
    typed(d, layout, ":")?;
    d.check(
        "QWERTZ Shift+Period opens Command without echoing its colon",
        d.app().command_open && d.app().command.is_empty(),
        json!({"open":true,"command":""}),
        json!({"open":d.app().command_open,"command":d.app().command}),
    )?;
    typed(d, layout, "split")?;
    d.check(
        "QWERTZ letters reach the command field, not editor bindings",
        d.app().command == "split" && d.revision() == before && d.app().sequence_cursor == 30,
        json!("split"),
        json!({"command":d.app().command,"Edit":d.app().sequence_cursor}),
    )?;
    d.events("Submit the QWERTZ command", plain(Key::Enter))?;
    d.changed(&before)?;
    let rows = d.app().beat_rows.clone();
    d.check(
        "QWERTZ :split cuts at frame 30 and selects the right fragment",
        rows.len() == 2
            && rows[0].start == 0
            && rows[0].frames == 30
            && rows[1].start == 30
            && rows[1].frames == fixture.length - 30
            && d.app().selected_beat.as_ref() == Some(&rows[1].id)
            && d.app().sequence_length() == fixture.length,
        json!({"beats":[[0,30],[30,fixture.length - 30]],"selected":"right"}),
        json!({"beats":rows.iter().map(|row| [row.start,row.frames]).collect::<Vec<_>>(),"selected":d.app().selected_beat,"frames":d.app().sequence_length()}),
    )?;
    d.capture("QWERTZ :split result")?;
    settle_typed(d, layout, "k")?;
    let left = (d.app().selected_beat.clone(), d.app().sequence_cursor);
    settle_typed(d, layout, "j")?;
    d.check(
        "QWERTZ k and j select the left then right beat at their starts",
        left == (Some(rows[0].id.clone()), 0)
            && d.app().selected_beat.as_ref() == Some(&rows[1].id)
            && d.app().sequence_cursor == 30,
        json!({"k":[rows[0].id,0],"j":[rows[1].id,30]}),
        json!({"k":left,"j":[d.app().selected_beat,d.app().sequence_cursor]}),
    )?;
    let saved = d.revision();
    typed(d, layout, "u")?;
    d.changed(&saved)?;
    d.check(
        "QWERTZ u restores the unsplit Original exactly",
        same_document(document(d)?, &fixture.baseline)?
            && !d.app().workspace.as_ref().is_some_and(|w| w.can_undo),
        json!("baseline document with a fresh revision"),
        json!({"beats":d.app().beat_rows.len(),"revision":d.revision()}),
    )
}

fn azerty(d: &mut Driver<'_>, fixture: &Fixture) -> Result<(), String> {
    let layout = Layout::Azerty;
    settle_typed(d, layout, "gg3l")?;
    cursor(d, "AZERTY Shift+3 is a count: gg3l lands on frame 3", 3)?;
    settle_typed(d, layout, "él")?;
    cursor(d, "AZERTY unshifted é (Num2 position) is not a count", 4)?;
    settle_typed(d, layout, "(h")?;
    cursor(d, "AZERTY unshifted ( (Num5 position) is not a count", 3)?;
    inert(
        d,
        layout,
        '&',
        "the Num1 position without Shift is not a count",
    )?;
    inert(
        d,
        layout,
        '[',
        "Kestrel owns Shift+Option+5, so [r, [p and [s are unavailable",
    )?;
    taught(
        d,
        "AZERTY Shift+Option+5 [",
        &["Shift+Option+5 types [", ":scope all"],
    )?;
    prefix(d, layout, '"', "\"")?;
    escape(d)?;
    settle_typed(d, layout, "ggw")?;
    cursor(
        d,
        "AZERTY w (physical Z) moves to the first word",
        fixture.words[0],
    )?;
    inert(d, layout, 'z', "logical z (physical W) is unbound")?;
    prefix(d, layout, 'q', "q")?;
    escape(d)?;
    prefix(d, layout, '@', "@")?;
    escape(d)?;
    settle_typed(d, layout, "gg]p")?;
    cursor(
        d,
        "AZERTY Shift+Option `]` then p reaches the first pause",
        fixture.pauses[0],
    )?;

    settle_typed(d, layout, "gg45l")?;
    cursor(d, "AZERTY Shift digits 45l", 45)?;
    let before = d.revision();
    typed(d, layout, "ma")?;
    d.changed(&before)?;
    settle_typed(d, layout, "G")?;
    cursor(d, "AZERTY Shift+G reaches the end", fixture.length)?;
    typed(d, layout, "'a")?;
    d.wait_for("Mark navigation completes", |app| {
        !app.service.is_busy() && !app.marks.is_pending()
    })?;
    d.settled()?;
    cursor(
        d,
        "AZERTY m (physical Semicolon) a (physical Q) then ' (Num4 position) a returns to the mark",
        45,
    )?;

    let before = d.revision();
    typed(d, layout, ":")?;
    typed(d, layout, "hold 12f")?;
    d.check(
        "AZERTY unshifted colon opens Command and Shift digits type into it",
        d.app().command_open && d.app().command == "hold 12f" && d.revision() == before,
        json!("hold 12f"),
        json!({"open":d.app().command_open,"command":d.app().command}),
    )?;
    d.capture("AZERTY command text")?;
    d.events("Submit the AZERTY command", plain(Key::Enter))?;
    d.changed(&before)?;
    let holds = holds(d)?;
    d.check(
        "AZERTY :hold 12f inserts exactly one 12-frame Hold",
        holds == [12] && d.app().sequence_length() == fixture.length + 12,
        json!({"holds":[12],"frames":fixture.length + 12}),
        json!({"holds":holds,"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    let saved = d.revision();
    typed(d, layout, "u")?;
    d.changed(&saved)?;
    let holds = self::holds(d)?;
    d.check(
        "AZERTY u removes the Hold",
        holds.is_empty() && d.app().sequence_length() == fixture.length,
        json!({"holds":[],"frames":fixture.length}),
        json!({"holds":holds,"frames":d.app().sequence_length()}),
    )?;
    visual_copy(d, layout, 10, 2)
}

/// Cyrillic letters have no egui key, so egui-winit delivers their physical
/// position. Before this fallback was restored, logical routing dropped them.
fn russian(d: &mut Driver<'_>, fixture: &Fixture) -> Result<(), String> {
    let layout = Layout::Russian;
    let revision = d.revision();
    settle_typed(d, layout, "пп3д")?;
    cursor(
        d,
        "Russian пп then 3д (g g 3 l positions) lands on frame 3",
        3,
    )?;
    settle_typed(d, layout, "р")?;
    cursor(d, "Russian р (H position) moves back one frame", 2)?;
    settle_typed(d, layout, "П")?;
    cursor(
        d,
        "Russian Shift+п (G position) reaches the end of the edit",
        fixture.length,
    )?;
    settle_typed(d, layout, "ппц")?;
    cursor(
        d,
        "Russian ц (W position) moves to the first word",
        fixture.words[0],
    )?;
    d.capture("Russian ЙЦУКЕН motions at their physical positions")?;
    inert(d, layout, '№', "Shift+3 types №, which is not a count")?;
    d.check(
        "Russian motions author nothing",
        d.revision() == revision,
        json!(revision),
        json!(d.revision()),
    )
}

/// `:caption` text is the command line: German fallback keys and a committed
/// Japanese composition both stay text.
fn command_composition(d: &mut Driver<'_>, fixture: &Fixture) -> Result<(), String> {
    let layout = Layout::Qwertz;
    settle_typed(d, layout, "gg")?;
    let revision = d.revision();
    let beat = d.app().selected_beat.clone();
    typed(d, layout, ":")?;
    typed(d, layout, "caption Grüße ")?;
    d.check(
        "QWERTZ ü and ß fallbacks are text inside Command",
        d.app().command == "caption Grüße "
            && d.app().bindings.pending().is_empty()
            && d.revision() == revision,
        json!("caption Grüße "),
        json!({"command":d.app().command,"pending":d.app().bindings.pending()}),
    )?;
    composition(d, "Command", "caption Grüße ", |d| {
        d.app().command_open.then(|| d.app().command.clone())
    })?;
    d.events("Submit the composed caption", plain(Key::Enter))?;
    d.changed(&revision)?;
    let captions = captions(d)?;
    d.check(
        ":caption stores the composed text exactly on the selected beat",
        captions == ["Grüße 日本語"]
            && !d.app().command_open
            && d.app().selected_beat == beat
            && d.app().sequence_length() == fixture.length,
        json!(["Grüße 日本語"]),
        json!({"captions":captions,"open":d.app().command_open,"error":d.app().error}),
    )?;
    d.capture("Composed CJK caption on the picture")?;
    d.report.skipped.push("The picture caption rasterizer uses the bundled Inter font, which has no CJK glyphs: 日本語 draws .notdef boxes in the viewer (and render) while the stored caption, inspector and status text are exact. This is the documented deadpan-render caption limit, not an input defect.".into());
    let saved = d.revision();
    typed(d, layout, "u")?;
    d.changed(&saved)?;
    let captions = self::captions(d)?;
    d.check(
        "u removes the composed caption",
        captions.is_empty(),
        json!([]),
        json!(captions),
    )
}

/// Preedit, then Enter and Escape during composition, then the winit commit.
/// `value` reads the field while it is open. Leaves the committed field open.
fn composition(
    d: &mut Driver<'_>,
    field: &str,
    prefix: &str,
    value: impl Fn(&Driver<'_>) -> Option<String>,
) -> Result<(), String> {
    let revision = d.revision();
    let at = d.app().sequence_cursor;
    // One native batch: the IME starts composing and the user presses
    // Escape before the app has seen the composition. The field must keep
    // focus; egui applies its focus filter before the app reads the batch.
    d.events(
        &format!("{field}: Japanese preedit and Escape in one batch"),
        std::iter::once(preedit("にほんご"))
            .chain(plain(Key::Escape))
            .collect(),
    )?;
    d.settled()?;
    let preedit_value = format!("{prefix}にほんご");
    d.check(
        &format!("{field} keeps focus and the preedit through a same-batch Escape and acts on nothing"),
        value(d).as_deref() == Some(preedit_value.as_str())
            && d.app().ime_composing
            && d.app().bindings.pending().is_empty()
            && d.revision() == revision
            && d.app().sequence_cursor == at,
        json!({"value":preedit_value,"composing":true}),
        json!({"value":value(d),"composing":d.app().ime_composing,"pending":d.app().bindings.pending()}),
    )?;
    d.capture(&format!("{field} with active Japanese preedit"))?;
    for key in [Key::Enter, Key::Escape] {
        d.events(&format!("{field}: {key:?} while composing"), plain(key))?;
        d.settled()?;
        d.check(
            &format!("{field}: {key:?} during composition neither submits nor cancels"),
            value(d).as_deref() == Some(preedit_value.as_str())
                && d.app().ime_composing
                && d.revision() == revision
                && d.app().sequence_cursor == at,
            json!({"value":preedit_value,"composing":true,"revision":revision}),
            json!({"value":value(d),"composing":d.app().ime_composing,"revision":d.revision(),"error":d.app().error}),
        )?;
    }
    d.events(
        &format!("{field}: commit 日本語 with its same-batch Enter"),
        commit("日本語")
            .into_iter()
            .chain(plain(Key::Enter))
            .collect(),
    )?;
    d.settled()?;
    let committed = format!("{prefix}日本語");
    d.check(
        &format!("{field} holds exactly the committed text; the commit batch's Enter is withheld"),
        value(d).as_deref() == Some(committed.as_str())
            && !d.app().ime_composing
            && d.revision() == revision,
        json!({"value":committed,"composing":false}),
        json!({"value":value(d),"composing":d.app().ime_composing,"revision":d.revision()}),
    )?;
    d.capture(&format!("{field} with committed 日本語"))
}

fn correction_composition(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.command("correct")?;
    d.settled()?;
    for _ in 0..8 {
        if d.app().correction_item() == Some(Item::Word(1)) {
            break;
        }
        // Words and pauses alternate in time order; only a later word is past it.
        let key = match d.app().correction_item() {
            Some(Item::Word(word)) if word > 1 => Key::H,
            _ => Key::L,
        };
        d.events("Select the second word", plain(key))?;
        d.settled()?;
    }
    // An unnamed Latin character at C must not open the editor. A real c
    // arrives with companion text, which opens the field without inserting c.
    d.events(
        "A Latin fallback at C cannot become a Corrections command",
        events(press(Key::C, Key::C, Modifiers::NONE, "ç")),
    )?;
    d.settled()?;
    d.check(
        "Corrections reads the produced character before selecting an action",
        focused_text(d).is_none(),
        json!(null),
        json!(focused_text(d)),
    )?;
    d.events(
        "Edit the word with its native text companion",
        Layout::Azerty.events("c"),
    )?;
    d.settled()?;
    d.check(
        "c opens the selected word's text field with focus",
        d.app().correction_item() == Some(Item::Word(1))
            && focused_text(d).as_deref() == Some("beta"),
        json!({"item":"Word(1)","value":"beta"}),
        json!({"item":format!("{:?}",d.app().correction_item()),"value":focused_text(d)}),
    )?;
    d.events(
        "Select the word text natively",
        vec![
            key_event(Key::A, Modifiers::COMMAND, true),
            key_event(Key::A, Modifiers::COMMAND, false),
        ],
    )?;
    composition(d, "Word text", "", focused_text)?;
    d.events("Apply the composed word", plain(Key::Enter))?;
    d.wait_for("Correction saved", |app| app.correction_settled())?;
    d.settled()?;
    let words = d
        .app()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.transcript.as_ref())
        .map(|transcript| {
            transcript
                .transcript
                .words()
                .iter()
                .map(|word| word.text.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    d.check(
        "Enter after the commit applies exactly the composed word without an edit revision",
        words == ["alpha", "日本語", "gamma"]
            && d.app().correction_error().is_none()
            && d.revision() == revision,
        json!({"words":["alpha","日本語","gamma"],"revision":revision}),
        json!({"words":words,"error":d.app().correction_error(),"revision":d.revision()}),
    )?;
    d.capture("Corrected word in the sheet")?;
    d.events("Close the correction sheet", plain(Key::Escape))?;
    d.settled()?;
    d.check(
        "Escape closes the correction sheet after composition",
        !d.app().correction_open(),
        json!(false),
        json!(d.app().correction_open()),
    )
}

fn youtube_composition(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.command("youtube")?;
    d.wait_for("YouTube URL field focused", |app| app.youtube.modal)?;
    for _ in 0..4 {
        d.step("YouTube URL field focus", false)?;
    }
    d.check(
        ":youtube opens its sheet with the URL field focused",
        d.app().youtube.modal
            && d.harness.ctx.memory(|memory| {
                memory.has_focus(egui::Id::new(crate::preview::youtube::URL_ID))
            }),
        json!({"modal":true,"focused":true}),
        json!({"modal":d.app().youtube.modal,"focused":d.harness.ctx.memory(|memory| memory.focused().map(|id| format!("{id:?}")))}),
    )?;
    composition(d, "YouTube URL", "", |d| Some(d.app().youtube.url.clone()))?;
    d.events("Submit the composed URL", plain(Key::Enter))?;
    d.settled()?;
    let refusal = scenarios::text_paint_visibility(d, "contains spaces or non-ASCII characters");
    d.check(
        "Enter submits the composed URL to the live check, which refuses it without a job",
        d.app().youtube.step_name() == "idle"
            && d.app().youtube.url == "日本語"
            && d.app().youtube.modal
            && !refusal.is_empty()
            && refusal.iter().all(|paint| paint["fully_visible"] == true)
            && d.revision() == revision,
        json!({"step":"idle","url":"日本語","modal":true,"refusal":"visible"}),
        json!({"step":d.app().youtube.step_name(),"url":d.app().youtube.url,"modal":d.app().youtube.modal,"refusal":refusal,"error":d.app().error}),
    )?;
    d.capture("YouTube URL refusal for composed text")?;
    d.events("Leave the YouTube sheet", plain(Key::Escape))?;
    d.settled()?;
    d.check(
        "Escape after composition leaves the YouTube sheet without starting work",
        !d.app().youtube.modal && d.app().youtube.step_name() == "idle",
        json!({"modal":false,"step":"idle"}),
        json!({"modal":d.app().youtube.modal,"step":d.app().youtube.step_name()}),
    )
}

fn focused_text(d: &Driver<'_>) -> Option<String> {
    d.harness
        .root()
        .children_recursive()
        .find(|node| {
            let access = node.accesskit_node();
            access.role() == egui::accesskit::Role::TextInput && access.is_focused()
        })
        .and_then(|node| node.accesskit_node().value().map(|value| value.to_string()))
}

fn holds(d: &Driver<'_>) -> Result<Vec<i64>, String> {
    Ok(document(d)?
        .nodes()
        .values()
        .filter_map(|node| match &node.kind {
            NodeKind::Hold { recipe } => Some(recipe.duration.frames()),
            _ => None,
        })
        .collect())
}

fn captions(d: &Driver<'_>) -> Result<Vec<String>, String> {
    Ok(document(d)?
        .nodes()
        .values()
        .flat_map(|node| node.captions.iter().map(|caption| caption.text.clone()))
        .collect())
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Layout replay has no document".into())
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual = serde_json::to_value(actual).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    Ok(actual == serde_json::to_value(expected).map_err(|error| error.to_string())?)
}
