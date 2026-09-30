use std::{fs::OpenOptions, io::Write, path::Path, sync::atomic::AtomicBool, time::Instant};

use deadpan_cli::{
    audio::OfflineAudioSession,
    export_picture::{ExportPictureSession, OutputFrameOrdinal},
    picture::ProjectPictureSession,
};
use deadpan_core::{AudioSample, FrameRange, RevisionId};
use deadpan_render::PictureRenderer;
use serde_json::{Value, json};

use super::Result;

pub(super) fn capture(
    package: &Path,
    revision: &RevisionId,
    range: FrameRange,
    name: &str,
    directory: &Path,
    deadline: Instant,
) -> Result<Value> {
    let cancelled = AtomicBool::new(false);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))?;
    let info = adapter.get_info();
    if info.backend != wgpu::Backend::Metal {
        return Err("reference capture requires Metal".into());
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Deadpan workflow qualification references"),
        ..Default::default()
    }))?;
    let mut pictures = ExportPictureSession::new(
        ProjectPictureSession::open_revision(package, revision, Some(range), &cancelled)?,
        PictureRenderer::new(&device, &queue),
        &cancelled,
        deadline,
    )?;
    let contract = pictures.contract().clone();
    if contract.frame_count() > 540 {
        return Err("reference fixture frame bound".into());
    }
    let mut audio =
        OfflineAudioSession::open_revision(package, revision, range, &cancelled, deadline)?;
    if audio.sample_range() != (contract.project_audio_start()..contract.project_audio_end()) {
        return Err("reference PCM and picture contract disagree".into());
    }
    let picture_path = directory.join(format!("{name}.reference.i420"));
    let audio_path = directory.join(format!("{name}.reference.f32"));
    let mut picture_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&picture_path)?;
    let mut audio_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&audio_path)?;
    for ordinal in 0..contract.frame_count() {
        let frame = pictures.prepare(OutputFrameOrdinal(ordinal), &cancelled, deadline)?;
        if frame.contract() != &contract {
            return Err("reference picture identity changed".into());
        }
        picture_file.write_all(frame.pixels().bytes())?;
    }
    let mut position = audio.sample_range().start.0;
    while position < audio.sample_range().end.0 {
        let count = u32::try_from((audio.sample_range().end.0 - position).min(8192))?;
        for sample in audio
            .read(AudioSample(position), count, &cancelled)?
            .samples
        {
            for channel in sample {
                audio_file.write_all(&channel.to_le_bytes())?;
            }
        }
        position = position
            .checked_add(i64::from(count))
            .ok_or("reference sample overflow")?;
    }
    picture_file.sync_all()?;
    audio_file.sync_all()?;
    Ok(
        json!({"contract": contract, "picture_reference": picture_path, "audio_reference": audio_path,
        "gpu": {"name": info.name, "backend": format!("{:?}", info.backend)},
        "scope": "fresh direct shared-pipeline picture and canonical limited PCM inputs; independent encoded readers run separately"}),
    )
}
