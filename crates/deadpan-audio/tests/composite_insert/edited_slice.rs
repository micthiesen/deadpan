//! Actual decoded PCM checks for immutable edited-content capture. Raw timing
//! uses StageAudio::read; gain ownership is checked separately on the authored
//! bus. This does not qualify mastering, acoustic quality or device delivery.
use super::*;

#[path = "edited_slice/move_range.rs"]
mod move_range;
#[path = "edited_slice/placement.rs"]
mod placement;

fn boundary(frame: i64) -> i64 {
    ntsc().audio_boundary(ProjectFrame(frame)).unwrap().0
}

fn capture(document: &ProjectDocument, parent: &str, range: Range<i64>) -> CapturedEditSlice {
    let before = document.clone();
    let slice = CapturedEditSlice::capture(
        document,
        &id(parent),
        FrameRange::new(ProjectFrame(range.start), ProjectFrame(range.end)).unwrap(),
        AudioTimingId {
            allocation: revision("capture-only"),
            ordinal: 0,
        },
    )
    .unwrap();
    assert_eq!(*document, before, "capture must be history-neutral");
    assert_eq!(slice.duration(), frames(range.end - range.start));
    slice
}

fn paste(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    index: usize,
    name: &str,
) -> ProjectDocument {
    let required = slice.identity_requirements().unwrap();
    let after = edit(
        document,
        name,
        Command::SpliceSlice {
            parent: id("root"),
            index,
            slice: slice.clone(),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|index| id(&format!("{name}-node-{index}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|index| MarkId::new(format!("{name}-mark-{index}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|index| id(&format!("{name}-alias-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert_eq!(
        after.duration().unwrap().frames(),
        document.duration().unwrap().frames() + slice.duration().frames()
    );
    after
}

fn cut(document: &ProjectDocument, selected: Range<i64>, name: &str) -> ProjectDocument {
    let range = FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end)).unwrap();
    let required = document
        .range_deletion(&id("root"), range)
        .unwrap()
        .required_ids;
    edit(
        document,
        name,
        Command::DeleteRange {
            parent: id("root"),
            range,
            identities: SplitIdentities {
                nodes: (0..required).map(|i| id(&format!("{name}-{i}"))).collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

fn pcm(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    count: usize,
) -> Vec<[f32; 2]> {
    let mut reader = renderer(document, provider);
    let mut result = Vec::with_capacity(count);
    for offset in (0..count).step_by(256) {
        result.extend(read(
            &mut reader,
            provider,
            start + i64::try_from(offset).unwrap(),
            u32::try_from((count - offset).min(256)).unwrap(),
        ));
    }
    result
}

#[track_caller]
fn exact(actual: &[[f32; 2]], expected: &[[f32; 2]], offset: usize) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "retained sample {}", offset + index);
    }
}

#[track_caller]
fn cold_reverse(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    expected: &[[f32; 2]],
) {
    assert!(expected.iter().flatten().all(|sample| sample.is_finite()));
    assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.001));
    let mut reader = renderer(document, provider);
    let tail = expected.len().saturating_sub(173);
    exact(
        &read(
            &mut reader,
            provider,
            start + i64::try_from(tail).unwrap(),
            u32::try_from(expected.len() - tail).unwrap(),
        ),
        &expected[tail..],
        tail,
    );
    let mut chunks = Vec::new();
    let mut offset = 0;
    for count in [193, 1, 47, 256, 113].into_iter().cycle() {
        if offset == expected.len() {
            break;
        }
        let count = count.min(expected.len() - offset);
        chunks.push((offset, count));
        offset += count;
    }
    for (offset, count) in chunks.into_iter().rev() {
        exact(
            &read(
                &mut reader,
                provider,
                start + i64::try_from(offset).unwrap(),
                u32::try_from(count).unwrap(),
            ),
            &expected[offset..offset + count],
            offset,
        );
    }
    exact(
        &read(&mut reader, provider, start, 128),
        &expected[..128],
        0,
    );
}

fn destination(prefix: i64, suffix: i64) -> ProjectDocument {
    document(
        ntsc(),
        &["prefix", "suffix"],
        vec![
            ("prefix", source(ntsc(), prefix)),
            ("suffix", source(ntsc(), suffix)),
        ],
    )
}

fn gain(trim: i32, envelopes: Vec<GainEnvelope>) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(trim).unwrap(), false, envelopes, vec![]).unwrap(),
    )
}

#[test]
fn unrelated_repeats_with_equal_raw_play_ids_keep_distinct_historical_clocks() {
    let original = document(
        ntsc(),
        &["lead", "repeat", "other-repeat", "tail"],
        vec![
            ("lead", source(ntsc(), 5)),
            ("repeat", repeat("voice", 2)),
            ("voice", source(ntsc(), 2)),
            ("other-repeat", repeat("other-voice", 2)),
            (
                "other-voice",
                BeatNode::hold("Other room", room(3, 700..921)),
            ),
            ("tail", source(ntsc(), 5)),
        ],
    );
    let paused = insert_pause(&original, 5, 1, "two-repeat-pause");
    let retained = cut(&paused, 1..2, "two-repeat-cut");
    let slice = capture(&retained, "root", 5..15);
    let NodeKind::Sequence { children } = &retained.nodes()[retained.root()].kind else {
        unreachable!()
    };
    let copied = paste(&retained, &slice, children.len(), "two-repeats-copied");
    let mut provider = Provider::new();
    let saved = pcm(&retained, &mut provider, 8008, 16016);
    let source_entry = expected(&provider, ExactRatio::ZERO, 128);
    assert_close(&saved[..128], &source_entry);
    let room = room_reference(&provider, 700..921, 4805);
    // The independent second Repeat originally starts at frame9: its entry
    // is B(9)-9*1601.6=-2/5, despite identical raw play allocation/ordinal IDs.
    assert_close(
        &saved[6406..6534],
        &sample_reference(&room, ratio(-2, 5), ExactRatio::ONE, 128),
    );
    cold_reverse(&copied, &mut provider, 32032, &saved);
}

fn authored(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    count: u32,
) -> Vec<[f32; 2]> {
    let mut reader = renderer(document, provider);
    reader
        .prepare_authored_bus(
            provider,
            AudioSample(start),
            count,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

#[test]
fn contents_exclude_unselected_parent_gain_while_owned_group_retains_it() {
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        GainRange::new(ExactRatio::ZERO, ExactRatio::integer(7)).unwrap(),
        GainDb::new(-6000).unwrap(),
        vec![
            GainSegment::new(
                ExactRatio::integer(7),
                GainDb::new(6000).unwrap(),
                GainCurve::Linear,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let mut voice = source(ntsc(), 7);
    voice.audio_treatments = gain(-3000, vec![envelope]);
    let mut group = BeatNode::sequence("Treated group", vec![id("voice")]);
    group.audio_treatments = gain(6000, vec![]);
    let original = document(
        ntsc(),
        &["lead", "group"],
        vec![("lead", silence(1)), ("group", group), ("voice", voice)],
    );
    let contents = capture(&original, "group", 2..4);
    let owned_group = capture(&original, "root", 1..8);
    let content_paste = paste(&destination(7, 5), &contents, 1, "content-gain-copy");
    let group_paste = paste(&destination(6, 5), &owned_group, 1, "group-gain-copy");
    let mut provider = Provider::new();
    // Destination shifts by five frames, exactly 8008 samples, preserving
    // nominal owner progress as well as raw sampling phase. Interior windows
    // avoid asserting anything about automatic fades at the new slice edges.
    let at = boundary(2) + 512;
    let old = authored(&original, &mut provider, at, 128);
    let content = authored(&content_paste, &mut provider, at + 8008, 128);
    let whole_group = authored(&group_paste, &mut provider, at + 8008, 128);
    assert_eq!(whole_group, old);
    let raw = pcm(&content_paste, &mut provider, at + 8008, 128);
    let oracle: Vec<_> = raw
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            // Original voice starts at exact sample1601.6; gain is evaluated in
            // its seven-frame owner clock with documented Q32 progress rounding.
            let local = ratio(i128::from(at + i64::try_from(index).unwrap()) * 5 - 8008, 5);
            let progress = local
                .checked_div(ratio(56056, 5))
                .unwrap()
                .checked_mul(ExactRatio::integer(
                    i64::try_from(GAIN_NUMERIC_SCALE).unwrap(),
                ))
                .unwrap()
                .round_even()
                .unwrap();
            let millidb = -9000. + 12000. * progress as f64 / GAIN_NUMERIC_SCALE as f64;
            sample.map(|value| (f64::from(value) * 10_f64.powf(millidb / 20000.)) as f32)
        })
        .collect();
    assert_close(&content, &oracle);
    let parent_factor = 10_f64.powf(6000. / 20000.);
    let with_parent: Vec<_> = content
        .iter()
        .map(|sample| sample.map(|value| (f64::from(value) * parent_factor) as f32))
        .collect();
    assert_close(&old, &with_parent);
    assert_ne!(content, old);
}
