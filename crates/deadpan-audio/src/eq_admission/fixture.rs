use super::*;
use deadpan_core::{
    AudioTreatments, BeatNode, ColorPolicy, EqualizerRecipe, GainDb, HoldAudio, HoldRecipe,
    HoldVideo, NodeId, PresentationBasis, ProjectDocument,
};

#[path = "checks.rs"]
pub(in crate::stages) mod checks;

pub(in crate::stages) const BASE: u64 = 17;

pub(in crate::stages) fn units() -> u64 {
    u64::try_from(crate::Equalizer::preparation_bytes_bound())
        .unwrap()
        .div_ceil(8)
}

pub(in crate::stages) fn fixture() -> Arc<RenderPlan> {
    let id = |name: &str| NodeId::new(name).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("eq-admission").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut root = BeatNode::sequence("EQ owner", vec![id("tone")]);
    root.audio_treatments = AudioTreatments::default()
        .with_equalizer(Some(
            EqualizerRecipe::new(
                [3000, -6000, 1500].map(|gain| GainDb::new(gain).unwrap()),
                None,
                None,
            )
            .unwrap(),
        ))
        .unwrap();
    let tone = BeatNode::hold(
        "Tone",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(512).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Tone {
                frequency_hz: 440,
                level: GainDb::new(-6000).unwrap(),
            },
        },
    );
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] =
        serde_json::to_value(BTreeMap::from([(id("root"), root), (id("tone"), tone)])).unwrap();
    Arc::new(RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap())
}

pub(in crate::stages) fn control<'work>(
    cancelled: &'work AtomicBool,
    work: &'work RefCell<ReadWork>,
) -> WorkControl<'work> {
    WorkControl {
        cancelled,
        work,
        deadline: Instant::now() + Duration::from_secs(30),
    }
}

pub(in crate::stages) fn owner(plan: &RenderPlan) -> AudioOwnerClock<'_> {
    let query = plan
        .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    query.spans()[0].owners()[0].clone()
}

pub(in crate::stages) fn stage(owner: &AudioOwnerClock<'_>) -> usize {
    owner
        .treatments()
        .unwrap()
        .order()
        .iter()
        .position(|stage| *stage == AudioTreatmentStage::Equalizer)
        .unwrap()
}

pub(in crate::stages) struct NoSources(pub(in crate::stages) usize);

impl AudioSourceProvider for NoSources {
    fn source(
        &mut self,
        _: &ProjectId,
        _: &RevisionId,
        _: &AssetId,
        _: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.0 += 1;
        Err(PreparationError::SourceUnavailable(
            "EQ tone witness uses no media".into(),
        ))
    }
}
