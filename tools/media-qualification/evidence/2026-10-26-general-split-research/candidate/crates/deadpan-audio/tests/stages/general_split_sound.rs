//! A pure structural Split must preserve every delivered sample, including the
//! common processor's complete input, retained sound clocks and limiter halo.
use super::*;
use deadpan_audio::LimitedAudio;

const PREPARE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug)]
enum Shape {
    Source,
    Hold,
    Sequence,
    Repeat,
    Retime,
}

#[derive(Clone, Copy, Debug)]
enum Parent {
    Sequence,
    Repeat,
    Retime,
}

fn repeat(child: &str, allocation: &str) -> BeatNode {
    let mut value = BeatNode::sequence("Repeated context", vec![]);
    value.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(RevisionId::new(allocation).unwrap(), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    value
}

fn treatment() -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(6000).unwrap(), false, vec![], vec![]).unwrap(),
    )
    .with_equalizer(Some(
        EqualizerRecipe::new(
            [3000, -6000, 1500].map(|db| GainDb::new(db).unwrap()),
            Some(EqCutoffHz::new(100).unwrap()),
            None,
        )
        .unwrap(),
    ))
    .unwrap()
    .with_saturation(Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()))
    .unwrap()
    .with_wet_sends(
        TailSends::default().with_level(TailEffect::Reverb, Some(GainDb::new(-6000).unwrap())),
    )
}

fn attachment(rate: FrameRate, samples: i64) -> BeatSound {
    let source = audio(97, 97 + samples);
    BeatSound {
        label: "Fractional onset sound crossing the split".into(),
        mapping: SourceAudioMapping::Placement {
            start: ratio(1, 7),
            frames: SourceAudioMapping::natural_rate(source.span, rate)
                .unwrap()
                .duration_frames(FrameDuration::ZERO)
                .unwrap(),
        },
        source,
        offset: AudioSample(13),
        treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    }
}

fn fixture(shape: Shape, parent: Parent, processed: bool, root_sound: bool) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut nodes = vec![("lead", hold(1)), ("tail", source(rate, 2, 100..3303))];
    let target = match shape {
        Shape::Source => source(rate, 4, 17..6424),
        Shape::Hold => room_tone(4, audio(1024, 1536)),
        Shape::Sequence => {
            nodes.push(("a", source(rate, 2, 17..3220)));
            nodes.push(("b", source(rate, 2, 3300..6503)));
            BeatNode::sequence("Shared Sequence", vec![id("a"), id("b")])
        }
        Shape::Repeat => {
            nodes.push(("a", source(rate, 2, 17..3220)));
            repeat("a", "owner-plays")
        }
        Shape::Retime => {
            nodes.push(("a", source(rate, 5, 17..8025)));
            retime("a", 4, 0..5, PitchPolicy::Preserve)
        }
    };
    nodes.push(("owner", target));
    let top = match parent {
        Parent::Sequence => "owner",
        Parent::Repeat => {
            nodes.push(("parent", repeat("owner", "parent-plays")));
            "parent"
        }
        Parent::Retime => {
            nodes.push(("parent", retime("owner", 5, 0..4, PitchPolicy::Preserve)));
            "parent"
        }
    };
    let base = document_with_asset(
        rate,
        &["lead", top, "tail"],
        nodes,
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let mut wire = serde_json::to_value(base).unwrap();
    // The core requires a qualified catalog sound. FixtureProvider separately
    // hashes and indexes the actual WAV before it exposes any source PCM.
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
    )
    .unwrap();
    wire["assets"]["media"]["content_hash"] = serde_json::json!(
        Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    wire["beat_sounds"]["owner"]["effect"] = serde_json::to_value(attachment(rate, 5900)).unwrap();
    if matches!(shape, Shape::Sequence | Shape::Repeat | Shape::Retime) {
        wire["beat_sounds"]["a"]["child-effect"] =
            serde_json::to_value(attachment(rate, 2700)).unwrap();
    }
    if root_sound {
        wire["beat_sounds"]["root"]["root-effect"] =
            serde_json::to_value(attachment(rate, 5900)).unwrap();
    }
    if processed {
        // This is one processor over the SUM of original and attached signals.
        // Processing separately and adding would not commute with saturation.
        wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(treatment()).unwrap();
        wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(treatment()).unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn command(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(name).unwrap(),
            command,
        },
    )
    .unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn prefix(document: &ProjectDocument, name: &str) -> ProjectDocument {
    let NodeKind::Hold { recipe } = hold(1).kind else {
        unreachable!()
    };
    command(
        document,
        name,
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: id(name),
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new(name).unwrap(),
                ordinal: 0,
            },
        },
    )
}

fn refine(document: &ProjectDocument, name: &str) -> ProjectDocument {
    let target = document
        .nodes()
        .iter()
        .filter_map(|(node, value)| match value.kind {
            NodeKind::Retime {
                purpose: RetimePurpose::Partition,
                duration,
                mapping,
                ..
            } if duration.frames() > 1 => Some((mapping.start(), node)),
            _ => None,
        })
        .max_by_key(|(start, _)| *start)
        .unwrap()
        .1
        .clone();
    split_command(document, &target, 1, name)
}

#[derive(Debug, PartialEq)]
struct Recording {
    authored: Vec<[f32; 2]>,
    faded: Vec<[f32; 2]>,
    limited: Vec<[f32; 2]>,
    gain: Vec<f64>,
}

struct SplitProvider {
    fixture: FixtureProvider,
    assets: BTreeMap<AssetId, AssetRecord>,
}

impl SplitProvider {
    fn new(document: &ProjectDocument) -> Self {
        let mut fixture = FixtureProvider::new();
        fixture.revisions.insert(document.revision_id().clone());
        let hash = fixture
            .source
            .index()
            .content()
            .sha256()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            document.assets()[&AssetId::new("media").unwrap()].content_hash,
            hash
        );
        Self {
            fixture,
            assets: document.assets().clone(),
        }
    }
}

impl AudioSourceProvider for SplitProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        self.fixture.source(project, revision, asset, cancelled)
    }

    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        // Old captures may name an earlier revision, but they must retain the
        // complete asset contract that this fixture verified against real WAV
        // bytes and the reopened decoder index, not merely reuse an alias.
        assert_eq!(expected, &self.assets[asset]);
        self.fixture.revisions.insert(revision.clone());
        self.source(project, revision, asset, cancelled)
    }
}

fn record(document: &ProjectDocument, pieces: &[u32], master: bool) -> Recording {
    let plan = Arc::new(RenderPlan::compile(document).unwrap());
    let total = plan.audio_duration().unwrap().0;
    let mut provider = SplitProvider::new(document);
    let mut stage = StageAudio::new(Arc::clone(&plan));
    let mut limited = LimitedAudio::new(plan);
    let mut result = Recording {
        authored: vec![],
        faded: vec![],
        limited: vec![],
        gain: vec![],
    };
    let cancelled = AtomicBool::new(false);
    let mut start = 0;
    for count in pieces.iter().copied().cycle() {
        if start == total {
            break;
        }
        let count = count.min(u32::try_from(total - start).unwrap());
        result.authored.extend(
            stage
                .prepare_authored_bus(
                    &mut provider,
                    AudioSample(start),
                    count,
                    PREPARE_TIMEOUT,
                    &cancelled,
                )
                .unwrap()
                .samples,
        );
        result.faded.extend(
            stage
                .prepare_edge_faded(
                    &mut provider,
                    AudioSample(start),
                    count,
                    PREPARE_TIMEOUT,
                    &cancelled,
                )
                .unwrap()
                .samples,
        );
        if master {
            let block = limited
                .read(
                    &mut provider,
                    AudioSample(start),
                    count,
                    PREPARE_TIMEOUT,
                    &cancelled,
                )
                .unwrap();
            result.limited.extend(block.samples);
            result.gain.extend(block.gain);
        }
        start += i64::from(count);
    }
    assert!(result.authored.iter().flatten().any(|v| v.abs() > 0.01));
    result
}

fn assert_preserved(before: &ProjectDocument, after: &ProjectDocument, master: bool, case: &str) {
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    let expected = record(before, &[4096], master);
    assert_recording(&record(after, &[511, 2047, 1009], master), &expected, case);
    // Cold readers use another partition and must still produce precisely the
    // same complete samples, including split boundaries and final tail support.
    assert_recording(&record(after, &[1601, 17, 3001], master), &expected, case);
}

fn assert_recording(actual: &Recording, expected: &Recording, case: &str) {
    for (stage, actual, expected) in [
        ("authored", &actual.authored, &expected.authored),
        ("faded", &actual.faded, &expected.faded),
        ("limited", &actual.limited, &expected.limited),
    ] {
        assert_eq!(actual.len(), expected.len(), "{case}: {stage} length");
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(actual, expected, "{case}: {stage} sample {index}");
        }
    }
    assert_eq!(actual.gain.len(), expected.gain.len());
    for (index, (actual, expected)) in actual.gain.iter().zip(&expected.gain).enumerate() {
        assert_eq!(actual, expected, "{case}: limiter gain {index}");
    }
}

#[test]
fn pristine_sound_bus_is_independent_of_read_partition() {
    let initial = fixture(Shape::Source, Parent::Sequence, true, false);
    let before = prefix(&prefix(&initial, "prefix-one"), "prefix-two");
    let expected = record(&before, &[4096], false);
    assert_recording(
        &record(&before, &[511, 2047, 1009], false),
        &expected,
        "pristine read partition",
    );
}

#[test]
fn split_preserves_sound_bus_at_each_processing_depth() {
    let equalizer = treatment()
        .with_saturation(None)
        .unwrap()
        .with_wet_sends(TailSends::default());
    let saturation = treatment()
        .with_equalizer(None)
        .unwrap()
        .with_wet_sends(TailSends::default());
    let wet = treatment()
        .with_equalizer(None)
        .unwrap()
        .with_saturation(None)
        .unwrap();
    let empty = AudioTreatments::default();
    let full = treatment();
    let mut differences = Vec::new();
    for (case, root, owner) in [
        ("dry", &empty, &empty),
        ("root-only", &full, &empty),
        ("owner-only", &empty, &full),
        ("equalizer", &equalizer, &equalizer),
        ("saturation", &saturation, &saturation),
        ("wet", &wet, &wet),
    ] {
        let initial = fixture(Shape::Source, Parent::Sequence, false, false);
        let mut wire = serde_json::to_value(initial).unwrap();
        wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(root).unwrap();
        wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(owner).unwrap();
        let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let before = prefix(&prefix(&initial, "prefix-one"), "prefix-two");
        let after = split_command(&before, &id("owner"), 2, "cut-one");
        let expected = record(&before, &[4096], false);
        let actual = record(&after, &[511, 2047, 1009], false);
        for (stage, expected, actual) in [
            ("authored", &expected.authored, &actual.authored),
            ("faded", &expected.faded, &actual.faded),
        ] {
            assert_eq!(expected.len(), actual.len(), "{case}: {stage}");
            if let Some(index) = expected.iter().zip(actual).position(|(a, b)| a != b) {
                differences.push((case, stage, index, expected[index], actual[index]));
            }
        }
    }
    assert!(differences.is_empty(), "first differences: {differences:?}");
}

#[test]
fn all_owner_kinds_keep_full_sound_pcm_after_split_and_refinement_with_prior_clocks() {
    for shape in [
        Shape::Source,
        Shape::Hold,
        Shape::Sequence,
        Shape::Repeat,
        Shape::Retime,
    ] {
        for parent in [Parent::Sequence, Parent::Repeat, Parent::Retime] {
            let initial = fixture(shape, parent, true, false);
            let before = prefix(&prefix(&initial, "prefix-one"), "prefix-two");
            let once = split_command(&before, &id("owner"), 2, "cut-one");
            let twice = refine(&once, "cut-two");
            let after = refine(&twice, "cut-three");
            assert_preserved(
                &before,
                &once,
                false,
                &format!("{shape:?}/{parent:?} first Split"),
            );
            assert_preserved(
                &before,
                &after,
                matches!(parent, Parent::Sequence),
                &format!("{shape:?}/{parent:?} refinement"),
            );
        }
    }
}

#[test]
fn root_sound_and_shared_root_processor_survive_child_splits_without_clock_recapture() {
    for processed in [false, true] {
        let before = fixture(Shape::Sequence, Parent::Sequence, processed, true);
        let once = split_command(&before, &id("owner"), 2, "root-sound-cut");
        let after = refine(&once, "root-sound-refinement");
        assert_preserved(
            &before,
            &after,
            true,
            &format!("root sound processed={processed}"),
        );
    }
}

#[test]
fn one_explicit_play_split_keeps_both_selected_and_other_plays_in_the_shared_sound_bus() {
    let initial = fixture(Shape::Sequence, Parent::Repeat, true, false);
    let before = prefix(&prefix(&initial, "prefix-one"), "prefix-two");
    let after = command(
        &before,
        "selected-play-cut",
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("owner"),
                repeats: vec![RepeatInstance {
                    node: id("parent"),
                    iteration: IterationId {
                        allocation: RevisionId::new("parent-plays").unwrap(),
                        ordinal: 1,
                    },
                }],
            },
            edit: OccurrenceEdit::Split {
                at: duration(2),
                identities: SplitIdentities {
                    nodes: (0..16)
                        .map(|n| id(&format!("occurrence-cut-{n}")))
                        .collect(),
                },
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("isolated-owner"), id("isolated-a"), id("isolated-b")],
                marks: vec![],
            },
        },
    );
    assert_preserved(&before, &after, true, "one selected play");
}

#[test]
fn split_witness_has_audible_attached_sound_and_live_shared_processing_and_returns() {
    let before = fixture(Shape::Sequence, Parent::Sequence, true, false);
    let actual = record(&before, &[4096], true);
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["beat_sounds"] = serde_json::json!({});
    let no_sounds = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let no_sounds = record(&no_sounds, &[4096], false);
    assert_ne!(actual.authored, no_sounds.authored);

    let dry = fixture(Shape::Sequence, Parent::Sequence, false, false);
    assert_ne!(actual.authored, record(&dry, &[4096], false).authored);
    // Do not count a merely present wet recipe as coverage. Removing the send
    // while retaining the EQ and nonlinear stage must change delivered PCM.
    let dry_treatment = treatment().with_wet_sends(TailSends::default());
    let mut wire = serde_json::to_value(&before).unwrap();
    for owner in ["owner", "root"] {
        wire["nodes"][owner]["audio_treatments"] = serde_json::to_value(&dry_treatment).unwrap();
    }
    let no_returns = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_ne!(
        actual.authored,
        record(&no_returns, &[4096], false).authored
    );
    assert_eq!(actual.gain.len(), actual.authored.len());
    assert!(
        actual
            .gain
            .iter()
            .all(|gain| gain.is_finite() && *gain > 0.0)
    );
}

#[test]
fn nested_selected_play_split_keeps_root_and_child_sounds_on_their_exact_clocks() {
    for root_sound in [false, true] {
        let initial = fixture(Shape::Sequence, Parent::Repeat, true, root_sound);
        let mut wire = serde_json::to_value(initial).unwrap();
        wire["nodes"]["outer"] = serde_json::to_value(repeat("parent", "outer-plays")).unwrap();
        wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "outer", "tail"]);
        let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
        // Capturing unrelated root timing edits remains unsupported for a root
        // sound. Exercise that branch uncaptured, without excluding its PCM.
        let before = if root_sound {
            initial
        } else {
            prefix(&prefix(&initial, "prefix-one"), "prefix-two")
        };
        let after = command(
            &before,
            "nested-play-cut",
            Command::EditOccurrence {
                instance: InstancePath {
                    node: id("owner"),
                    repeats: [("outer", "outer-plays"), ("parent", "parent-plays")]
                        .map(|(repeat, allocation)| RepeatInstance {
                            node: id(repeat),
                            iteration: IterationId {
                                allocation: RevisionId::new(allocation).unwrap(),
                                ordinal: 1,
                            },
                        })
                        .to_vec(),
                },
                edit: OccurrenceEdit::Split {
                    at: duration(2),
                    identities: SplitIdentities {
                        nodes: (0..32).map(|n| id(&format!("nested-cut-{n}"))).collect(),
                    },
                },
                identities: OccurrenceIdentities {
                    nodes: (0..16)
                        .map(|n| id(&format!("nested-isolation-{n}")))
                        .collect(),
                    marks: vec![],
                },
            },
        );
        assert_preserved(
            &before,
            &after,
            true,
            &format!("nested selected play root_sound={root_sound}"),
        );
    }
}
