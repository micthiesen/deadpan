//! Preview proxy seeking: a jump in Your edit shows the verified proxy picture
//! with a visible "Proxy" tier, then the exact Original picture replaces it
//! once the cursor rests.
//!
//! The replay Original (320×180, short GOP) seeks fast enough that the real
//! background job correctly decides it needs no proxy. The scenario therefore
//! verifies the committed proxy of this exact Original against it, as a
//! finished background build would, and publishes it through the project's
//! own proxy cache. Encoding is covered by `proxy_real_media` and `perf seek`.

use std::io::Write;

use super::*;
use egui::Key;

use crate::worker::PictureTier;

fn publish_fixture_proxy(d: &Driver<'_>) -> Result<(), String> {
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let asset = original_asset(&workspace).ok_or("No Original")?.clone();
    let registered = workspace.sources.get(&asset).ok_or("No Original source")?;
    let video = registered
        .receipt
        .snapshot()
        .video()
        .ok_or("No picture stream")?;
    let cancelled = AtomicBool::new(false);
    let mut snapshot = workspace
        .originals
        .snapshot_original(
            &registered.original,
            deadpan_store::original_media::OriginalMediaLimits::default(),
            &cancelled,
        )
        .map_err(|e| e.to_string())?;
    let input = deadpan_media::source_input::VerifiedSourceInput::copy_verified(
        &mut snapshot,
        video.index().content(),
        1 << 30,
        Duration::from_secs(60),
        &cancelled,
    )
    .map_err(|e| e.to_string())?;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proxy/cfr-bframes.proxy.mp4"),
    )
    .map_err(|e| e.to_string())?;
    let cache = d.app().proxies.cache.clone().ok_or("No proxy cache")?;
    let staging = cache.stage().map_err(|e| e.to_string())?;
    staging
        .movie()
        .write_all(&bytes)
        .map_err(|e| e.to_string())?;
    let sidecar = deadpan_media::proxy::verify_proxy(
        staging.movie(),
        &input,
        registered.original.object().content().digest(),
        Arc::new(video.index().clone()),
        video.interpretation(),
        deadpan_media::proxy::ProxyReason::Requested,
        deadpan_media::proxy::VerifyControl {
            timeout: Duration::from_secs(60),
            cancelled: &cancelled,
            pause: None,
        },
    )
    .map_err(|e| e.to_string())?;
    let key =
        deadpan_cli::proxy::proxy_key(&registered.original, video).map_err(|e| e.to_string())?;
    cache
        .publish(&key, staging, &sidecar)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn badge(d: &Driver<'_>) -> Vec<Value> {
    scenarios::text_paint_visibility(d, "Proxy")
        .into_iter()
        .filter(|paint| paint["text"] == "Proxy")
        .collect()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.settled()?;
    d.wait_for("The background proxy job has decided", |app| {
        matches!(app.proxies.state_name(), "not_needed" | "ready")
    })?;
    let decided = d.app().proxies.state_name();
    d.check(
        "A short-GOP 320×180 Original needs no proxy, and the job says so without an edit or error",
        decided == "not_needed" && d.app().proxies.detail().is_none(),
        json!("not_needed"),
        json!({"state": decided}),
    )?;
    // The setting turns automatic proxies off and on, without an edit.
    d.command("proxies off")?;
    d.wait_for("Automatic proxies off", |app| {
        app.proxies.state_name() == "disabled"
    })?;
    let off = d.app().message.clone();
    d.command("proxies on")?;
    d.wait_for("Automatic proxies on again", |app| {
        app.proxies.state_name() == "not_needed"
    })?;
    d.check(
        ":proxies off and :proxies on change the setting with a message and no edit",
        off.as_deref()
            .is_some_and(|message| message.contains("off"))
            && d.app().message.as_deref() == Some("Automatic seek proxies are on.")
            && !d
                .app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.can_undo),
        json!({"off":"…off…","on":"Automatic seek proxies are on."}),
        json!({"off":off,"on":d.app().message}),
    )?;
    publish_fixture_proxy(d)?;
    d.command("sequence")?;
    d.settled()?;
    let revision = d.revision();
    // Hold replies so each tier is observed on screen before the next.
    // The worker opens a published proxy while idle, after a request wanted
    // it; the first picture after publication may therefore be exact.
    // Alternate between the ends until a jump shows the proxy.
    let mut proxy = None;
    for attempt in 0..6 {
        d.app_mut().feedback.hold_preview = true;
        if attempt % 2 == 0 {
            d.key_modified(Key::G, egui::Modifiers::SHIFT)?;
        } else {
            d.chord(&[Key::G, Key::G])?;
        }
        d.wait_for("Jump picture decoded", |app| {
            app.feedback.held_reply.is_some()
        })?;
        let reply = d
            .app_mut()
            .feedback
            .held_reply
            .take()
            .ok_or("No jump reply")?;
        if attempt % 2 == 0
            && reply
                .picture
                .as_ref()
                .is_ok_and(|picture| picture.tier == PictureTier::Proxy)
        {
            proxy = Some(reply);
            break;
        }
        d.app_mut().feedback.hold_preview = false;
        d.app_mut().feedback.release_reply = Some(reply);
        d.settled()?;
        // A proxy picture at the start is refined before the next jump.
        d.wait_for("Any refinement finished", |app| {
            !app.presentation.refining() && !app.presentation.needs_render()
        })?;
        std::thread::sleep(Duration::from_millis(100));
        d.app_mut().feedback.held_reply = None;
    }
    let proxy = proxy.ok_or("No jump to the end showed the proxy picture")?;
    let proxy_tier = proxy.picture.as_ref().map(|picture| picture.tier).ok();
    let ticket = proxy.ticket;
    d.app_mut().feedback.release_reply = Some(proxy);
    d.wait_for("Proxy picture displayed", |app| {
        app.presentation.displayed_tier() == Some(PictureTier::Proxy)
            && !app.presentation.needs_render()
    })?;
    let last = d.app().sequence_length().saturating_sub(1);
    let painted = badge(d);
    let label = d.app().presentation.displayed_label();
    let camera_blocked = d
        .app()
        .presentation
        .stable_sequence_ticket(
            d.app().workspace.as_ref().ok_or("No project")?.session,
            d.app()
                .workspace
                .as_ref()
                .ok_or("No project")?
                .document
                .revision_id(),
            ProjectFrame(i64::try_from(last).map_err(|e| e.to_string())?),
        )
        .is_none();
    d.check(
        "A jump shows the proxy picture first, with a visible Proxy tier and accessible label, and it is never a Camera target",
        proxy_tier == Some(PictureTier::Proxy)
            && d.app().sequence_cursor.min(last) == last
            && painted.len() == 1
            && painted[0]["fully_visible"] == true
            && label.as_deref().is_some_and(|label| label.ends_with("· proxy preview"))
            && d.app().presentation.refining()
            && camera_blocked
            && d.revision() == revision,
        json!({"tier":"Proxy","cursor":last,"badge":"Proxy fully visible","label":"… · proxy preview","camera_target":false}),
        json!({"tier":format!("{proxy_tier:?}"),"cursor":d.app().sequence_cursor,"badge":painted,"label":label,"refining":d.app().presentation.refining(),"camera_blocked":camera_blocked}),
    )?;
    d.capture("Proxy picture while seeking")?;
    d.wait_for("Exact Original picture decoded", |app| {
        app.feedback.held_reply.is_some()
    })?;
    let refined = d
        .app_mut()
        .feedback
        .held_reply
        .take()
        .ok_or("No refined reply")?;
    let refined_tier = refined.picture.as_ref().map(|picture| picture.tier).ok();
    let same_ticket = refined.ticket == ticket;
    d.app_mut().feedback.hold_preview = false;
    d.app_mut().feedback.release_reply = Some(refined);
    d.wait_for("Exact Original picture displayed", |app| {
        app.presentation.displayed_tier() == Some(PictureTier::Original)
            && !app.presentation.needs_render()
    })?;
    d.settled()?;
    let painted = badge(d);
    let label = d.app().presentation.displayed_label();
    d.check(
        "When the cursor rests, the exact Original picture of the same request replaces the proxy and the tier disappears",
        refined_tier == Some(PictureTier::Original)
            && same_ticket
            && painted.is_empty()
            && label.as_deref().is_some_and(|label| !label.contains("proxy"))
            && !d.app().presentation.refining()
            && d.revision() == revision,
        json!({"tier":"Original","same_request":true,"badge":[]}),
        json!({"tier":format!("{refined_tier:?}"),"same_request":same_ticket,"badge":painted,"label":label}),
    )?;
    d.capture("Exact Original picture after the cursor rests")?;
    d.report.skipped.push("The background build itself is not exercised here: this Original needs no proxy by policy. proxy_real_media encodes and verifies proxies through the real worker, and perf seek builds them for 1080p60 and 4K30.".into());
    Ok(())
}
