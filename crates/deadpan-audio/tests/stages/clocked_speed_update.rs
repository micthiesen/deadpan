//! Exact speed updates use native resampling, then retained-domain transfer.
//! A composed native coordinate slope is not an equivalent PCM oracle.
use super::*;
use deadpan_audio::{RootSignalBlock, SignalTransferError};
use deadpan_plan::{AudioBoundDomain, AudioSignalContent};

fn fixture(partial: bool) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let spoken = |native, frames| {
        let mut node = source(rate, frames, 0..1);
        let NodeKind::Source { source } = &mut node.kind else {
            unreachable!()
        };
        let audio = audio_at_rate(native, native + 20_000, 44_100);
        source.audio_mapping = SourceAudioMapping::natural_rate(audio.span, rate).unwrap();
        source.audio = Some(audio);
        node
    };
    let initial = document_with_asset(
        rate,
        &["lead", "slow", "tail"],
        [
            ("lead", hold(2)),
            ("slow", retime("inner", 24, 0..12, PitchPolicy::FollowSpeed)),
            ("inner", spoken(3100, 12)),
            ("tail", spoken(9000, 2)),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let before = insert_pause(&initial, 5, 3, "clocked-speed");
    assert_eq!(before.duration().unwrap(), duration(31));
    assert_eq!(before.derived_clocks()[&id("slow")].extent, ratio(27, 1));
    assert_eq!(before.derived_clocks()[&id("slow")].child_end, ratio(27, 2));
    assert!(
        before
            .audio_bindings()
            .bindings()
            .contains_key(&id("clocked-speed-split-2"))
    );
    assert!(before.audio_bindings().bindings().contains_key(&id("tail")));
    if !partial {
        return before;
    }
    let mut wire = serde_json::to_value(&before).unwrap();
    for owner in ["clocked-speed-split-2", "tail"] {
        assert!(
            wire["audio_bindings"]["bindings"]
                .as_object_mut()
                .unwrap()
                .remove(owner)
                .is_some()
        );
    }
    // Fixture-only binding removal can orphan a timing table. Retain every
    // record referenced by any remaining binding, phase, or sound journal.
    fn references(value: &serde_json::Value, used: &mut BTreeSet<AudioTimingId>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, value) in fields {
                    if key == "timing" {
                        used.insert(serde_json::from_value(value.clone()).unwrap());
                    } else {
                        references(value, used);
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    references(value, used);
                }
            }
            _ => {}
        }
    }
    let state = wire["audio_bindings"].as_object_mut().unwrap();
    let mut used = BTreeSet::new();
    for (key, value) in state.iter() {
        if key != "timings" {
            references(value, &mut used);
        }
    }
    state
        .get_mut("timings")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .retain(|record| {
            used.contains(&serde_json::from_value::<AudioTimingId>(record["id"].clone()).unwrap())
        });
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn native_carrier(
    provider: &FixtureProvider,
    support: Range<i64>,
    selected: Range<i64>,
    phase: ExactRatio,
    step: ExactRatio,
) -> Vec<[f32; 2]> {
    let recipe = ResampleRecipe::new(
        selected,
        phase,
        AudioSample(support.start),
        step,
        AudioSample(support.start)..AudioSample(support.end),
    )
    .unwrap();
    let mut output = Vec::new();
    let length = (support.end - support.start) as usize;
    while output.len() < length {
        let count = (length - output.len()).min(251) as u32;
        output.extend(
            provider
                .source
                .prepare(
                    recipe.clone(),
                    AudioSample(support.start + output.len() as i64),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    output
}

fn transfer(
    support: Range<i64>,
    carrier: &[[f32; 2]],
    at: ExactRatio,
    live: Range<i64>,
    step: ExactRatio,
) -> Recorded {
    assert_eq!(carrier.len(), (support.end - support.start) as usize);
    let recipe = RootSignalTransfer::new(
        AudioSample(support.start)..AudioSample(support.end),
        at,
        SignalSample(live.start),
        step,
        SignalSample(live.start)..SignalSample(live.end),
    )
    .unwrap();
    let mut result = Recorded {
        samples: Vec::new(),
        suppressed: Vec::new(),
    };
    while result.samples.len() < (live.end - live.start) as usize {
        let start = live.start + result.samples.len() as i64;
        let count = (live.end - start).min(127) as u32;
        let block = recipe
            .render::<SignalTransferError>(
                SignalSample(start),
                count,
                &AtomicBool::new(false),
                |at, count| {
                    let start = (at.0 - support.start) as usize;
                    Ok(RootSignalBlock {
                        start: at,
                        samples: carrier[start..start + count as usize].to_vec(),
                        suppressed: vec![],
                    })
                },
            )
            .unwrap();
        result
            .suppressed
            .extend((start..start + i64::from(count)).map(|at| {
                block
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&SignalSample(at)))
            }));
        result.samples.extend(block.samples);
    }
    result
}

fn expected(provider: &FixtureProvider, partial: bool, frames: i64) -> Recorded {
    let (left, pause, right, tail, total, step) = match frames {
        27 => (
            3203..8008,
            8008..12813,
            12813..46446,
            46446..49650,
            49650,
            ExactRatio::ONE,
        ),
        15 => (
            3203..5873,
            5873..8542,
            8542..27227,
            27227..30430,
            30430,
            ratio(9, 5),
        ),
        _ => unreachable!(),
    };
    let mut result = Recorded {
        samples: vec![[0.; 2]; total],
        suppressed: vec![true; total],
    };
    // The existing physical alias keeps the COMPLETE Source12 carrier, not
    // the left/right Partition's current demand. Current native end is20758.
    let original = native_carrier(
        provider,
        3203..41642,
        3100..20758,
        ratio(4_959_853, 1600),
        ratio(147, 320),
    );
    let left_pcm = transfer(3203..41642, &original, ratio(3203, 1), left.clone(), step);
    result.samples[left.start as usize..left.end as usize].copy_from_slice(&left_pcm.samples);
    result.suppressed[left.start as usize..left.end as usize].copy_from_slice(&left_pcm.suppressed);
    let right_pcm = if partial {
        // Missing right binding starts from its PRE-EDIT CURRENT full Source
        // root[5,29), not the old lattice of its original source before pause.
        // Visible right begins at12813, phase5307.296875 on this carrier.
        let current = native_carrier(
            provider,
            8008..46446,
            3100..20758,
            ratio(3100, 1),
            ratio(147, 320),
        );
        transfer(8008..46446, &current, ratio(12813, 1), right.clone(), step)
    } else {
        transfer(3203..41642, &original, ratio(8008, 1), right.clone(), step)
    };
    result.samples[right.start as usize..right.end as usize].copy_from_slice(&right_pcm.samples);
    result.suppressed[right.start as usize..right.end as usize]
        .copy_from_slice(&right_pcm.suppressed);
    let (support, phase) = if partial {
        (46446..49650, ratio(3_599_853, 400)) //8999.6325 at current root29
    } else {
        (41642..44845, ratio(3_600_147, 400)) //9000.3675 at historical root26
    };
    let native = native_carrier(
        provider,
        support.clone(),
        9000..11943,
        phase,
        ratio(147, 160),
    );
    let tail_pcm = transfer(
        support.clone(),
        &native,
        ratio(support.start.into(), 1),
        tail.clone(),
        ExactRatio::ONE,
    );
    result.samples[tail.start as usize..tail.end as usize].copy_from_slice(&tail_pcm.samples);
    result.suppressed[tail.start as usize..tail.end as usize].copy_from_slice(&tail_pcm.suppressed);
    // Full old tail demand has3204 cells. Its historical carrier has3203
    // when retained from root26, and3204 when captured at current root29.
    assert_eq!(
        tail.end - tail.start,
        if frames == 27 { 3204 } else { 3203 }
    );
    let exhausted = (tail.end - tail.start)
        .saturating_sub(support.end - support.start)
        .max(0) as usize;
    assert_eq!(exhausted, usize::from(!partial && frames == 27));
    if exhausted != 0 {
        assert_eq!(result.samples[total - 1], [0.; 2]);
        assert!(result.suppressed[total - 1]);
    }
    assert!(
        result.suppressed[pause.start as usize..pause.end as usize]
            .iter()
            .all(|value| *value)
    );
    result
}

fn same(actual: &Recorded, expected: &Recorded, label: &str) {
    assert_eq!(
        actual.samples.len(),
        expected.samples.len(),
        "{label} length"
    );
    assert_eq!(
        actual.suppressed.len(),
        expected.suppressed.len(),
        "{label} mask length"
    );
    for (at, (actual, expected)) in actual.samples.iter().zip(&expected.samples).enumerate() {
        for channel in 0..2 {
            assert_eq!(
                actual[channel], expected[channel],
                "{label} sample{at} channel{channel}"
            );
        }
    }
    for (at, (actual, expected)) in actual
        .suppressed
        .iter()
        .zip(&expected.suppressed)
        .enumerate()
    {
        assert_eq!(actual, expected, "{label} suppression sample{at}");
    }
}

fn inspect_after(document: &ProjectDocument, partial: bool) {
    let plan = RenderPlan::compile(document).unwrap();
    for (live, reference, raw, step) in [
        (3203..5873, 3203, 3203..41642, ratio(9, 5)),
        (
            8542..27227,
            if partial { 12813 } else { 8008 },
            if partial { 8008..46446 } else { 3203..41642 },
            ratio(9, 5),
        ),
        (
            27227..30430,
            if partial { 46446 } else { 41642 },
            if partial { 46446..49650 } else { 41642..44845 },
            ExactRatio::ONE,
        ),
    ] {
        let query = plan
            .audio_processing(
                AudioSample(live.start)..AudioSample(live.start + 1),
                Default::default(),
            )
            .unwrap();
        assert_eq!(query.spans.len(), 1);
        assert_eq!(
            query.spans[0].allocated_samples,
            AudioSample(live.start)..AudioSample(live.end)
        );
        let AudioSignalContent::Bound(bound) = &query.spans[0].content else {
            panic!("physical timing must be retained")
        };
        assert_eq!(
            bound.reference_at_offset(0).unwrap(),
            ratio(reference.into(), 1)
        );
        assert_eq!(bound.reference_samples_per_output_sample(), step);
        let AudioBoundDomain::Root(domain) = bound.raw_domain().unwrap() else {
            panic!("root carrier")
        };
        assert_eq!(
            domain.root_samples(),
            AudioSample(raw.start)..AudioSample(raw.end)
        );
    }
}

fn update(before: &ProjectDocument, frames: i64, revision: &str) -> ProjectDocument {
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command: Command::SetRetime {
            node: id("slow"),
            duration: duration(frames),
            pitch: PitchPolicy::FollowSpeed,
        },
    };
    let transaction = apply(before, &request).unwrap();
    let old = before.derived_clocks()[&id("slow")].extent;
    assert_eq!(transaction.duration_delta, frames - old.numerator() as i64);
    let after = transaction.forward.apply(before).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *before);
    assert_eq!(
        after.derived_clocks()[&id("slow")].child_start,
        ExactRatio::ZERO
    );
    assert_eq!(after.derived_clocks()[&id("slow")].child_end, ratio(27, 2));
    assert_eq!(
        after.derived_clocks()[&id("slow")].extent,
        ratio(frames.into(), 1)
    );
    for (node, clock) in before.derived_clocks() {
        if node != &id("slow") {
            assert_eq!(after.derived_clocks()[node], *clock);
        }
    }
    for node in ["inner", "clocked-speed-split-2"] {
        if let Some(old) = before.audio_bindings().bindings().get(&id(node)) {
            assert_eq!(
                after.audio_bindings().bindings().get(&id(node)),
                Some(old),
                "inside sampling history must stay unchanged"
            );
        }
    }
    if let Some(old) = before.audio_bindings().bindings().get(&id("tail")) {
        let new = &after.audio_bindings().bindings()[&id("tail")];
        assert_eq!(new.lattice, old.lattice);
        assert_eq!(new.resume, old.resume);
        assert!(new.reanchors.starts_with(&old.reanchors));
        // An existing entry resume can already express this movement, making
        // another allocation-entry step inert. The literal inspect_after map
        // and complete PCM/mask oracle prove the actual transport separately.
    }
    assert_eq!(
        after.nodes()[&id("clocked-speed-pause")],
        before.nodes()[&id("clocked-speed-pause")]
    );
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn witness(partial: bool) {
    let before = fixture(partial);
    let (_, mut provider) = mono_original(2);
    let old = expected(&provider, partial, 27);
    assert!(old.samples.iter().flatten().any(|value| value.abs() > 0.01));
    same(
        &record(&before, &mut provider, false, &[251, 3]),
        &old,
        "literal pre-update baseline",
    );
    let after = update(&before, 15, "clocked-speed-15");
    assert_eq!(after.duration().unwrap(), duration(19));
    inspect_after(&after, partial);
    let new = expected(&provider, partial, 15);
    same(
        &record(&after, &mut provider, false, &[127, 3, 251]),
        &new,
        "two-stage updated PCM",
    );
    same(
        &record(&after, &mut provider, false, &[1, 239]),
        &new,
        "updated chunk parity",
    );
    // A second PUBLIC update returns to the same exact owner geometry, with
    // the same cascaded sampling clocks. Reuse the independent old oracle,
    // never a captured processed output or an aligned actual baseline.
    let returned = update(&after, 27, "clocked-speed-return-27");
    assert_eq!(returned.duration().unwrap(), duration(31));
    same(
        &record(&returned, &mut provider, false, &[127, 3, 251]),
        &old,
        "second speed update composes history",
    );
}

#[test]
fn clocked_speed_update_27_to_15_keeps_two_stage_pcm_and_retained_outside_source() {
    witness(false);
}

#[test]
fn clocked_speed_update_27_to_15_captures_partial_current_carriers_and_repeated_update() {
    witness(true);
}

#[test]
fn clocked_speed_update_keeps_empty_old_room_tone_carrier_suppressed_without_source_reads() {
    let initial = document_with_asset(
        FrameRate::new(30_000, 1001).unwrap(),
        &["lead", "slow", "tail"],
        [
            ("lead", hold(2)),
            (
                "slow",
                retime("input", 1, 0..100_001, PitchPolicy::FollowSpeed),
            ),
            (
                "input",
                BeatNode::sequence(
                    "Tiny room / silence",
                    vec![id("tiny-tone"), id("silent-rest")],
                ),
            ),
            ("tiny-tone", room_tone(1, audio_at_rate(0, 321, 44_100))),
            ("silent-rest", hold(100_000)),
            ("tail", hold(2)),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    // The tiny tone originally occupies root[2,2+1/100001). Both
    // RoundEven boundaries are3203, so the complete old carrier is empty.
    let captured = insert_pause(&initial, 0, 1, "empty-old-capture");
    let expansion = apply(
        &captured,
        &CommandRequest {
            project_id: captured.project_id().clone(),
            expected_revision: captured.revision_id().clone(),
            new_revision: RevisionId::new("empty-old-expanded").unwrap(),
            command: Command::SetRetime {
                node: id("slow"),
                duration: duration(100_000),
                pitch: PitchPolicy::FollowSpeed,
            },
        },
    )
    .unwrap();
    let expanded = expansion.forward.apply(&captured).unwrap();
    assert_eq!(expansion.inverse.apply(&expanded).unwrap(), captured);
    assert!(expanded.derived_clocks().is_empty());
    // Public exact pause falls just after the tone, inside silent-rest. No
    // huge PCM preparation is performed by constructing this public history.
    let before = insert_pause(&expanded, 4, 3, "empty-old-exact");
    assert_eq!(
        before.derived_clocks()[&id("slow")].extent,
        ratio(100_003, 1)
    );
    let old_binding = before.audio_bindings().bindings()[&id("tiny-tone")].clone();
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["slow"]["kind"]["duration"] = serde_json::to_value(duration(1000)).unwrap();
    wire["derived_clocks"]["slow"]["extent"] = serde_json::to_value(ratio(1000, 1)).unwrap();
    let candidate =
        ProjectDocument::from_json(&wire.to_string()).expect("coherent test-only proposed extent");
    candidate.validate().unwrap();
    assert_eq!(candidate.audio_bindings(), before.audio_bindings());
    assert_eq!(candidate.duration().unwrap(), duration(1005));
    for (owner, clock) in before.derived_clocks() {
        if owner != &id("slow") {
            assert_eq!(candidate.derived_clocks()[owner], *clock);
        }
    }
    let (_, mut provider) = mono_original(2);
    fn empty_pcm(document: &ProjectDocument, provider: &mut FixtureProvider) {
        provider.revisions.insert(document.revision_id().clone());
        let plan = Arc::new(RenderPlan::compile(document).unwrap());
        let query = plan
            .audio_processing(AudioSample(4805)..AudioSample(4821), Default::default())
            .unwrap();
        assert_eq!(query.spans.len(), 1);
        assert_eq!(
            query.spans[0].allocated_samples,
            AudioSample(4805)..AudioSample(4821)
        );
        let AudioSignalContent::Bound(bound) = &query.spans[0].content else {
            panic!("retained room-tone owner")
        };
        let step = ratio(100_003, 100_000 * 1000);
        assert_eq!(bound.reference_samples_per_output_sample(), step);
        assert!(
            step.checked_mul(ratio(64, 1))
                .unwrap()
                .compare_integer(1)
                .is_lt()
        );
        let AudioBoundDomain::Root(domain) = bound.raw_domain().unwrap() else {
            panic!("old root carrier")
        };
        assert_eq!(domain.root_samples(), AudioSample(3203)..AudioSample(3203));
        // Current tone span=100000000/10000400003 root frames. It receives
        //16 allocated samples although the captured physical signal has none.
        assert_eq!(
            ratio(3, 1)
                .checked_add(ratio(100_000_000, 10_000_400_003))
                .unwrap()
                .checked_mul(ratio(8008, 5))
                .unwrap()
                .round_even()
                .unwrap(),
            4821
        );
        let calls = provider.calls;
        let mut renderer = StageAudio::new(plan);
        let whole = renderer
            .read(
                provider,
                AudioSample(4805),
                16,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(whole.samples.len(), 16);
        assert_eq!(whole.suppressed, vec![AudioSample(4805)..AudioSample(4821)]);
        for (offset, sample) in whole.samples.iter().enumerate() {
            assert_eq!(
                *sample,
                [0.; 2],
                "empty carrier output sample{}",
                4805 + offset
            );
        }
        let mut start = 4805;
        for count in [1, 3, 12] {
            let block = renderer
                .read(
                    provider,
                    AudioSample(start),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                block.suppressed,
                vec![AudioSample(start)..AudioSample(start + i64::from(count))]
            );
            for sample in block.samples {
                assert_eq!(sample, [0.; 2]);
            }
            start += i64::from(count);
        }
        assert_eq!(start, 4821);
        assert_eq!(
            provider.calls, calls,
            "empty old domain must skip source resolution/preparation"
        );
        assert_eq!(
            renderer.cached_stage_count(),
            0,
            "no RoomTone PCM was prepared"
        );
    }
    // Establish actual renderer behavior FIRST, independently of the new
    // public command guard. Empty support bypasses an otherwise invalid step.
    empty_pcm(&candidate, &mut provider);
    let snapshot = before.to_json().unwrap();
    let outcome = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new("empty-old-clocked-speed").unwrap(),
            command: Command::SetRetime {
                node: id("slow"),
                duration: duration(1000),
                pitch: PitchPolicy::FollowSpeed,
            },
        },
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
    let transaction = outcome.unwrap_or_else(|error| {
        panic!(
            "empty old physical carrier must admit public speed update; code={:?}; message={}",
            error.code, error.message
        )
    });
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    assert_eq!(after.duration().unwrap(), duration(1005));
    assert_eq!(
        after.audio_bindings().bindings()[&id("tiny-tone")],
        old_binding
    );
    let after = ProjectDocument::from_json(&after.to_json().unwrap()).unwrap();
    empty_pcm(&after, &mut provider);

    // Fresh capture control: grow only to2 before authoring the exact pause.
    // Removing the old binding makes the PRE-EDIT CURRENT tone's physical
    // carrier root[3,3+2/100001), whose two boundaries both round to4805.
    let expansion = apply(
        &captured,
        &CommandRequest {
            project_id: captured.project_id().clone(),
            expected_revision: captured.revision_id().clone(),
            new_revision: RevisionId::new("empty-fresh-expanded").unwrap(),
            command: Command::SetRetime {
                node: id("slow"),
                duration: duration(2),
                pitch: PitchPolicy::FollowSpeed,
            },
        },
    )
    .unwrap();
    let expanded = expansion.forward.apply(&captured).unwrap();
    assert_eq!(expansion.inverse.apply(&expanded).unwrap(), captured);
    let exact = insert_pause(&expanded, 4, 3, "empty-fresh-exact");
    assert_eq!(exact.derived_clocks()[&id("slow")].extent, ratio(5, 1));
    let mut wire = serde_json::to_value(&exact).unwrap();
    assert!(
        wire["audio_bindings"]["bindings"]
            .as_object_mut()
            .unwrap()
            .remove("tiny-tone")
            .is_some()
    );
    fn references(value: &serde_json::Value, used: &mut BTreeSet<AudioTimingId>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, value) in fields {
                    if key == "timing" {
                        used.insert(serde_json::from_value(value.clone()).unwrap());
                    } else {
                        references(value, used);
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    references(value, used);
                }
            }
            _ => {}
        }
    }
    let state = wire["audio_bindings"].as_object_mut().unwrap();
    let mut used = BTreeSet::new();
    for (key, value) in state.iter() {
        if key != "timings" {
            references(value, &mut used);
        }
    }
    state
        .get_mut("timings")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .retain(|record| {
            used.contains(&serde_json::from_value::<AudioTimingId>(record["id"].clone()).unwrap())
        });
    let fresh = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(
        !fresh
            .audio_bindings()
            .bindings()
            .contains_key(&id("tiny-tone"))
    );
    let transaction = apply(
        &fresh,
        &CommandRequest {
            project_id: fresh.project_id().clone(),
            expected_revision: fresh.revision_id().clone(),
            new_revision: RevisionId::new("empty-fresh-clocked-speed").unwrap(),
            command: Command::SetRetime {
                node: id("slow"),
                duration: duration(1000),
                pitch: PitchPolicy::FollowSpeed,
            },
        },
    )
    .expect("a fresh physical carrier with zero old samples must admit its live allocation");
    assert_eq!(transaction.duration_delta, 995);
    let changed = transaction.forward.apply(&fresh).unwrap();
    assert_eq!(transaction.inverse.apply(&changed).unwrap(), fresh);
    let changed = ProjectDocument::from_json(&changed.to_json().unwrap()).unwrap();
    assert_eq!(changed.duration().unwrap(), duration(1005));
    assert!(
        changed
            .audio_bindings()
            .bindings()
            .contains_key(&id("tiny-tone"))
    );
    provider.revisions.insert(changed.revision_id().clone());
    let plan = Arc::new(RenderPlan::compile(&changed).unwrap());
    let query = plan
        .audio_processing(AudioSample(4805)..AudioSample(4811), Default::default())
        .unwrap();
    assert_eq!(query.spans.len(), 1);
    assert_eq!(
        query.spans[0].allocated_samples,
        AudioSample(4805)..AudioSample(4811)
    );
    let AudioSignalContent::Bound(bound) = &query.spans[0].content else {
        panic!("freshly captured room-tone owner")
    };
    assert_eq!(bound.reference_samples_per_output_sample(), ratio(1, 200));
    assert!(
        ratio(1, 200)
            .checked_mul(ratio(64, 1))
            .unwrap()
            .compare_integer(1)
            .is_lt()
    );
    let AudioBoundDomain::Root(domain) = bound.raw_domain().unwrap() else {
        panic!("fresh empty root carrier")
    };
    assert_eq!(domain.root_samples(), AudioSample(4805)..AudioSample(4805));
    // Current tone width=400/100001 root frames; RoundEven end4811 gives
    // exactly6 live samples. This differs from the bound carrier's16 above.
    assert_eq!(
        ratio(3, 1)
            .checked_add(ratio(400, 100_001))
            .unwrap()
            .checked_mul(ratio(8008, 5))
            .unwrap()
            .round_even()
            .unwrap(),
        4811
    );
    let calls = provider.calls;
    let mut renderer = StageAudio::new(plan);
    let block = renderer
        .read(
            &mut provider,
            AudioSample(4805),
            6,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples.len(), 6);
    assert_eq!(block.suppressed, vec![AudioSample(4805)..AudioSample(4811)]);
    for (offset, sample) in block.samples.iter().enumerate() {
        assert_eq!(
            *sample,
            [0.; 2],
            "fresh empty carrier output sample{}",
            4805 + offset
        );
    }
    let mut start = 4805;
    for count in [1, 2, 3] {
        let block = renderer
            .read(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            block.suppressed,
            vec![AudioSample(start)..AudioSample(start + i64::from(count))]
        );
        for sample in block.samples {
            assert_eq!(sample, [0.; 2]);
        }
        start += i64::from(count);
    }
    assert_eq!(start, 4811);
    assert_eq!(
        provider.calls, calls,
        "fresh empty carrier must not resolve/prepare source PCM"
    );
    assert_eq!(renderer.cached_stage_count(), 0);
}

#[path = "clocked_speed_update/source_crop_pcm.rs"]
mod source_crop_pcm;
