//! Atomic test patches qualify the clock primitive without inventing a Trim command.
use super::*;

fn fixture(
    lead: i64,
    start: i64,
    end: i64,
    audio_start: ExactRatio,
) -> (ProjectDocument, Provider) {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut voice = source(rate, 0, 8197, 6);
    let NodeKind::Source { source } = &mut voice.kind else {
        unreachable!()
    };
    source.audio_offset = AudioSample(7);
    source.audio_mapping = SourceAudioMapping::Placement {
        start: audio_start,
        frames: ratio(40985, 8008),
    };
    document(
        rate,
        &["lead", "crop"],
        vec![
            ("lead", hold(lead)),
            ("crop", partition("a", start, end)),
            ("a", voice),
        ],
    )
}

fn endpoint_patch(
    before: &ProjectDocument,
    start: i64,
    end: i64,
    endpoint: AudioSourceEndpoint,
) -> DocumentPatch {
    let timing = AudioTimingId {
        allocation: revision("endpoint-phase"),
        ordinal: 0,
    };
    let captured = capture_unbound_audio_bindings(before, timing.clone()).unwrap();
    let mut records: Vec<_> = captured
        .timings()
        .iter()
        .map(|(id, layout)| AudioTimingRecord {
            id: id.clone(),
            layout: layout.clone(),
        })
        .collect();
    if !captured.timings().contains_key(&timing) {
        records.push(AudioTimingRecord {
            id: timing.clone(),
            layout: FrozenAudioLayout::capture(before).unwrap(),
        });
    }
    let mut placement = lattice("a", AudioClockRoot::ProjectRootRoundEven);
    placement.reference.timing = timing;
    let mut bindings = captured.bindings().clone();
    bindings
        .get_mut(&id("a"))
        .unwrap()
        .reanchors
        .push(AudioReanchorStep::for_source_endpoint(placement, endpoint));
    let bindings =
        AudioBindingState::new_with_gaps(records, bindings, captured.gap_bindings().clone())
            .unwrap();
    DocumentPatch {
        project_id: before.project_id().clone(),
        from_revision: before.revision_id().clone(),
        to_revision: revision("endpoint-phase"),
        presentation: None,
        nodes: BTreeMap::from([(
            id("crop"),
            ValueChange {
                before: Some(before.nodes()[&id("crop")].clone()),
                after: Some(partition("a", start, end)),
            },
        )]),
        assets: BTreeMap::new(),
        marks: BTreeMap::new(),
        sounds: BTreeMap::new(),
        sound_routes: BTreeMap::new(),
        sound_allowances: BTreeMap::new(),
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
        audio_lineage: BTreeMap::new(),
        audio_bindings: Some(ValueChange {
            before: Some(before.audio_bindings().clone()),
            after: Some(bindings),
        }),
    }
}

#[track_caller]
fn exact(actual: &[[f32; 2]], expected: &[[f32; 2]]) {
    assert_eq!(actual.len(), expected.len());
    if let Some((at, (a, b))) = actual
        .iter()
        .zip(expected)
        .enumerate()
        .find(|(_, (a, b))| a != b)
    {
        panic!("endpoint PCM first mismatch {at}: {a:?} != {b:?}");
    }
}
fn backwards(
    doc: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    count: usize,
    chunk: usize,
) -> Vec<[f32; 2]> {
    assert!((1..=256).contains(&chunk));
    let mut reader = StageAudio::new(Arc::new(RenderPlan::compile(doc).unwrap()));
    let mut result = vec![[0.; 2]; count];
    for offset in (0..count)
        .step_by(chunk)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let n = (count - offset).min(chunk);
        result[offset..offset + n].copy_from_slice(
            &render(
                &mut reader,
                provider,
                start + i64::try_from(offset).unwrap(),
                u32::try_from(n).unwrap(),
            )
            .samples,
        );
    }
    result
}

#[test]
fn disjoint_later_endpoint_keeps_literal_phase_through_rebase_and_inverse() {
    let (before, mut provider) = fixture(5, 0, 1, ExactRatio::ZERO);
    let immutable = before.clone();
    let baseline = pcm(&before, &mut provider, 8108..8364, 193, false);
    let patch = endpoint_patch(&before, 2, 3, AudioSourceEndpoint::End);
    let after = patch.apply(&before).unwrap();
    // End: B(6)=9610 becomes virtual B(4)=6406. At B(5)=8008,
    // Original q=(9610-8008)+(8008-6406)-offset7=3197.
    let expected = source_oracle(&provider, 0..8197, ExactRatio::integer(3197), 256);
    assert_ne!(
        expected,
        source_oracle(&provider, 0..8197, ExactRatio::integer(3196), 256)
    );
    for chunk in [193, 239] {
        exact(
            &backwards(&after, &mut provider, 8008, 256, chunk),
            &expected,
        );
    }
    let prefix = prefix_patch(&after, "crop", 2, after.audio_bindings());
    let rebased = prefix.apply(&after).unwrap();
    for chunk in [193, 239] {
        exact(
            &backwards(&rebased, &mut provider, 8008, 256, chunk),
            &expected,
        );
    }
    assert_eq!(prefix.inverse().apply(&rebased).unwrap(), after);
    let restored = patch.inverse().apply(&after).unwrap();
    assert_eq!(restored, immutable);
    assert_eq!(before, immutable);
    exact(
        &pcm(&restored, &mut provider, 8108..8364, 239, false),
        &baseline,
    );
}

#[test]
fn disjoint_earlier_endpoint_uses_signed_old_origin_with_offset_once() {
    let (before, mut provider) = fixture(1, 2, 3, ratio(-5000, 8008)); // -1000 canonical samples.
    let patch = endpoint_patch(&before, 0, 1, AudioSourceEndpoint::Start);
    let after = patch.apply(&before).unwrap();
    // Physical origin -1f; nearest old start maps B(1)->B(3).
    // (1602+1601.6)+(1602-4805)+1000-7=993.6.
    // The physical owner starts at local zero, so its permitted Source samples
    // start at 1000-offset7=993. The earlier part of the file is excluded.
    let expected = source_oracle(&provider, 993..8197, ratio(4968, 5), 256);
    assert_ne!(
        expected,
        source_oracle(&provider, 993..8197, ratio(4963, 5), 256)
    );
    assert_ne!(
        expected,
        source_oracle(&provider, 0..8197, ratio(4968, 5), 256)
    );
    // Away from the 128-sample kernel halo, this same phase agrees with the
    // whole-file oracle. Keep support and phase as separate assertions.
    let body = source_oracle(&provider, 993..8197, ratio(5968, 5), 256);
    exact(
        &body,
        &source_oracle(&provider, 0..8197, ratio(5968, 5), 256),
    );
    for chunk in [193, 239] {
        exact(
            &backwards(&after, &mut provider, 1602, 256, chunk),
            &expected,
        );
        exact(&backwards(&after, &mut provider, 1802, 256, chunk), &body);
    }
    assert_eq!(patch.inverse().apply(&after).unwrap(), before);
}

#[test]
fn endpoint_extends_prior_resume_terms_and_reanchors_without_recapturing_phase() {
    let (current, mut provider) = fixture(4, 3, 4, ExactRatio::ZERO);
    let (first, _) = fixture(1, 0, 6, ExactRatio::ZERO);
    let (second, _) = fixture(2, 0, 6, ExactRatio::ZERO);
    let mut old_wire = serde_json::to_value(&current).unwrap();
    old_wire["nodes"].as_object_mut().unwrap().remove("lead");
    old_wire["nodes"].as_object_mut().unwrap().remove("crop");
    old_wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("a")])).unwrap();
    let old = ProjectDocument::from_json(&old_wire.to_string()).unwrap();
    let state = AudioBindingState::new(
        [&old, &first, &second]
            .into_iter()
            .enumerate()
            .map(|(n, doc)| AudioTimingRecord {
                id: historical_placement(u32::try_from(n).unwrap())
                    .reference
                    .timing,
                layout: FrozenAudioLayout::capture(doc).unwrap(),
            })
            .collect(),
        BTreeMap::from([(
            id("a"),
            OwnedAudioBinding {
                lattice: historical_placement(0),
                resume: Some(AudioResume {
                    local_boundary: ExactRatio::ONE,
                    phase: AudioLocalPhase {
                        constant: ratio(64 * 5, 8008),
                        terms: vec![AudioPhaseTerm {
                            placement: historical_placement(0),
                            from_local: ExactRatio::ZERO,
                            to_local: ExactRatio::ONE,
                        }],
                    },
                }),
                reanchors: vec![
                    AudioReanchorStep::for_allocation(
                        historical_placement(1),
                        Some(ExactFrameRange::new(ratio(3, 1), ratio(7, 1)).unwrap()),
                    ),
                    AudioReanchorStep::for_allocation(
                        historical_placement(2),
                        Some(ExactFrameRange::new(ratio(5, 1), ratio(8, 1)).unwrap()),
                    ),
                ],
            },
        )]),
    )
    .unwrap();
    let before = with_bindings(&current, &state);
    exact(
        &pcm(&before, &mut provider, 6406..6662, 193, false),
        &source_oracle(&provider, 0..8197, ExactRatio::integer(4863), 256),
    );
    let patch = endpoint_patch(&before, 4, 5, AudioSourceEndpoint::End);
    let after = patch.apply(&before).unwrap();
    // 64+4*1602-offset7=6465, not a new source-start-derived phase.
    let expected = source_oracle(&provider, 0..8197, ExactRatio::integer(6465), 256);
    assert_ne!(
        expected,
        source_oracle(&provider, 0..8197, ExactRatio::integer(6464), 256)
    );
    for chunk in [193, 239] {
        exact(
            &backwards(&after, &mut provider, 6406, 256, chunk),
            &expected,
        );
    }
    for (id, layout) in before.audio_bindings().timings() {
        assert_eq!(after.audio_bindings().timings().get(id), Some(layout));
    }
    assert_eq!(patch.inverse().apply(&after).unwrap(), before);
}

#[test]
fn previously_dormant_source_uses_structural_endpoint_then_reveals_real_pcm() {
    let (before, mut provider) = fixture(5, 0, 1, ExactRatio::integer(2));
    for chunk in [193, 239] {
        assert_eq!(
            backwards(&before, &mut provider, 8008, 256, chunk),
            vec![[0.; 2]; 256]
        );
    }
    assert_eq!(provider.calls, 0);
    let patch = endpoint_patch(&before, 2, 3, AudioSourceEndpoint::End);
    let after = patch.apply(&before).unwrap();
    // At output 8108: retained physical q=3204+100, mapping starts
    // at 2*(8008/5)=3203.2, and the independent offset is 7. Thus
    // source q=93.8; the old crop contained no selected audio at all.
    let expected = source_oracle(&provider, 0..8197, ratio(469, 5), 256);
    assert_ne!(expected, vec![[0.; 2]; 256]);
    assert_ne!(
        expected,
        source_oracle(&provider, 0..8197, ratio(464, 5), 256)
    );
    for chunk in [193, 239] {
        exact(
            &backwards(&after, &mut provider, 8108, 256, chunk),
            &expected,
        );
    }
    assert!(provider.calls > 0);
    assert_eq!(patch.inverse().apply(&after).unwrap(), before);
}

#[test]
fn absent_source_audio_keeps_endpoint_clock_without_reading_media() {
    let (base, mut provider) = fixture(5, 0, 1, ExactRatio::ZERO);
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    let picture = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 30,
            time_base,
        },
    )
    .unwrap();
    let mut node = base.nodes()[&id("a")].clone();
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    source.audio = None;
    source.audio_mapping = SourceAudioMapping::FitBeat;
    source.audio_offset = AudioSample(0);
    source.video = SourceVideo::Stream {
        asset: AssetId::new("picture").unwrap(),
        span: picture,
    };
    let mut wire = serde_json::to_value(&base).unwrap();
    wire["nodes"]["a"] = serde_json::to_value(node).unwrap();
    wire["assets"]["picture"] = serde_json::to_value(AssetRecord {
        label: "Picture without audio".into(),
        content_hash: "a".repeat(64),
        video: Some(picture),
        audio: None,
        still_image: false,
        frame_count: Some(frames(30)),
        source_qualification: None,
    })
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = endpoint_patch(&before, 2, 3, AudioSourceEndpoint::End)
        .apply(&before)
        .unwrap();
    for chunk in [193, 239] {
        assert_eq!(
            backwards(&after, &mut provider, 8008, 256, chunk),
            vec![[0.; 2]; 256]
        );
    }
    assert_eq!(provider.calls, 0);
}
