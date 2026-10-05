//! Specification §8.2 "Delayed caption": `:caption` with a delay on a split
//! fragment, the caption drawn into the actual viewer target by the shared
//! GPU pass, the inspector row, `:caption clear`, Undo, and a caption
//! recorded in a macro and replayed on another beat.

use super::*;
use deadpan_core::{CaptionPlacement, NodeKind};
use egui::Key;
use std::sync::atomic::AtomicBool;

/// The captions on the selected beat's Source or Hold, with the offset of
/// the beat in that host's clock.
fn captions(d: &Driver<'_>) -> (Vec<deadpan_core::Caption>, i64) {
    let app = d.app();
    let (Some(workspace), Some(node)) = (&app.workspace, &app.selected_beat) else {
        return (Vec::new(), 0);
    };
    deadpan_core::cutaway_host(&workspace.document, node)
        .map_or((Vec::new(), 0), |(host, offset)| {
            (workspace.document.nodes()[&host].captions.clone(), offset)
        })
}

/// Linear working RGB of the top fifth of the visible viewer target, read
/// back from the picture the viewer actually submitted.
fn top_band(d: &mut Driver<'_>) -> Result<Vec<[f32; 3]>, String> {
    d.settled()?;
    let app = d.app_mut();
    let target = &app
        .target
        .as_ref()
        .ok_or("The viewer has no picture target")?
        .target;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut readback = loop {
        match app
            .renderer
            .begin_working_readback(target, &cancelled, deadline)
        {
            Ok(readback) => break readback,
            Err(deadpan_render::RenderError::ReadbackBusy) => std::thread::yield_now(),
            Err(error) => return Err(error.to_string()),
        }
    };
    app.render_state
        .device
        .poll(eframe::wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(5)),
        })
        .map_err(|error| error.to_string())?;
    let pixels = loop {
        if let Some(pixels) = readback
            .poll(&cancelled)
            .map_err(|error| error.to_string())?
        {
            break pixels;
        }
        if Instant::now() > deadline {
            return Err("Viewer readback timed out".into());
        }
        std::thread::yield_now();
    };
    // Odd viewer rasters have no encoder planes; sample the working texels.
    let stride = pixels.row_stride_bytes() as usize;
    let (width, height) = (pixels.width() as usize, pixels.height() as usize);
    let half = |bytes: &[u8]| {
        let bits = u16::from_le_bytes([bytes[0], bytes[1]]);
        let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
        let exponent = i32::from((bits >> 10) & 0x1f);
        let mantissa = f32::from(bits & 0x3ff);
        sign * if exponent == 0 {
            mantissa * 2f32.powi(-24)
        } else {
            (1.0 + mantissa / 1024.0) * 2f32.powi(exponent - 15)
        }
    };
    let mut band = Vec::with_capacity(width * (height / 5));
    for row in pixels.bytes().chunks_exact(stride).take(height / 5) {
        for texel in row[..width * 8].chunks_exact(8) {
            band.push([half(&texel[0..2]), half(&texel[2..4]), half(&texel[4..6])]);
        }
    }
    Ok(band)
}

/// Pixels that changed between two bands, and how many of those became
/// white fill or dark outline.
fn caption_ink(with: &[[f32; 3]], without: &[[f32; 3]]) -> (usize, usize, usize) {
    let mut changed = 0;
    let mut white = 0;
    let mut dark = 0;
    for (after, before) in with.iter().zip(without) {
        if after.iter().zip(before).any(|(a, b)| (a - b).abs() > 0.25) {
            changed += 1;
            if after.iter().all(|channel| *channel > 0.9) {
                white += 1;
            }
            if after.iter().all(|channel| *channel < 0.05) {
                dark += 1;
            }
        }
    }
    (changed, white, dark)
}

fn undo(d: &mut Driver<'_>, label: &str, count: usize) -> Result<(), String> {
    let applied = d.revision();
    d.key(Key::U)?;
    d.changed(&applied)?;
    d.settled()?;
    let shown = captions(d).0.len();
    d.check(label, shown == count, json!(count), json!(shown))
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("sequence")?;
    super::transcript::focus_your_edit(d)?;
    // Split out the fragment over Edit [30, 42).
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L, Key::S])?;
    d.settled()?;
    d.chord(&[Key::Num1, Key::Num2, Key::L, Key::S])?;
    d.settled()?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    d.settled()?;
    let duration = d.app().sequence_length();
    let plain = top_band(d)?;

    let before = d.revision();
    d.command("caption Are we done? at=top delay=4f")?;
    d.changed(&before)?;
    d.settled()?;
    let (placed, offset) = captions(d);
    d.check(
        ":caption with delay=4f captions the fragment from its fifth frame to its end, in its Source's clock, with timing unchanged",
        placed.len() == 1
            && placed[0].text == "Are we done?"
            && placed[0].placement == CaptionPlacement::Top
            && placed[0].range.start().0 - offset == 4
            && placed[0].range.end().0 - offset == 12
            && d.app().sequence_length() == duration,
        json!({"text":"Are we done?","placement":"top","beat_range":[4,12],"frames":duration}),
        json!({"captions":placed.iter().map(|caption| json!({"text":caption.text,"range":[caption.range.start().0 - offset, caption.range.end().0 - offset]})).collect::<Vec<_>>(),"frames":d.app().sequence_length(),"error":d.app().error}),
    )?;
    d.step("Paint the caption row in the inspector", true)?;
    let row = scenarios::text_paint_visibility(d, "“Are we done?”");
    d.check(
        "The inspector lists the caption",
        !row.is_empty(),
        json!("“Are we done?”"),
        json!(row),
    )?;
    let delayed = top_band(d)?;
    d.check(
        "Before its delay the caption is not drawn",
        caption_ink(&delayed, &plain).0 == 0,
        json!(0),
        json!(caption_ink(&delayed, &plain).0),
    )?;
    d.chord(&[Key::Num6, Key::L])?;
    d.settled()?;
    let shown = top_band(d)?;
    d.capture("Delayed caption in the viewer")?;

    // :caption clear on the beat removes it; Undo restores it, Undo again
    // removes the placement.
    let before = d.revision();
    d.command("caption clear")?;
    d.changed(&before)?;
    d.settled()?;
    d.check(
        ":caption clear removes the caption",
        captions(d).0.is_empty(),
        json!(0),
        json!(captions(d).0.len()),
    )?;
    // The same frame without the caption: the difference is the caption's
    // white fill and dark outline, drawn by the shared pass in the viewer.
    let cleared = top_band(d)?;
    let (changed, white, dark) = caption_ink(&shown, &cleared);
    d.check(
        "After its delay the viewer target draws white caption text with a dark outline in the top band",
        changed > 100 && white > 20 && dark > 20,
        json!({"changed":"> 100","white":"> 20","dark":"> 20"}),
        json!({"changed":changed,"white":white,"dark":dark}),
    )?;
    undo(d, "Undo restores the cleared caption", 1)?;
    undo(d, "Undo removes the placed caption", 0)?;

    // Record a caption in macro c and replay it on the next beat.
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    d.chord(&[Key::Q, Key::C])?;
    let before = d.revision();
    d.command("caption \"Again?\" reveal=2")?;
    d.changed(&before)?;
    d.settled()?;
    d.key(Key::Q)?;
    d.wait_for("Macro c saved", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.macros.recording()
    })?;
    d.chord(&[Key::G, Key::G, Key::Num4, Key::Num2, Key::L])?;
    d.settled()?;
    let before = d.revision();
    let shifted = |key, pressed| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    };
    d.events(
        "Run macro c with native @ text",
        vec![
            shifted(Key::Num2, true),
            egui::Event::Text("@".into()),
            shifted(Key::Num2, false),
        ],
    )?;
    d.key(Key::C)?;
    d.changed(&before)?;
    d.settled()?;
    let (replayed, offset) = captions(d);
    let hosts = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project")?
        .document
        .nodes()
        .values()
        .filter(|node| {
            matches!(node.kind, NodeKind::Source { .. })
                && node.captions.iter().any(|caption| {
                    caption.text == "Again?" && caption.reveal.map(|play| play.get()) == Some(2)
                })
        })
        .count();
    d.check(
        "A recorded caption replays on the beat at the new cursor; the recorded beat keeps its own",
        replayed
            .iter()
            .any(|caption| caption.text == "Again?" && caption.range.start().0 == offset)
            && replayed.len() == 1
            && hosts == 2
            && d.app().sequence_length() == duration,
        json!({"captions_on_new_beat":1,"captioned_sources":2,"frames":duration}),
        json!({"captions":replayed.iter().map(|caption| [caption.range.start().0, caption.range.end().0]).collect::<Vec<_>>(),"offset":offset,"captioned_sources":hosts,"frames":d.app().sequence_length(),"error":d.app().error,"message":d.app().message}),
    )?;
    Ok(())
}
