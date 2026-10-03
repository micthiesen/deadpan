//! Visual-only comparison of the actual decoded picture through the shared GPU
//! compositor at a fixed raster. It is separate from UI performance samples.

use super::*;
use sha2::{Digest as _, Sha256};
use std::sync::atomic::AtomicBool;

pub(super) fn fingerprint(d: &mut Driver<'_>) -> Result<Option<String>, String> {
    if d.options.mode != RunMode::Visual {
        return Ok(None);
    }
    d.settled()?;
    let app = d.app_mut();
    let picture = app
        .presentation
        .picture()
        .ok_or("Missing group comparison picture")?;
    let frame = picture
        .frame
        .as_ref()
        .ok_or("Group comparison requires decoded video")?;
    let layers = camera::render_layers(picture)?;
    // A fixed target isolates authored composition from command/footer layout.
    // This target is private to this synchronous replay checkpoint and is never
    // registered with egui or substituted for the visible preview target.
    let target = app
        .renderer
        .create_target(320, 180)
        .map_err(|error| error.to_string())?;
    if let Some((width, height)) = picture.canvas {
        app.renderer
            .render_composed(
                frame,
                &target,
                picture.picture_context.as_deref(),
                [width, height],
                FitMode::Fit,
                &layers,
            )
            .map_err(|error| error.to_string())?;
    } else {
        app.renderer
            .render(frame, &target, FitMode::Fit)
            .map_err(|error| error.to_string())?;
    }
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut readback = app
        .renderer
        .begin_working_readback(&target, &cancelled, deadline)
        .map_err(|error| error.to_string())?;
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
        std::thread::yield_now();
    };
    let stride = usize::try_from(pixels.row_stride_bytes()).map_err(|error| error.to_string())?;
    let width = usize::try_from(pixels.width()).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    for row in pixels.bytes().chunks_exact(stride) {
        // GPU padding has no pixel meaning and need not be deterministic.
        digest.update(&row[..width * 8]);
    }
    Ok(Some(
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    ))
}
