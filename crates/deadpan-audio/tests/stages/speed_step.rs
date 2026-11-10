//! `:repeat … speed-step=` PCM: each later play is its own Retime override,
//! so its sound is the canonical tape resampling or Preserve stretch of the
//! play's whole input at that play's exact rate, starting where it starts.
use super::*;

const PLAYS: &str = "plays";

fn sped_repeat(pitch: PitchPolicy) -> ProjectDocument {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let repeat = BeatNode {
        label: "Faster each time".into(),
        framing: None,
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        audio_treatments: Default::default(),
        kind: NodeKind::Repeat {
            child: id("source"),
            iterations: IterationOrder::new(RevisionId::new(PLAYS).unwrap(), 3).unwrap(),
            gap: None,
            escalation: None,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let mut document = document_with_asset(
        rate,
        &["repeat"],
        [("source", source(rate, 512, 512..1024)), ("repeat", repeat)],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let step = SpeedStep {
        step: ratio(1, 2),
        multiply: false,
        preserve_pitch: pitch == PitchPolicy::Preserve,
    };
    // The same leaves the semantic planner authors: the recipe, then one
    // occurrence-isolated Retime per later play.
    let mut commands = vec![Command::SetRepeatEscalation {
        node: id("repeat"),
        escalation: Some(RepeatEscalation {
            gain_step: GainDb::UNITY,
            zoom: None,
            speed: Some(step),
        }),
    }];
    for play in 1..3 {
        commands.push(Command::EditOccurrence {
            instance: InstancePath {
                node: id("source"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: IterationId {
                        allocation: RevisionId::new(PLAYS).unwrap(),
                        ordinal: play,
                    },
                }],
            },
            edit: OccurrenceEdit::WrapRetime {
                id: id(&format!("speed-{play}")),
                duration: step.duration(play, duration(512)).unwrap(),
                pitch,
            },
            identities: OccurrenceIdentities {
                nodes: vec![id(&format!("copy-{play}"))],
                marks: Vec::new(),
            },
        });
    }
    for (index, command) in commands.into_iter().enumerate() {
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(format!("speed-step-{index}")).unwrap(),
            command,
        };
        let edit = apply(&document, &request).unwrap();
        let next = edit.forward.apply(&document).unwrap();
        assert_eq!(edit.inverse.apply(&next).unwrap(), document);
        document = next;
    }
    document
}

fn heard(document: &ProjectDocument) -> Vec<[f32; 2]> {
    let mut provider = FixtureProvider::new();
    provider.revisions.insert(document.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let total = usize::try_from(renderer.plan().audio_duration().unwrap().0).unwrap();
    let mut output = Vec::new();
    // Irregular reads cross every play boundary at a different offset.
    for requested in [97_u32, 256, 13].into_iter().cycle() {
        if output.len() == total {
            return output;
        }
        let count = requested.min(u32::try_from(total - output.len()).unwrap());
        let start = AudioSample(i64::try_from(output.len()).unwrap());
        output.extend(
            renderer
                .read(
                    &mut provider,
                    start,
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    unreachable!()
}

/// Speeds 1, 1.5 and 2 over 512 input frames: 512, 341 (341.33 rounded
/// once) and 256 output frames, one 48 kHz sample per frame.
const LENGTHS: [u32; 3] = [512, 341, 256];

#[test]
fn a_tape_speed_step_resamples_each_play_at_its_own_exact_rate() {
    let input = || fixture_sample;
    let expected: Vec<_> = LENGTHS
        .iter()
        .flat_map(|&frames| {
            sample_reference(
                512..1024,
                ExactRatio::integer(512),
                ratio(512, i128::from(frames)),
                frames,
                input(),
            )
        })
        .collect();
    let actual = heard(&sped_repeat(PitchPolicy::FollowSpeed));
    assert_eq!(actual.len(), 512 + 341 + 256);
    assert_eq!(actual, expected);
    // The later plays really are faster: not the first play's samples.
    assert_ne!(actual[512..768], actual[..256]);
}

#[test]
fn a_preserve_speed_step_stretches_each_play_at_its_own_exact_rate() {
    let input: Vec<_> = (512..1024).map(fixture_sample).collect();
    let mut expected = sample_reference(
        512..1024,
        ExactRatio::integer(512),
        ExactRatio::ONE,
        512,
        fixture_sample,
    );
    for frames in &LENGTHS[1..] {
        expected.extend(stretch_reference(&input, *frames, 512, u64::from(*frames)));
    }
    let actual = heard(&sped_repeat(PitchPolicy::Preserve));
    assert_eq!(actual, expected);
    assert_ne!(actual, heard(&sped_repeat(PitchPolicy::FollowSpeed)));
}
