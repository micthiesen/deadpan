//! 44.1 kHz reconstruction and shifted composite owners through TrimSource.
//! Synthetic video clocks admit the pure command; decoded PCM is real media.

use super::*;

fn linked_target(document: &ProjectDocument, duration: i64, automatic: bool) -> ProjectDocument {
    let full = audio(0..44_117);
    let mut node = document.nodes()[&id("target")].clone();
    node.audio_edges = Default::default();
    if !automatic {
        node.audio_edges.node_start = AudioEdgePolicy::Hard;
        node.audio_edges.node_end = AudioEdgePolicy::Hard;
        node.audio_edges.source_placement_start = AudioEdgePolicy::Hard;
        node.audio_edges.source_placement_end = AudioEdgePolicy::Hard;
    }
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    let full_frames = SourceAudioMapping::natural_rate(full.span, ntsc())
        .unwrap()
        .duration_frames(frames(duration))
        .unwrap();
    let start = ratio(-30_000, 8008); // 6000 mix samples of real prefix context.
    let selection = ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(duration)).unwrap();
    source.video = SourceVideo::Stream {
        asset: AssetId::new("media").unwrap(),
        span: full.span,
    };
    source.video_mapping = SourceVideoMapping::SelectedPlacement {
        start,
        frames: full_frames,
        selection,
        endpoints: EndpointPolicy::HoldAdjacent,
    };
    source.audio = Some(full.clone());
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start,
        frames: full_frames,
        selection,
    };
    source.audio_offset = AudioSample(0);
    source.edit_window = Some(SourceEditWindow::new(selection.start, selection.end).unwrap());
    source.link = LinkRelation::Linked;
    let mut record = document.assets()[&AssetId::new("media").unwrap()].clone();
    record.video = Some(full.span);
    record.still_image = false;
    record.source_qualification = Some(SourceQualificationId::new("b".repeat(64)).unwrap());
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["target"] = serde_json::to_value(node).unwrap();
    wire["assets"]["media"] = serde_json::to_value(record).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn read_chunks(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    count: usize,
    chunk: u32,
    faded: bool,
) -> Vec<[f32; 2]> {
    assert!(chunk > 0 && chunk <= 256);
    let mut reader = renderer(document, provider);
    let mut result = vec![[0.; 2]; count];
    let offsets: Vec<_> = (0..count).step_by(chunk as usize).collect();
    // Cold tail first, then backward blocks; never rely on forward cache state.
    for offset in offsets.into_iter().rev() {
        let frames = u32::try_from((count - offset).min(chunk as usize)).unwrap();
        let at = start + i64::try_from(offset).unwrap();
        let block = if faded {
            reader
                .read_edge_faded(
                    provider,
                    AudioSample(at),
                    frames,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        } else {
            read(&mut reader, provider, at, frames)
        };
        result[offset..offset + frames as usize].copy_from_slice(&block);
    }
    result
}

fn oracle(
    provider: &Provider,
    support: Range<i64>,
    phase: ExactRatio,
    count: u32,
) -> Vec<[f32; 2]> {
    assert!(count <= 256);
    provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                support,
                phase,
                AudioSample(0),
                ratio(147, 160),
                AudioSample(0)..AudioSample(i64::from(count)),
            )
            .unwrap(),
            AudioSample(0),
            count,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

#[track_caller]
fn exact(actual: &[[f32; 2]], expected: &[[f32; 2]], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label}");
    if let Some((at, (actual, expected))) = actual
        .iter()
        .zip(expected)
        .enumerate()
        .find(|(_, (a, b))| a != b)
    {
        panic!("{label}: first mismatch {at}: actual {actual:?}, expected {expected:?}");
    }
}

#[path = "trim/roll.rs"]
mod roll;

#[path = "trim/combined.rs"]
mod combined;
