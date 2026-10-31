use super::*;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use std::io::Cursor;

#[derive(Clone, Copy)]
pub(super) enum Wav {
    Stereo48000,
    Mono44100,
}
pub(super) struct Fixture {
    pub name: &'static str,
    pub checked_source: &'static str,
    pub current: ProjectDocument,
    pub reference: ProjectDocument,
    pub owner: &'static str,
    pub observed_at: i64,
    pub windows: Vec<(i64, u32)>,
    pub wav: Wav,
}
fn path(wav: Wav) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures")
        .join(match wav {
            Wav::Stereo48000 => "pcm-stereo-48000.wav",
            Wav::Mono44100 => "pcm-mono-44100.wav",
        })
}
pub(super) fn prepared(wav: Wav) -> crate::PreparedSource {
    let bytes = std::fs::read(path(wav)).unwrap();
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
    let (channels, mask) = match wav {
        Wav::Stereo48000 => (2, 3),
        Wav::Mono44100 => (1, 4),
    };
    crate::PreparedSource::with_layout(
        session,
        &index,
        AudioChannelLayout::Native { channels, mask },
        &cancelled,
    )
    .unwrap()
}
pub(super) fn wav_descriptor(wav: Wav) -> Value {
    use std::fmt::Write as _;

    let path = path(wav);
    let bytes = std::fs::read(&path).unwrap();
    let mut sha256 = String::with_capacity(64);
    for byte in Sha256::digest(&bytes) {
        write!(sha256, "{byte:02x}").unwrap();
    }
    json!({
        "repository_relative_path": format!("native/deadpan-source/tests/audio-fixtures/{}", path.file_name().unwrap().to_str().unwrap()),
        "sha256": sha256,
        "bytes": bytes.len(),
        "adapter": "AudioSession::open_verified -> PreparedSource::with_layout",
        "stream": 0,
    })
}
fn resume(phase: i64) -> Option<AudioResume> {
    Some(AudioResume {
        local_boundary: ExactRatio::ZERO,
        phase: AudioLocalPhase {
            constant: ExactRatio::integer(phase),
            terms: vec![],
        },
    })
}
fn root() -> AudioClockRoot {
    AudioClockRoot::ProjectRootRoundEven
}
fn root_resumed() -> Fixture {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (old, _) = document(rate, &["a"], vec![("a", source(rate, 0, 8192, 8192))]);
    let (current, _) = document(
        rate,
        &["prefix", "a"],
        vec![("prefix", hold(512)), ("a", source(rate, 0, 8192, 8192))],
    );
    let current = bind(
        &old,
        &current,
        vec![("a", lattice("a", root()), resume(2048))],
    );
    Fixture {
        name: "root-resumed-signed-history",
        checked_source: "bound_reads/owner_input.rs::resumed_owner_exposes_readable_reference_history_before_current_delivery",
        current,
        reference: old,
        owner: "a",
        observed_at: 512,
        windows: [-16, 0, 32, 8064, 8170, 8192]
            .into_iter()
            .map(|x| (x - 1536, 128))
            .collect(),
        wav: Wav::Stereo48000,
    }
}
fn point() -> Fixture {
    let rate = FrameRate::new(32_000, 1).unwrap();
    let (old, _) = document(
        rate,
        &["stage"],
        vec![
            ("prefix", hold(1)),
            ("a", source(rate, 0, 600, 400)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("prefix"), id("a")]),
            ),
            ("stage", preserve("group", 401, 802)),
        ],
    );
    let (base, _) = document(rate, &["a"], vec![("a", source(rate, 0, 600, 400))]);
    let template = || {
        lattice(
            "a",
            AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
        )
    };
    let reference = bind(&old, &base, vec![("a", template(), None)]);
    let (moved, _) = document(
        rate,
        &["prefix", "a"],
        vec![("prefix", hold(5)), ("a", source(rate, 0, 600, 400))],
    );
    let current = bind(&old, &moved, vec![("a", template(), resume(2))]);
    Fixture {
        name: "point-fractional-phase-before-delivery",
        checked_source: "bound_reads/owner_input.rs::retained_point_input_fades_keep_fractional_source_phase_before_delivery",
        current,
        reference,
        owner: "a",
        observed_at: 8,
        windows: [0, 64, 256, 472, 536]
            .into_iter()
            .map(|x| (x + 5, 64))
            .collect(),
        wav: Wav::Stereo48000,
    }
}
fn treatments() -> Fixture {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut voice = source(rate, 0, 8192, 8192);
    let gain = |value| GainDb::new(value).unwrap();
    let range =
        |start, end| GainRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap();
    voice.audio_treatments = AudioTreatments::with_all_stages(
        vec![
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::ClipGain,
            AudioTreatmentStage::StereoBalance,
        ],
        Some(
            ClipGain::new(
                gain(6000),
                false,
                vec![
                    GainEnvelope::new(
                        GainClock::OwnerOutput,
                        range(1000, 4096),
                        gain(-3000),
                        vec![
                            GainSegment::new(
                                ExactRatio::integer(4096),
                                gain(6000),
                                GainCurve::Linear,
                            )
                            .unwrap(),
                        ],
                    )
                    .unwrap(),
                ],
                vec![range(2040, 2070)],
            )
            .unwrap(),
        ),
        Some(Saturation::new(gain(9000)).unwrap()),
        Some(StereoBalance::new(-25).unwrap()),
    )
    .unwrap();
    let (old, _) = document(rate, &["a"], vec![("a", voice.clone())]);
    let (current, _) = document(
        rate,
        &["prefix", "a"],
        vec![("prefix", hold(512)), ("a", voice)],
    );
    let mut wire = serde_json::to_value(&current).unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::from_clip_gain(ClipGain::new(gain(0), true, vec![], vec![]).unwrap()),
    )
    .unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let current = bind(
        &old,
        &current,
        vec![("a", lattice("a", root()), resume(2048))],
    );
    Fixture {
        name: "root-reference-treatments-and-masks",
        checked_source: "bound_reads/owner_input.rs::retained_treatments_keep_reference_gain_mute_and_order_without_exterior_owners",
        current,
        reference: old,
        owner: "a",
        observed_at: 512,
        windows: [0, 32, 1984, 2040, 2060, 4096, 8096]
            .into_iter()
            .map(|x| (x - 1536, 96))
            .collect(),
        wav: Wav::Stereo48000,
    }
}
fn ntsc() -> Fixture {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let nodes = || {
        vec![
            ("a", source(rate, 0, 3203, 2)),
            ("b", hold(2)),
            ("group", BeatNode::sequence("Group", vec![id("a"), id("b")])),
            ("stage", preserve("group", 4, 8)),
        ]
    };
    let (old, _) = document(rate, &["stage"], nodes());
    let mut moved = nodes();
    moved.push(("prefix", hold(1)));
    let (current, _) = document(rate, &["prefix", "stage"], moved);
    let current = bind(
        &old,
        &current,
        vec![("stage", lattice("stage", root()), None)],
    );
    Fixture {
        name: "root-ntsc-enclosing-and-silence",
        checked_source: "bound_reads/owner_input.rs::retained_ntsc_binding_and_silence_keep_the_current_output_policy",
        current,
        reference: old,
        owner: "group",
        observed_at: 1602,
        windows: [1602, 7900, 8100, 12000]
            .into_iter()
            .map(|x| (x, 128))
            .collect(),
        wav: Wav::Stereo48000,
    }
}
fn qualified_44100(children: &[&str], prefix: bool) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
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
    let node = BeatNode {
        framing: None,
        label: "Original speech".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(30),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
                audio: Some(audio.clone()),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
        cutaways: vec![],
        captions: vec![],
    };
    let empty = ProjectDocument::new(
        ProjectId::new("sampling-project").unwrap(),
        revision("sampling-revision"),
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence("Edit", children.iter().map(|x| id(x)).collect()),
        ),
        (id("source"), node),
    ]);
    if prefix {
        nodes.insert(id("prefix"), hold(1));
    }
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
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn fractional_44100() -> Fixture {
    let old = qualified_44100(&["source"], false);
    let moved = qualified_44100(&["prefix", "source"], true);
    let current = bind(
        &old,
        &moved,
        vec![("source", lattice("source", root()), None)],
    );
    Fixture {
        name: "root-44100-ntsc-reference-fractional-sampling",
        checked_source: "routed/tests.rs::qualified_plan(false) and Provider::new; root move/bind pattern from bound_reads/owner_input.rs; combined fixture, no assumed expected output",
        current,
        reference: old,
        owner: "source",
        observed_at: 1602,
        windows: [-32, 0, 128, 47936, 48000, 48048]
            .into_iter()
            .map(|x| (x + 1602, 128))
            .collect(),
        wav: Wav::Mono44100,
    }
}
pub(super) fn fixtures() -> Vec<Fixture> {
    vec![
        root_resumed(),
        point(),
        treatments(),
        ntsc(),
        fractional_44100(),
    ]
}
