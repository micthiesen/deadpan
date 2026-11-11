//! Complete Trim over real 44.1 kHz PCM. Expected time maps are literal, not
//! obtained from the new resolver, binding queries or returned render spans.
use super::*;

fn combined(
    before: &ProjectDocument,
    intent: SourceTrimIntent,
) -> (ProjectDocument, EditTransaction) {
    let r = before
        .source_trim_edit(&id("root"), &id("target"), None, intent)
        .unwrap();
    let tx = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("combined"),
            command: Command::ApplySourceTrim {
                parent: id("root"),
                node: id("target"),
                right: None,
                intent,
                resources: SourceTrimResources {
                    target_wrapper: r.required_target_wrapper.then(|| id("target-window")),
                    right_wrapper: r.required_right_wrapper.then(|| id("right-window")),
                    split: SplitIdentities {
                        nodes: (0..r.required_split_nodes)
                            .map(|n| id(&format!("combined-split-{n}")))
                            .collect(),
                    },
                    fillers: (0..r.required_filler_nodes)
                        .map(|n| id(&format!("combined-filler-{n}")))
                        .collect(),
                },
            },
        },
    )
    .unwrap();
    let after = tx.forward.apply(before).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), *before);
    (after, tx)
}

fn linked_at_rate(document: &ProjectDocument, length: i64) -> ProjectDocument {
    let mut node = document.nodes()[&id("target")].clone();
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    let full = audio(0..44117);
    let selection = ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(length)).unwrap();
    let extent =
        SourceAudioMapping::natural_rate(full.span, document.presentation_basis().frame_rate)
            .unwrap()
            .duration_frames(frames(length))
            .unwrap();
    source.video = SourceVideo::Stream {
        asset: full.asset.clone(),
        span: full.span,
    };
    source.video_mapping = SourceVideoMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: extent,
        selection,
        endpoints: EndpointPolicy::HoldAdjacent,
    };
    source.audio = Some(full.clone());
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: extent,
        selection,
    };
    source.edit_window = Some(SourceEditWindow::new(selection.start, selection.end).unwrap());
    source.link = LinkRelation::Linked;
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["target"] = serde_json::to_value(node).unwrap();
    let mut asset = document.assets()[&AssetId::new("media").unwrap()].clone();
    asset.video = Some(full.span);
    asset.still_image = false;
    asset.source_qualification = Some(SourceQualificationId::new("b".repeat(64)).unwrap());
    wire["assets"]["media"] = serde_json::to_value(asset).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn picture_only(mut node: BeatNode, length: i64) -> BeatNode {
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    source.duration = frames(length);
    let selection = ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(length)).unwrap();
    source.edit_window = Some(SourceEditWindow::new(selection.start, selection.end).unwrap());
    let SourceVideoMapping::SelectedPlacement {
        selection: video, ..
    } = &mut source.video_mapping
    else {
        unreachable!()
    };
    *video = selection;
    source.audio = None;
    source.audio_mapping = SourceAudioMapping::FitBeat;
    source.audio_offset = AudioSample(0);
    source.link = LinkRelation::Independent;
    node
}

fn bus(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    count: usize,
    chunk: u32,
) -> Vec<[f32; 2]> {
    let mut reader = renderer(document, provider);
    let mut result = vec![[0.; 2]; count];
    for at in (0..count)
        .step_by(chunk as usize)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let n = (count - at).min(chunk as usize);
        let block = reader
            .prepare_authored_bus(
                provider,
                AudioSample(start + at as i64),
                n as u32,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        result[at..at + n].copy_from_slice(&block.samples);
    }
    result
}

#[test]
fn combined_ripple_appends_one_root_map_after_a_prior_route_with_exact_44100_phase() {
    let base = document(ntsc(), &["target"], vec![("target", source(ntsc(), 4))]);
    let base = linked_at_rate(&base, 4);
    let target = picture_only(base.nodes()[&id("target")].clone(), 4);
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["lead"] = serde_json::to_value(picture_only(target.clone(), 1)).unwrap();
    wire["nodes"]["tail"] = serde_json::to_value(picture_only(target.clone(), 1)).unwrap();
    wire["nodes"]["target"] = serde_json::to_value(target).unwrap();
    let mut root = BeatNode::sequence(
        "Picture without Hold gates",
        vec![id("lead"), id("target"), id("tail")],
    );
    root.audio_treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-6000).unwrap(), false, vec![], vec![]).unwrap(),
    );
    wire["nodes"]["root"] = serde_json::to_value(root).unwrap();
    let full = audio(0..44117);
    let sound = SoundId::new("effect").unwrap();
    wire["sounds"]["effect"] = serde_json::to_value(SoundEvent {
        owner: id("root"),
        label: "Prior routed sound".into(),
        source: full.clone(),
        mapping: SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: SourceAudioMapping::natural_rate(full.span, ntsc())
                .unwrap()
                .duration_frames(frames(5))
                .unwrap(),
            selection: ExactFrameRange::new(ExactRatio::ZERO, ratio(40005, 8008)).unwrap(),
        },
        offset: AudioSample(7),
        treatments: deadpan_core::AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    })
    .unwrap();
    let previous = RootSoundEdit {
        grid: RootSoundGrid::root(ntsc()),
        operation: RootSoundOperation::Insert {
            at: ProjectFrame(2),
            duration: frames(1),
        },
        cuts: RootSoundCutEdges {
            before: AudioEdgePolicy::Hard,
            after: AudioEdgePolicy::Hard,
        },
    };
    wire["sound_routes"]["effect"] = serde_json::to_value(RootSoundRoute {
        recipe_extent: frames(5),
        recipe_grid: RootSoundGrid::root(ntsc()),
        edits: vec![previous],
    })
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let immutable = before.clone();
    let intent = SourceTrimIntent {
        in_frames: 1,
        out_frames: 1,
        policy: SourceTrimPolicy::Ripple,
        ..Default::default()
    };
    let (after, tx) = combined(&before, intent);
    assert_eq!(before, immutable);
    assert_eq!(after.duration().unwrap(), frames(6));
    assert_eq!(after.sound_routes()[&sound].edits.len(), 2);
    assert_eq!(after.sound_routes()[&sound].edits[0], previous);
    assert_eq!(
        after.sound_routes()[&sound].edits[1].operation,
        RootSoundOperation::Trim {
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(5)).unwrap(),
            in_frames: 1,
            out_frames: 1,
        }
    );
    let mut provider = Provider::new();
    // Complete I=O=1 anchors new B1=1602 to old B2=3203. Thus new 3203
    // still maps to old4804, the final sample of the prior Insert's gap.
    // The first audible sample is new 3204 -> old 4805 -> recipe 3203 ->
    // source 3196 after offset 7, at 44.1 kHz phase 3196*147/160=117453/40.
    // Full recipe selection ends at 8001 mix samples, giving support 0..7351.
    let factor = 10_f64.powf(-6.0 / 20.0);
    for (start, phase) in [(3204, ratio(117453, 40)), (8208, ratio(970053, 160))] {
        let raw = oracle(&provider, 0..7351, phase, 256);
        assert!(raw.iter().flatten().any(|v| v.abs() > 0.001));
        assert_ne!(
            raw,
            oracle(
                &provider,
                0..7351,
                phase.checked_add(ratio(147, 160)).unwrap(),
                256
            )
        );
        let expected: Vec<_> = raw
            .iter()
            .map(|s| {
                [
                    (f64::from(s[0]) * factor) as f32,
                    (f64::from(s[1]) * factor) as f32,
                ]
            })
            .collect();
        for chunk in [193, 239] {
            exact(
                &bus(&after, &mut provider, start, 256, chunk),
                &expected,
                "one root map, offset and root gain once",
            );
        }
    }
    for start in [1602, 6406] {
        for chunk in [193, 239] {
            assert_eq!(
                bus(&after, &mut provider, start, 256, chunk),
                vec![[0.; 2]; 256]
            );
        }
    }
    assert_eq!(bus(&after, &mut provider, 3203, 1, 1), vec![[0.; 2]]);
    // The complete map retains the old final sample 9609. Sequential rounded
    // Delete+Insert reaches old 9608 instead. This is a separate negative
    // control, not an implementation of the command under test.
    let raw_last = oracle(&provider, 0..7351, ExactRatio::integer(7350), 1);
    let old_last: Vec<_> = raw_last
        .iter()
        .map(|s| {
            [
                (f64::from(s[0]) * factor) as f32,
                (f64::from(s[1]) * factor) as f32,
            ]
        })
        .collect();
    let wrong = oracle(&provider, 0..7351, ratio(1175853, 160), 1);
    assert_ne!(raw_last, wrong);
    exact(
        &bus(&after, &mut provider, 9609, 1, 1),
        &old_last,
        "final sample survives one old-to-final map",
    );
    let mut sequential = serde_json::to_value(&after).unwrap();
    let mut route = before.sound_routes()[&sound].clone();
    for operation in [
        RootSoundOperation::Delete {
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(2)).unwrap(),
        },
        RootSoundOperation::Insert {
            at: ProjectFrame(4),
            duration: frames(1),
        },
    ] {
        route.edits.push(RootSoundEdit {
            grid: RootSoundGrid::root(ntsc()),
            operation,
            cuts: RootSoundCutEdges {
                before: AudioEdgePolicy::Hard,
                after: AudioEdgePolicy::Hard,
            },
        });
    }
    sequential["sound_routes"]["effect"] = serde_json::to_value(route).unwrap();
    let sequential = ProjectDocument::from_json(&sequential.to_string()).unwrap();
    let wrong: Vec<_> = wrong
        .iter()
        .map(|s| {
            [
                (f64::from(s[0]) * factor) as f32,
                (f64::from(s[1]) * factor) as f32,
            ]
        })
        .collect();
    exact(
        &bus(&sequential, &mut provider, 9609, 1, 1),
        &wrong,
        "sequential rounded negative control",
    );
    let restored = tx.inverse.apply(&after).unwrap();
    assert_eq!(restored, immutable);
    for chunk in [193, 239] {
        exact(
            &bus(&restored, &mut provider, 5005, 256, chunk),
            &bus(&before, &mut provider, 5005, 256, 193),
            "inverse prior root route",
        );
    }
}
