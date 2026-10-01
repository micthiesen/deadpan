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

fn trim(
    document: &ProjectDocument,
    parent: &str,
    edge: SourceTrimEdge,
    delta: i64,
) -> (ProjectDocument, EditTransaction) {
    let before = document.clone();
    let resolution = document
        .source_trim(
            &id(parent),
            &id("target"),
            edge,
            delta,
            SourceTrimMode::Ripple,
        )
        .unwrap();
    assert_eq!(resolution.applied_delta_frames, delta);
    let allocation = revision("trim");
    let tx = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: allocation.clone(),
            command: Command::TrimSource {
                parent: id(parent),
                node: id("target"),
                edge,
                delta_frames: delta,
                mode: SourceTrimMode::Ripple,
                wrapper: resolution.needs_wrapper.then(|| id("trim-window")),
                timing: AudioTimingId {
                    allocation,
                    ordinal: 0,
                },
            },
        },
    )
    .unwrap();
    assert_eq!(document, &before);
    let after = tx.forward.apply(document).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *document);
    (after, tx)
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

#[test]
fn trim_44100_crop_keeps_filter_context_and_tail_growth_uses_the_original_rate() {
    let base = document(
        ntsc(),
        &["lead", "target"],
        vec![("lead", silence(1)), ("target", source(ntsc(), 4))],
    );
    let before = linked_target(&base, 4, true);
    let mut provider = Provider::new();
    let (crop, crop_tx) = trim(&before, "root", SourceTrimEdge::In, 1);
    // Original 44.1 kHz coordinates are mix coordinates times 147/160.
    // W=[0,4): ceil([6000,12406.4)*147/160)=[5513,11399).
    // Retained old frame-2 entry: (6000+1601.4)*147/160=5587029/800.
    let cropped = oracle(&provider, 5513..11399, ratio(5_587_029, 800), 256);
    let faded: Vec<_> = cropped
        .iter()
        .enumerate()
        .map(|(at, sample)| {
            let gain = ((at as f64 + 0.5) / 96.0).min(1.0) as f32;
            [sample[0] * gain, sample[1] * gain]
        })
        .collect();
    assert_ne!(
        cropped,
        oracle(
            &provider,
            5513..11399,
            ratio(5_587_029, 800).checked_add(ratio(147, 160)).unwrap(),
            256
        )
    );
    for chunk in [193, 239] {
        exact(
            &read_chunks(&crop, &mut provider, 1602, 256, chunk, false),
            &cropped,
            "44.1 kHz raw crop",
        );
        exact(
            &read_chunks(&crop, &mut provider, 1602, 256, chunk, true),
            &faded,
            "44.1 kHz raw context with a new consuming-output entry fade",
        );
    }
    assert_eq!(crop_tx.inverse.apply(&crop).unwrap(), before);

    let (grown, tx) = trim(&before, "root", SourceTrimEdge::Out, 1);
    assert_eq!(
        RenderPlan::compile(&grown)
            .unwrap()
            .audio_duration()
            .unwrap(),
        AudioSample(9610)
    );
    // W=[0,5) ends at (6000+8008)*147/160=12869.85. The newly
    // delivered tail starts at B(5)=8008, or exact mix position 12406.4.
    let tail = oracle(&provider, 5513..12870, ratio(569_919, 50), 256);
    for chunk in [193, 239] {
        exact(
            &read_chunks(&grown, &mut provider, 8008, 256, chunk, false),
            &tail,
            "44.1 kHz added tail",
        );
        exact(
            &read_chunks(&grown, &mut provider, 8008, 256, chunk, true),
            &tail,
            "old end no longer fades",
        );
    }
    let baseline = read_chunks(&before, &mut provider, 1602, 256, 193, false);
    let restored = tx.inverse.apply(&grown).unwrap();
    exact(
        &read_chunks(&restored, &mut provider, 1602, 256, 239, false),
        &baseline,
        "44.1 kHz inverse",
    );
}

#[test]
fn in_extension_reanchors_repeat_gap_preserve_and_outer_suffix_owners_once() {
    let mut repeated = repeat("repeated-voice", 2);
    let NodeKind::Repeat { gap, .. } = &mut repeated.kind else {
        unreachable!()
    };
    *gap = Some(room(1, 700..921));
    let base = document(
        ntsc(),
        &["lead", "group", "outer"],
        vec![
            ("lead", silence(1)),
            (
                "group",
                BeatNode::sequence("Nested", vec![id("target"), id("repeat"), id("stage")]),
            ),
            ("target", source(ntsc(), 3)),
            ("repeat", repeated),
            ("repeated-voice", source(ntsc(), 1)),
            ("stage", preserve("stretched-voice", 4, 12)),
            ("stretched-voice", source(ntsc(), 4)),
            ("outer", source(ntsc(), 3)),
        ],
    );
    let before = linked_target(&base, 3, false);
    let mut provider = Provider::new();
    let (after, tx) = trim(&before, "group", SourceTrimEdge::In, -1);
    assert_eq!(
        RenderPlan::compile(&before)
            .unwrap()
            .audio_duration()
            .unwrap(),
        AudioSample(35235)
    );
    assert_eq!(
        RenderPlan::compile(&after)
            .unwrap()
            .audio_duration()
            .unwrap(),
        AudioSample(36837)
    );
    // First Repeat play entered at B(4)=6406, 2/5 sample before exact
    // frame 4. Reanchor keeps that phase after it moves to exact frame 5.
    let first_play = expected(&provider, ratio(-2, 5), 256);
    exact(
        &read_chunks(&before, &mut provider, 6406, 256, 193, false),
        &first_play,
        "independent old Repeat phase",
    );
    assert_ne!(first_play, expected(&provider, ExactRatio::ZERO, 256));
    exact(
        &read_chunks(&after, &mut provider, 8008, 256, 239, false),
        &first_play,
        "retained Repeat phase",
    );
    // Check each allocated owner separately: gap/play counts can change by
    // one at NTSC. Preserve and outer windows below retain equal counts.
    for (label, old_start, new_start, count) in [
        ("gap", 8008, 9610, 1601),
        ("second play", 9610, 11211, 1601),
        ("opaque Preserve", 11211, 12813, 19219),
        ("outer sibling", 30430, 32032, 4805),
    ] {
        let old = read_chunks(&before, &mut provider, old_start, count, 193, false);
        assert!(
            old.iter().flatten().any(|sample| sample.abs() > 0.001),
            "{label} fixture must be audible"
        );
        for chunk in [193, 239] {
            exact(
                &read_chunks(&after, &mut provider, new_start, count, chunk, false),
                &old,
                label,
            );
        }
    }
    assert_eq!(before.nodes()[&id("repeat")], after.nodes()[&id("repeat")]);
    assert_eq!(before.nodes()[&id("stage")], after.nodes()[&id("stage")]);
    let restored = tx.inverse.apply(&after).unwrap();
    exact(
        &read_chunks(&restored, &mut provider, 11211, 512, 251, false),
        &read_chunks(&before, &mut provider, 11211, 512, 193, false),
        "composite inverse PCM",
    );
}

#[path = "trim/roll.rs"]
mod roll;

#[path = "trim/source_endpoints.rs"]
mod source_endpoints;

#[path = "trim/combined.rs"]
mod combined;
