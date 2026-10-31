//! Exact checked fixture ports for the preparatory retained-fade OLD witness.
//! Audio ports reconstruct the checked metadata only; no media file/provider is opened.
use super::*;
use deadpan_core::*;
use std::collections::BTreeMap;

fn observe(
    visit: &mut impl FnMut(&Value, &AudioOwnerInput<'_>, &[(&str, i64, i64)]),
    descriptor: Value,
    document: &ProjectDocument,
    name: &str,
    at: i64,
    windows: &[(&str, i64, i64)],
) {
    let plan = RenderPlan::compile(document).unwrap();
    let selected = plan
        .audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
        .unwrap()
        .spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node == NodeId::new(name).unwrap())
        .unwrap()
        .clone();
    let input = plan
        .audio_owner_input(&selected, AudioSample(at), Default::default())
        .unwrap();
    let mut descriptor = descriptor;
    descriptor["document"] = serde_json::to_value(document).unwrap();
    descriptor["selector"] = json!({"node":name,"observed_at":at,"factory":"audio_gain_owners then actual audio_owner_input"});
    visit(&descriptor, &input, windows);
}

pub(in crate::plan) fn visit_cases(
    mut visit: impl FnMut(&Value, &AudioOwnerInput<'_>, &[(&str, i64, i64)]),
) {
    plan_ports::visit(&mut visit);
    audio_ports::visit(&mut visit);
}
mod plan_ports {
    use super::*;
    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }
    fn revision(value: &str) -> RevisionId {
        RevisionId::new(value).unwrap()
    }
    fn ratio(n: i128, d: i128) -> ExactRatio {
        ExactRatio::new(n, d).unwrap()
    }
    fn frames(value: i64) -> FrameDuration {
        FrameDuration::new(value).unwrap()
    }
    fn hold(value: i64) -> BeatNode {
        BeatNode::hold(
            "Voice",
            HoldRecipe {
                duration: frames(value),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
        )
    }
    fn sequence(children: &[&str]) -> BeatNode {
        BeatNode::sequence("Group", children.iter().map(|name| id(name)).collect())
    }
    fn retime(
        child: &str,
        duration: i64,
        range: Range<i64>,
        pitch: PitchPolicy,
        purpose: RetimePurpose,
    ) -> BeatNode {
        let mut node = hold(duration);
        node.kind = NodeKind::Retime {
            child: id(child),
            duration: frames(duration),
            mapping: FrameRange::new(ProjectFrame(range.start), ProjectFrame(range.end)).unwrap(),
            pitch,
            purpose,
        };
        node
    }
    fn document(rate: u32, children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
        document_at(FrameRate::new(rate, 1).unwrap(), children, nodes)
    }
    fn document_at(
        rate: FrameRate,
        children: &[&str],
        nodes: Vec<(&str, BeatNode)>,
    ) -> ProjectDocument {
        let base = ProjectDocument::new(
            ProjectId::new("owner-clocks").unwrap(),
            revision("current"),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: rate,
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let mut nodes: BTreeMap<_, _> = nodes
            .into_iter()
            .map(|(name, node)| (id(name), node))
            .collect();
        nodes.insert(id("root"), sequence(children));
        let mut wire = serde_json::to_value(base).unwrap();
        wire["nodes"] = serde_json::to_value(nodes).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }
    fn bind(
        current: &ProjectDocument,
        old: &ProjectDocument,
        owner: &str,
        clock: AudioClockRoot,
        phase: Option<ExactRatio>,
    ) -> ProjectDocument {
        let timing = AudioTimingId {
            allocation: revision("retained-clock"),
            ordinal: 0,
        };
        let state = AudioBindingState::new(
            vec![AudioTimingRecord {
                id: timing.clone(),
                layout: FrozenAudioLayout::capture(old).unwrap(),
            }],
            BTreeMap::from([(
                id(owner),
                OwnedAudioBinding {
                    reanchors: vec![],
                    lattice: AudioPlacementTemplate {
                        reference_local_offset: deadpan_core::ExactRatio::ZERO,
                        gap_after: None,
                        reference: AudioReferenceClock {
                            recipe: AudioRecipeKind::Node,
                            timing,
                            root: clock,
                            physical: id(owner),
                        },
                        arguments: vec![],
                        births: vec![],
                    },
                    resume: phase.map(|constant| AudioResume {
                        local_boundary: ExactRatio::ZERO,
                        phase: AudioLocalPhase {
                            constant,
                            terms: vec![],
                        },
                    }),
                },
            )]),
        )
        .unwrap();
        let mut wire = serde_json::to_value(current).unwrap();
        wire["audio_bindings"] = serde_json::to_value(state).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }
    pub(super) fn visit(visit: &mut impl FnMut(&Value, &AudioOwnerInput<'_>, &[(&str, i64, i64)])) {
        let nodes = || {
            vec![
                ("a", hold(2)),
                ("b", hold(2)),
                ("group", sequence(&["a", "b"])),
                (
                    "stage",
                    retime("group", 8, 0..4, PitchPolicy::Preserve, RetimePurpose::Edit),
                ),
            ]
        };
        let old = document(48_000, &["stage"], nodes());
        let mut moved = nodes();
        moved.push(("prefix", hold(1)));
        let current = document(48_000, &["prefix", "stage"], moved);
        let bound = bind(
            &current,
            &old,
            "stage",
            AudioClockRoot::ProjectRootRoundEven,
            Some(ratio(3, 1)),
        );
        observe(
            visit,
            json!({"name":"plan/root_retained_descendant",
        "source":{"path":"crates/deadpan-plan/src/audio_owners/input_tests.rs","symbol":"retained_binding_is_not_reconstructed_from_its_nominal_descendant_clock"},
        "old_document":old,"current_document":current}),
            &bound,
            "group",
            1,
            &[
                ("signed_complete", -3, 7),
                ("root_reference_seam", 1, 3),
                ("before_delivery", -2, 0),
            ],
        );
        let old = document(
            32_000,
            &["stage"],
            vec![
                ("prefix", hold(1)),
                ("voice", hold(2)),
                ("group", sequence(&["prefix", "voice"])),
                (
                    "stage",
                    retime("group", 6, 0..3, PitchPolicy::Preserve, RetimePurpose::Edit),
                ),
            ],
        );
        let current = document(32_000, &["voice"], vec![("voice", hold(2))]);
        let bound = bind(
            &current,
            &old,
            "voice",
            AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
            None,
        );
        observe(
            visit,
            json!({"name":"plan/point_retained_labels",
        "source":{"path":"crates/deadpan-plan/src/audio_owners/input_tests.rs","symbol":"moved_preserve_input_keeps_its_point_ceil_reference_labels"},
        "old_document":old,"current_document":current}),
            &bound,
            "voice",
            0,
            &[
                ("point_reference_support", -2, 6),
                ("first", 0, 1),
                ("later", 2, 3),
            ],
        );
    }
}
mod audio_ports {
    use super::*;
    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }
    fn revision(value: &str) -> RevisionId {
        RevisionId::new(value).unwrap()
    }
    fn frames(value: i64) -> FrameDuration {
        FrameDuration::new(value).unwrap()
    }
    fn source(rate: FrameRate, start: i64, end: i64, duration: i64) -> BeatNode {
        let audio = SourceAudio {
            asset: AssetId::new("media").unwrap(),
            span: SourceSpan::new(
                SourceTimestamp {
                    ticks: start,
                    time_base: SourceTimeBase::new(1, 48_000).unwrap(),
                },
                SourceTimestamp {
                    ticks: end,
                    time_base: SourceTimeBase::new(1, 48_000).unwrap(),
                },
            )
            .unwrap(),
        };
        BeatNode {
            framing: None,
            label: "Source".into(),
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Source {
                source: SourceNode {
                    edit_window: None,
                    duration: frames(duration),
                    video: SourceVideo::Blank,
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio_mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
                    audio: Some(audio),
                    audio_offset: AudioSample(0),
                    link: LinkRelation::Independent,
                },
            },
            cutaways: Vec::new(),
            captions: Vec::new(),
        }
    }

    fn hold(duration: i64) -> BeatNode {
        BeatNode::hold(
            "Silence",
            HoldRecipe {
                picture_context: None,
                duration: frames(duration),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        )
    }

    fn preserve(child: &str, selected: i64, duration: i64) -> BeatNode {
        BeatNode {
            framing: None,
            label: "Preserve".into(),
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: id(child),
                duration: frames(duration),
                mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(selected)).unwrap(),
                pitch: PitchPolicy::Preserve,
                purpose: RetimePurpose::Edit,
            },
            cutaways: Vec::new(),
            captions: Vec::new(),
        }
    }

    fn lattice(owner: &str, clock: AudioClockRoot) -> AudioPlacementTemplate {
        AudioPlacementTemplate {
            reference_local_offset: deadpan_core::ExactRatio::ZERO,
            gap_after: None,
            reference: AudioReferenceClock {
                recipe: AudioRecipeKind::Node,
                timing: AudioTimingId {
                    allocation: revision("timing"),
                    ordinal: 0,
                },
                root: clock,
                physical: id(owner),
            },
            arguments: vec![],
            births: vec![],
        }
    }

    fn bind(
        old: &ProjectDocument,
        current: &ProjectDocument,
        bindings: Vec<(&str, AudioPlacementTemplate, Option<AudioResume>)>,
    ) -> ProjectDocument {
        let state = AudioBindingState::new(
            vec![AudioTimingRecord {
                id: AudioTimingId {
                    allocation: revision("timing"),
                    ordinal: 0,
                },
                layout: FrozenAudioLayout::capture(old).unwrap(),
            }],
            bindings
                .into_iter()
                .map(|(owner, lattice, resume)| {
                    (
                        id(owner),
                        OwnedAudioBinding {
                            reanchors: Vec::new(),
                            lattice,
                            resume,
                        },
                    )
                })
                .collect(),
        )
        .unwrap();
        let mut wire = serde_json::to_value(current).unwrap();
        wire["audio_bindings"] = serde_json::to_value(state).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }

    fn metadata_document(
        rate: FrameRate,
        children: &[&str],
        nodes: Vec<(&str, BeatNode)>,
    ) -> ProjectDocument {
        // Exact final metadata from bound_reads::document + controlled_reads::fixture_document_at_rate.
        let base = ProjectDocument::new(
            ProjectId::new("controlled-read").unwrap(),
            revision("revision"),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: rate,
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let mut wire = serde_json::to_value(base).unwrap();
        let mut nodes: BTreeMap<_, _> = nodes
            .into_iter()
            .map(|(name, node)| (id(name), node))
            .collect();
        nodes.insert(
            id("root"),
            BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
        );
        wire["nodes"] = serde_json::to_value(nodes).unwrap();
        let audio = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
            SourceTimestamp {
                ticks: 8197,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
        )
        .unwrap();
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            AssetId::new("media").unwrap(),
            AssetRecord {
                label: "PCM fixture".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(audio),
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        )]))
        .unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }
    pub(super) fn visit(visit: &mut impl FnMut(&Value, &AudioOwnerInput<'_>, &[(&str, i64, i64)])) {
        {
            let rate = FrameRate::new(30_000, 1001).unwrap();
            let nodes = || {
                vec![
                    ("a", source(rate, 0, 3203, 2)),
                    ("b", hold(2)),
                    ("group", BeatNode::sequence("Group", vec![id("a"), id("b")])),
                    ("stage", preserve("group", 4, 8)),
                ]
            };
            let old = metadata_document(rate, &["stage"], nodes());
            let mut moved = nodes();
            moved.push(("prefix", hold(1)));
            let current = metadata_document(rate, &["prefix", "stage"], moved);
            let bound = bind(
                &old,
                &current,
                vec![(
                    "stage",
                    lattice("stage", AudioClockRoot::ProjectRootRoundEven),
                    None,
                )],
            );

            observe(
                visit,
                json!({"name":"audio/root_ntsc_retained_source_and_silence",
        "source":{"path":"crates/deadpan-audio/src/bound_reads/owner_input.rs","symbol":"retained_ntsc_binding_and_silence_keep_the_current_output_policy",
            "metadata_port":"bound_reads::document plus controlled_reads::fixture_document_at_rate; provider/media excluded"},
        "old_document":old,"current_document":current}),
                &bound,
                "group",
                1602,
                &[
                    ("source", 1602, 1730),
                    ("near_source_end", 7900, 8028),
                    ("silence", 8100, 8228),
                    ("late_silence", 12000, 12128),
                ],
            );
        }
        {
            let rate = FrameRate::new(32_000, 1).unwrap();
            let old = metadata_document(
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
            let base = metadata_document(rate, &["a"], vec![("a", source(rate, 0, 600, 400))]);
            let template = || {
                lattice(
                    "a",
                    AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
                )
            };
            let reference = bind(&old, &base, vec![("a", template(), None)]);
            let moved = metadata_document(
                rate,
                &["prefix", "a"],
                vec![("prefix", hold(5)), ("a", source(rate, 0, 600, 400))],
            );
            let moved = bind(
                &old,
                &moved,
                vec![(
                    "a",
                    template(),
                    Some(AudioResume {
                        local_boundary: ExactRatio::ZERO,
                        phase: AudioLocalPhase {
                            constant: ExactRatio::integer(2),
                            terms: vec![],
                        },
                    }),
                )],
            );

            observe(
                visit,
                json!({"name":"audio/point_retained_fractional_source",
        "source":{"path":"crates/deadpan-audio/src/bound_reads/owner_input.rs","symbol":"retained_point_input_fades_keep_fractional_source_phase_before_delivery",
            "metadata_port":"bound_reads::document plus controlled_reads::fixture_document_at_rate; provider/media excluded"},
        "old_document":old,"base_document":base,"reference_document":reference}),
                &moved,
                "a",
                8,
                &[
                    ("source_start", 5, 69),
                    ("early", 69, 133),
                    ("middle", 261, 325),
                    ("near_end", 477, 541),
                    ("after_end", 541, 605),
                ],
            );
        }
        {
            let rate = FrameRate::new(48_000, 1).unwrap();
            let mut voice = source(rate, 0, 8192, 8192);
            let gain = |value| GainDb::new(value).unwrap();
            let range = |start, end| {
                GainRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap()
            };
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
            let old = metadata_document(rate, &["a"], vec![("a", voice.clone())]);
            let current = metadata_document(
                rate,
                &["prefix", "a"],
                vec![("prefix", hold(512)), ("a", voice)],
            );
            let mut wire = serde_json::to_value(&current).unwrap();
            wire["nodes"]["root"]["audio_treatments"] =
                serde_json::to_value(AudioTreatments::from_clip_gain(
                    ClipGain::new(gain(0), true, vec![], vec![]).unwrap(),
                ))
                .unwrap();
            let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
            let bound = bind(
                &old,
                &current,
                vec![(
                    "a",
                    lattice("a", AudioClockRoot::ProjectRootRoundEven),
                    Some(AudioResume {
                        local_boundary: ExactRatio::ZERO,
                        phase: AudioLocalPhase {
                            constant: ExactRatio::integer(2048),
                            terms: vec![],
                        },
                    }),
                )],
            );

            observe(
                visit,
                json!({"name":"audio/root_retained_reference_treatments",
        "source":{"path":"crates/deadpan-audio/src/bound_reads/owner_input.rs","symbol":"retained_treatments_keep_reference_gain_mute_and_order_without_exterior_owners",
            "metadata_port":"bound_reads::document plus controlled_reads::fixture_document_at_rate; provider/media excluded"},
        "old_document":old,"current_document":current}),
                &bound,
                "a",
                512,
                &[
                    ("signed_start", -1536, -1440),
                    ("early", -1504, -1408),
                    ("before_mute", 448, 544),
                    ("mute_start", 504, 600),
                    ("mute_end", 524, 620),
                    ("envelope_end", 2560, 2656),
                    ("source_end", 6560, 6656),
                ],
            );
        }
    }
}
