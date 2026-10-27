use super::*;
use crate::PreparedSource;
use deadpan_core::*;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{AudioBoundaryRule, AudioRootSource, AudioSoundRoute, AudioSourceVoiceRecipe};
use sha2::{Digest, Sha256};
use std::io::Cursor;

const TIMEOUT: Duration = Duration::from_secs(30);

fn qualified_plan(preserve: bool) -> RenderPlan {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let id = |value| NodeId::new(value).unwrap();
    let clock = SourceTimeBase::new(1, 44_100).unwrap();
    let audio = SourceAudio {
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: 44_100,
                time_base: clock,
            },
        )
        .unwrap(),
    };
    let source = BeatNode {
        framing: None,
        label: "Original speech".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: FrameDuration::new(30).unwrap(),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
                audio: Some(audio.clone()),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let mut nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence(
                "Edit",
                vec![id(if preserve { "preserve" } else { "source" })],
            ),
        ),
        (id("source"), source),
    ]);
    if preserve {
        nodes.insert(
            id("preserve"),
            BeatNode {
                framing: None,
                label: "Full preparation".into(),
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Retime {
                    purpose: RetimePurpose::Edit,
                    child: id("source"),
                    duration: FrameDuration::new(60).unwrap(),
                    mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(30)).unwrap(),
                    pitch: PitchPolicy::Preserve,
                },
                cutaways: Vec::new(),
                captions: Vec::new(),
            },
        );
    }
    let empty = ProjectDocument::new(
        ProjectId::new("sampling-project").unwrap(),
        RevisionId::new("sampling-revision").unwrap(),
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        audio.asset,
        AssetRecord {
            label: "pcm-mono-44100.wav".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio.span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap()
}

struct Provider {
    source: PreparedSource,
    calls: usize,
    revoked: bool,
}

impl Provider {
    fn new() -> Self {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav"),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let session = AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap(),
            0,
            AudioSessionLimits::default(),
            &cancelled,
        )
        .unwrap();
        let index = session.index().clone();
        let source = PreparedSource::with_layout(
            session,
            &index,
            AudioChannelLayout::Native {
                channels: 1,
                mask: 4,
            },
            &cancelled,
        )
        .unwrap();
        Self {
            source,
            calls: 0,
            revoked: false,
        }
    }
}

impl AudioSourceProvider for Provider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        _: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(project.as_str(), "sampling-project");
        assert_eq!(revision.as_str(), "sampling-revision");
        assert_eq!(asset.as_str(), "original");
        self.calls += 1;
        if self.revoked {
            Err(PreparationError::SourceUnavailable(
                "revoked real WAV".into(),
            ))
        } else {
            Ok(&self.source)
        }
    }
}

fn domain(plan: &RenderPlan, occurrence: bool) -> AudioRoutedRoot<'_> {
    let query = plan
        .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let source = if occurrence {
        // The Original may already be a Preserve stage. Its intrinsic source
        // is the same real WAV retained by this independent occurrence.
        let clock = deadpan_core::SourceTimeBase::new(1, 44_100).unwrap();
        SourceAudio {
            asset: AssetId::new("original").unwrap(),
            span: deadpan_core::SourceSpan::new(
                deadpan_core::SourceTimestamp {
                    ticks: 0,
                    time_base: clock,
                },
                deadpan_core::SourceTimestamp {
                    ticks: 44_100,
                    time_base: clock,
                },
            )
            .unwrap(),
        }
    } else {
        let AudioSignalContent::Leaf(AudioContent::Source { source, .. }) = &query.spans[0].content
        else {
            panic!("expected raw source");
        };
        source.clone()
    };
    let rate = plan.metadata().presentation_basis.frame_rate;
    let recipe = AudioSourceVoiceRecipe {
        mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
        source,
        offset: AudioSample(0),
    };
    let step = ExactRatio::new(
        i128::from(rate.numerator()),
        48_000 * i128::from(rate.denominator()),
    )
    .unwrap();
    let (voice, extent) = if occurrence {
        let batch = plan
            .source_voice_occurrences(
                &deadpan_core::NodeId::new("source").unwrap(),
                recipe.clone(),
                AudioSample(0)..AudioSample(256),
                Default::default(),
            )
            .unwrap();
        let voice = batch.voices()[0].clone();
        let extent = voice.extent();
        (Some(voice), extent.end.checked_sub(extent.start).unwrap())
    } else {
        (None, ExactRatio::integer(30))
    };
    let grid = AudioSampleGrid::new(ExactRatio::ZERO, step, AudioBoundaryRule::RoundEven).unwrap();
    let cut = step.checked_mul(ExactRatio::integer(32)).unwrap();
    let route = SoundRoute::identity(extent)
        .unwrap()
        .ripple(
            SoundRippleMap::new(
                extent,
                3,
                vec![
                    SoundRippleNode::Keep {
                        range: deadpan_core::ExactFrameRange {
                            start: ExactRatio::ZERO,
                            end: cut,
                        },
                    },
                    SoundRippleNode::Gap { duration: cut },
                    SoundRippleNode::Keep {
                        range: deadpan_core::ExactFrameRange {
                            start: cut,
                            end: extent,
                        },
                    },
                    SoundRippleNode::Sequence {
                        parts: vec![0, 1, 2],
                    },
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let count = route.nodes().len();
    let route = AudioSoundRoute::<AudioSample>::new(route, vec![grid; count]).unwrap();
    if let Some(voice) = voice {
        AudioRoutedRoot::occurrence(voice, route).unwrap()
    } else {
        AudioRoutedRoot::source(
            AudioRootSource::new(plan, recipe, extent, grid).unwrap(),
            route,
        )
        .unwrap()
    }
}

fn work(control: WorkControl<'_>) -> [u64; 4] {
    let work = control.work.borrow();
    [
        work.plan_work as u64,
        u64::from(work.prepared_stages),
        work.prepared_frames,
        u64::from(work.source_checks),
    ]
}

#[test]
fn borrowed_routed_queries_reuse_real_wav_raw_and_preserve_inputs_exactly() {
    for (preserve, occurrence) in [(false, false), (false, true), (true, true)] {
        let plan = Arc::new(qualified_plan(preserve));
        let domain = domain(&plan, occurrence);
        let mut owned = StageAudio::new(Arc::clone(&plan));
        let mut borrowed = StageAudio::new(Arc::clone(&plan));
        let mut owned_provider = Provider::new();
        let mut borrowed_provider = Provider::new();
        let cancelled = AtomicBool::new(false);
        let owned_budget = PreparationBudget::new(TIMEOUT, &cancelled).unwrap();
        let borrowed_budget = PreparationBudget::new(TIMEOUT, &cancelled).unwrap();
        let prepared = borrowed
            .preflight_routed_root(&domain, AudioSample(16), 96, borrowed_budget.control())
            .unwrap();
        for _ in 0..2 {
            let owned_query = owned
                .preflight_routed_root(&domain, AudioSample(16), 96, owned_budget.control())
                .unwrap();
            let before_owned = work(owned_budget.control());
            let before_borrowed = work(borrowed_budget.control());
            owned.active_frames += 96 * 3;
            borrowed.active_frames += 96 * 3;
            let expected = owned
                .read_routed_root_controlled(
                    &mut owned_provider,
                    owned_query,
                    owned_budget.control(),
                )
                .unwrap();
            let actual = borrowed
                .read_routed_root_borrowed(
                    &mut borrowed_provider,
                    &prepared,
                    borrowed_budget.control(),
                )
                .unwrap();
            owned.active_frames -= 96 * 3;
            borrowed.active_frames -= 96 * 3;
            assert_eq!(actual.start, expected.start);
            assert_eq!(
                actual
                    .samples
                    .iter()
                    .map(|f| f.map(f32::to_bits))
                    .collect::<Vec<_>>(),
                expected
                    .samples
                    .iter()
                    .map(|f| f.map(f32::to_bits))
                    .collect::<Vec<_>>()
            );
            assert!(actual.samples.iter().any(|f| *f != [0.0; 2]));
            assert_eq!(actual.suppressed, expected.suppressed);
            assert!(
                actual
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&AudioSample(48)))
            );
            assert_eq!(actual.exhausted, expected.exhausted);
            assert_eq!(actual.dependencies, expected.dependencies);
            assert_eq!(actual.relative_depth, expected.relative_depth);
            let after_owned = work(owned_budget.control());
            let after_borrowed = work(borrowed_budget.control());
            assert_eq!(
                std::array::from_fn::<_, 4, _>(|i| after_owned[i] - before_owned[i]),
                std::array::from_fn::<_, 4, _>(|i| after_borrowed[i] - before_borrowed[i])
            );
            assert_eq!(owned_provider.calls, borrowed_provider.calls);
            assert_eq!(owned.active_frames, borrowed.active_frames);
        }
        borrowed_provider.revoked = true;
        assert!(matches!(
            borrowed.read_routed_root_borrowed(
                &mut borrowed_provider,
                &prepared,
                borrowed_budget.control()
            ),
            Err(StageAudioError::Preparation(
                PreparationError::SourceUnavailable(_)
            ))
        ));
    }
}
