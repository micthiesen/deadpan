//! Command-level PCM oracles share decoding helpers with the stage suite, but
//! derive cut/resume coordinates directly from the project frame clock.
use super::*;

#[path = "occurrence_pause.rs"]
mod occurrence_pause;

#[path = "processing_hold_duration.rs"]
mod processing_hold_duration;

fn insert_pause(document: &ProjectDocument, at: i64, frames: i64, name: &str) -> ProjectDocument {
    let revision = RevisionId::new(name).unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision.clone(),
        command: Command::InsertTime {
            at: ProjectFrame(at),
            hold: HoldRecipe {
                picture_context: None,
                duration: duration(frames),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: id(&format!("{name}-pause")),
            identities: SplitIdentities {
                nodes: (0..document.nodes().len() + 4)
                    .map(|index| id(&format!("{name}-split-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision,
                ordinal: 0,
            },
        },
    };
    let before = document.clone();
    let transaction = apply(document, &request).unwrap();
    assert_eq!(transaction.duration_delta, frames);
    assert_eq!(*document, before);
    let changed = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&changed).unwrap(), before);
    changed
}

fn mono_original(frames: i64) -> (ProjectDocument, FixtureProvider) {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let selected = audio_at_rate(100, 20_100, 44_100);
    let mut leaf = source(rate, frames, 0..1);
    let NodeKind::Source { source } = &mut leaf.kind else {
        unreachable!()
    };
    source.audio_mapping = SourceAudioMapping::natural_rate(selected.span, rate).unwrap();
    source.audio = Some(selected);
    let document = document_with_asset(
        rate,
        &["source"],
        [("source", leaf)],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let provider = FixtureProvider::from_fixture(
        "pcm-mono-44100.wav",
        AudioChannelLayout::Native {
            channels: 1,
            mask: 4,
        },
    );
    (document, provider)
}

fn ntsc_boundary(frame: i64) -> usize {
    let samples = ratio(i128::from(frame) * 8008, 5).round_even().unwrap();
    usize::try_from(samples).unwrap()
}

#[derive(Debug, PartialEq)]
struct Recorded {
    samples: Vec<[f32; 2]>,
    suppressed: Vec<bool>,
}

fn record(
    document: &ProjectDocument,
    provider: &mut FixtureProvider,
    faded: bool,
    pieces: &[u32],
) -> Recorded {
    provider.revisions.insert(document.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let total = renderer.plan().audio_duration().unwrap().0 as usize;
    let mut result = Recorded {
        samples: Vec::with_capacity(total),
        suppressed: vec![false; total],
    };
    for requested in pieces.iter().copied().cycle() {
        if result.samples.len() == total {
            break;
        }
        let start = result.samples.len();
        let count = requested.min((total - start) as u32);
        let (samples, suppressed) = if faded {
            let block = renderer
                .read_edge_faded(
                    provider,
                    AudioSample(start as i64),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            (block.samples, block.suppressed)
        } else {
            let block = renderer
                .read(
                    provider,
                    AudioSample(start as i64),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            (block.samples, block.suppressed)
        };
        result.samples.extend(samples);
        for range in &suppressed {
            result.suppressed[range.start.0 as usize..range.end.0 as usize].fill(true);
        }
    }
    result
}

fn assert_pause(recorded: &Recorded, samples: Range<usize>) {
    assert!(
        recorded.samples[samples.clone()]
            .iter()
            .all(|v| *v == [0.; 2])
    );
    assert!(recorded.suppressed[samples].iter().all(|v| *v));
}

fn assert_resumed(actual: &Recorded, old: &Recorded, new_samples: Range<usize>, old_start: usize) {
    for (offset, new_sample) in new_samples.enumerate() {
        let old_sample = old_start + offset;
        assert_eq!(
            actual.samples[new_sample],
            old.samples.get(old_sample).copied().unwrap_or([0.; 2]),
            "new sample {new_sample} must resume old sample {old_sample}"
        );
        assert_eq!(
            actual.suppressed[new_sample],
            old.suppressed.get(old_sample).copied().unwrap_or(true),
            "suppression at new {new_sample}, old {old_sample}"
        );
    }
}

#[test]
fn start_and_end_pauses_retain_hard_source_edges_room_tone_phase_and_silence() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut first = source(rate, 128, 0..128);
    first.audio_edges.node_start = AudioEdgePolicy::Hard;
    first.audio_edges.node_end = AudioEdgePolicy::Hard;
    let original = document_with_asset(
        rate,
        &["a", "room", "quiet", "b"],
        [
            ("a", first),
            ("room", room_tone(128, audio(512, 641))),
            ("quiet", hold(32)),
            ("b", source(rate, 128, 1024..1152)),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let start = insert_pause(&original, 0, 17, "start-pause");
    let end = insert_pause(&start, 433, 13, "end-pause");
    let mut provider = FixtureProvider::new();
    for faded in [false, true] {
        let old = record(&original, &mut provider, faded, &[256]);
        let shifted = record(&start, &mut provider, faded, &[29, 199]);
        assert_pause(&shifted, 0..17);
        assert_resumed(&shifted, &old, 17..433, 0);
        let appended = record(&end, &mut provider, faded, &[37]);
        assert_resumed(&appended, &shifted, 0..433, 0);
        assert_pause(&appended, 433..446);
    }
}

#[test]
fn a_pause_preserves_the_two_sample_creative_envelope_on_both_sides() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = document_with_asset(
        rate,
        &["source"],
        [("source", source(rate, 2, 0..2))],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let inserted = insert_pause(&original, 1, 1, "tiny-pause");
    let mut provider = FixtureProvider::new();
    let old = record(&original, &mut provider, true, &[2]);
    let actual = record(&inserted, &mut provider, true, &[1]);
    assert_eq!(old.samples[0], fixture_sample(0).map(|v| v * 0.5));
    assert_eq!(old.samples[1], fixture_sample(1).map(|v| v * 0.5));
    assert_eq!(
        actual.samples,
        vec![old.samples[0], [0.; 2], old.samples[1]]
    );
    assert_eq!(actual.suppressed, vec![false, true, false]);
}
