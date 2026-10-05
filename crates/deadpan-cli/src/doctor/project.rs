//! `doctor --project`: read-only, bounded diagnostics for one package. It
//! names the actual stored sources, decoder interpretation and preview canvas
//! and times the once-per-revision stages (snapshot load, validation, plan
//! compile, anchor index), so a slow workspace refresh is attributable without
//! reading logs. Nothing here is authored state and nothing is written.

use std::path::Path;
use std::time::Instant;

use deadpan_core::{AnchorIndex, NodeKind};
use deadpan_plan::RenderPlan;
use deadpan_store::{AccessMode, ProjectStore};

use crate::CliError;

fn ms(started: Instant) -> f64 {
    (started.elapsed().as_secs_f64() * 1_000_000.0).round() / 1000.0
}

pub fn report(package: &Path) -> Result<serde_json::Value, CliError> {
    let opened = Instant::now();
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let open_ms = ms(opened);
    let loaded = Instant::now();
    let document = store.snapshot()?;
    let snapshot_ms = ms(loaded);
    let validated = Instant::now();
    let durations = document.durations()?;
    let validate_ms = ms(validated);
    let compiled = Instant::now();
    let plan = RenderPlan::compile(&document)?;
    let compile_ms = ms(compiled);
    let indexed = Instant::now();
    drop(AnchorIndex::new(&document)?);
    let anchor_ms = ms(indexed);
    let root_children = match &document.nodes()[document.root()].kind {
        NodeKind::Sequence { children } => children.len(),
        _ => 0,
    };
    let mut sources = Vec::new();
    for (asset, record) in document.assets() {
        // No stored qualification (for example a legacy or generated asset)
        // is a fact, not a failure; a stored one that cannot be read is an error.
        if record.source_qualification.is_none() {
            sources.push(serde_json::json!({
                "asset": asset, "label": record.label, "qualification": "absent",
            }));
            continue;
        }
        let receipt = match store.registered_source(document.revision_id(), asset) {
            Ok(receipt) => receipt,
            Err(error) => {
                sources.push(serde_json::json!({
                    "asset": asset, "label": record.label, "qualification": "error",
                    "error": error.to_string(),
                }));
                continue;
            }
        };
        let original = store.original_record(receipt.original().content())?;
        let video = receipt.snapshot().video().map(|video| {
            let info = video.interpretation();
            let frames = video.index().index().frames();
            let keyframes: Vec<usize> = frames
                .iter()
                .enumerate()
                .filter_map(|(ordinal, frame)| frame.keyframe.then_some(ordinal))
                .collect();
            let max_spacing = keyframes
                .windows(2)
                .map(|pair| pair[1] - pair[0])
                .chain(keyframes.last().map(|last| frames.len() - last))
                .max();
            serde_json::json!({
                "codec": info.codec, "pixel_format": info.pixel_format,
                "width": info.width, "height": info.height,
                "rotation_quarter_turns": info.rotation_quarter_turns,
                "indexed_frames": frames.len(), "keyframes": keyframes.len(),
                "max_keyframe_spacing_frames": max_spacing,
            })
        });
        sources.push(serde_json::json!({
            "asset": asset,
            "label": record.label,
            "qualification": "qualified",
            "video": video,
            "audio_layout": receipt.snapshot().audio_layout().map(|layout| format!("{layout:?}")),
            "original_bytes": original.as_ref().map(|original| original.object().byte_length()),
            "original_managed": original.as_ref().map(|original| original.managed()),
        }));
    }
    let file_bytes = |name: &str| std::fs::metadata(package.join(name)).map_or(0, |m| m.len());
    let basis = document.presentation_basis();
    Ok(serde_json::json!({
        "package": package,
        "revision": document.revision_id(),
        // One cold sample each in this process, not a distribution.
        "single_sample_ms": {
            "open_read_only": open_ms,
            "head_snapshot": snapshot_ms,
            "validate_durations": validate_ms,
            "plan_compile": compile_ms,
            "anchor_index": anchor_ms,
        },
        // Opening hashes every stored history row; only revisions after this
        // build's receipt are recomputed.
        "history_validation": store.open_validation(),
        "document": {
            "nodes": document.nodes().len(),
            "root_children": root_children,
            "timed_nodes": durations.len(),
            "marks": document.marks().len(),
            "assets": document.assets().len(),
            "duration_frames": plan.duration().frames(),
        },
        "plan": plan.metadata(),
        "preview": {
            "canvas": [basis.width, basis.height],
            "frame_rate": basis.frame_rate,
            "color_policy": basis.color_policy,
            "quality_tier": "full canvas; no automatic proxy or reduced preview tier is implemented",
        },
        "decoder_path": "one persistent descriptor-only FFmpeg decoder per retained Original snapshot (deadpan-source); picture, audio and export sessions each open their own",
        "encoder_path": "automatic_sdr_v1 hardware/software admission probe at render time (see render report provenance)",
        "storage": {
            "database_bytes": file_bytes("project.sqlite"),
            "wal_bytes": file_bytes("project.sqlite-wal"),
        },
        "sources": sources,
    }))
}
