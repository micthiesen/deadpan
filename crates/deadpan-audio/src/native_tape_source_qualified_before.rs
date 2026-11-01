//! Additive qualified-WAV family; registration is real tempfile fixture setup.
use super::*;
use deadpan_media::audio_index::AudioLayoutInterpretation;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_store::ProjectStore;
use deadpan_store::original_media::{OriginalMediaLimits, OriginalMediaRecord, OriginalOwnership};
use deadpan_store::source_registration::SourceRegistration;

type SetupResult<T> = Result<T, Box<dyn std::error::Error>>;
type NativeProvider = crate::stages::controlled_reads::Provider;

fn media_limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(30)).unwrap()
}
fn open_audio(store: &ProjectStore, record: &OriginalMediaRecord) -> SetupResult<AudioSession> {
    let cancelled = AtomicBool::new(false);
    let snapshot =
        store.snapshot_original(record.object().content(), media_limits(), &cancelled)?;
    let identity = SourceContentIdentity::new(record.sha256(), record.object().byte_length())?;
    Ok(AudioSession::open_input(
        snapshot.into_source_input(identity)?,
        0,
        AudioSessionLimits::default(),
        &cancelled,
    )?)
}
fn native_provider(
    store: &ProjectStore,
    record: &OriginalMediaRecord,
    expected: &deadpan_media::source_qualification::SourceQualificationSnapshot,
) -> SetupResult<NativeProvider> {
    let audio = open_audio(store, record)?;
    let decoded = DecodedSourceQualification::for_registration(
        None,
        Some(&audio),
        Some(AudioLayoutInterpretation::StereoLeftRight),
    )?;
    assert_eq!(decoded.snapshot(), expected);
    let index = audio.index().clone();
    let layout = decoded
        .snapshot()
        .audio_layout()
        .expect("real audio-only receipt has layout");
    Ok(NativeProvider {
        prepared: crate::PreparedSource::with_layout(
            audio,
            &index,
            layout,
            &AtomicBool::new(false),
        )?,
        calls: 0,
    })
}
fn qualified_plan(asset: &deadpan_core::AssetRecord) -> SetupResult<(Arc<RenderPlan>, Value)> {
    let rate = FrameRate::new(48_000, 1)?;
    let (document, _) = crate::stages::controlled_reads::fixture_document_at_rate(rate);
    let mut wire = serde_json::to_value(document)?;
    let duration = |frames| FrameDuration::new(frames).unwrap();
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence("Root", vec![id("stage")]))?;
    wire["nodes"]["room"] = serde_json::to_value(BeatNode::hold(
        "Silent hold",
        HoldRecipe {
            picture_context: None,
            duration: duration(128),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))?;
    wire["nodes"]["b"] = serde_json::to_value(BeatNode::sequence(
        "Preserve input",
        vec![id("a"), id("room")],
    ))?;
    wire["nodes"]["stage"] = serde_json::to_value(BeatNode {
        framing: None,
        label: "Preserve".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("b"),
            duration: duration(768),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(640))?,
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    })?;
    wire["assets"]["media"] = serde_json::to_value(asset)?;
    let checked = ProjectDocument::from_json(&wire.to_string())?;
    Ok((Arc::new(RenderPlan::compile(&checked)?), wire))
}
fn qualified_projection<'a>(
    plan: &'a RenderPlan,
    base: &AudioStageProjection<'a>,
    output: bool,
    fit_beat: bool,
) -> Result<Arc<AudioStageProjection<'a>>, PlanError> {
    let owner = base.stage().input_signal();
    let query = owner.query(
        SignalSample(0)..SignalSample(1),
        AudioQueryLimits::default(),
    )?;
    let source = query
        .spans
        .iter()
        .find_map(|span| match &span.content {
            AudioSignalContent::Leaf(deadpan_plan::AudioContent::Source { source, .. }) => {
                Some(source.clone())
            }
            _ => None,
        })
        .ok_or(PlanError::NoSourceAudio)?;
    let voice = owner.source_voice(deadpan_plan::AudioSourceVoiceRecipe {
        mapping: if fit_beat {
            deadpan_core::SourceAudioMapping::FitBeat
        } else {
            deadpan_core::SourceAudioMapping::natural_rate(
                source.span,
                plan.metadata().presentation_basis.frame_rate,
            )?
        },
        source,
        offset: AudioSample(0),
    })?;
    let signal = if output {
        voice.output_signal()
    } else {
        voice.input_signal()
    };
    let input = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ratio(640, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(640, 1),
            ExactRatio::ZERO..ratio(640, 1),
            signal.clone(),
        )],
    )?;
    let policy = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ratio(768, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(768, 1),
            ExactRatio::ZERO..ratio(640, 1),
            signal,
        )],
    )?;
    AudioStageProjection::new(
        base.stage().clone(),
        input,
        policy,
        FrameDuration::new(768)?,
    )
}
fn family() -> SetupResult<Value> {
    let directory = tempfile::tempdir()?;
    let initial = ProjectDocument::new(
        ProjectId::new("native-tape-source-qualification")?,
        RevisionId::new("initial")?,
        deadpan_core::PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1)?,
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        },
        id("root"),
    )?;
    let mut store = ProjectStore::create(&directory.path().join("fixture.deadpan"), &initial)?;
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()?;
    let cancelled = AtomicBool::new(false);
    let record = store
        .retain_original(
            &path,
            OriginalOwnership::Managed,
            media_limits(),
            &cancelled,
        )?
        .record;
    let audio = open_audio(&store, &record)?;
    let decoded = DecodedSourceQualification::for_registration(
        None,
        Some(&audio),
        Some(AudioLayoutInterpretation::StereoLeftRight),
    )?;
    let outcome = store.register_source(
        &SourceRegistration {
            expected_revision: store.snapshot()?.revision_id().clone(),
            new_revision: RevisionId::new("registered")?,
            original: record.object().content().clone(),
            new_asset_id: deadpan_core::AssetId::new("media")?,
            label: "Real stereo WAV".into(),
            insertion: None,
        },
        &decoded,
        None,
        media_limits(),
        &cancelled,
    )?;
    let receipt = store.source_qualification(&outcome.qualification)?;
    assert_eq!(receipt.id(), &outcome.qualification);
    assert_eq!(receipt.snapshot(), decoded.snapshot());
    assert_eq!(receipt.snapshot().content().sha256(), record.sha256());
    assert_eq!(
        receipt.snapshot().content().byte_length(),
        record.object().byte_length()
    );
    let registered = store.snapshot()?;
    let asset = registered
        .assets()
        .get(&outcome.asset_id)
        .expect("actual register_source asset");
    assert_eq!(asset, &receipt.asset_record("Real stereo WAV".into())?);
    let (plan, wire) = qualified_plan(asset)?;
    let base = projected(&plan);
    let source_index = audio.index().clone();
    let provider = NativeProvider {
        prepared: crate::PreparedSource::with_layout(
            audio,
            &source_index,
            receipt
                .snapshot()
                .audio_layout()
                .expect("registered audio layout"),
            &cancelled,
        )?,
        calls: 0,
    };
    assert_eq!(Some(provider.prepared.index()), receipt.snapshot().audio());
    assert_eq!(
        Some(provider.prepared.matrix_layout()),
        receipt.snapshot().audio_layout()
    );
    let actual_native_contract = json!({
        "index":provider.prepared.index(),"matrix_layout":provider.prepared.matrix_layout(),
        "provenance":provider.prepared.provenance(),
    });
    let mut initial_provider = Some(provider);
    let mut attempts = Vec::new();
    for (output, fit_beat) in [(false, false), (true, false), (true, true)] {
        match qualified_projection(&plan, &base, output, fit_beat) {
            Ok(projection) => {
                let public_empty = public_case_with_provider(
                    &plan,
                    &projection,
                    "qualified_source_empty",
                    0,
                    256,
                    false,
                    match initial_provider.take() {
                        Some(provider) => provider,
                        None => native_provider(&store, &record, receipt.snapshot())?,
                    },
                );
                let public_positive = public_case_with_provider(
                    &plan,
                    &projection,
                    "qualified_source_late_window",
                    512,
                    256,
                    false,
                    native_provider(&store, &record, receipt.snapshot())?,
                );
                let warm = warm_case_with_provider(
                    &plan,
                    &projection,
                    "qualified_source_warm",
                    512,
                    256,
                    "default",
                    native_provider(&store, &record, receipt.snapshot())?,
                );
                attempts.push(json!({"source_output":output,"fit_beat":fit_beat,
                    "setup":"Ok","public_empty":public_empty,"public_late":public_positive,"warm":warm}));
            }
            Err(e) => attempts.push(json!({"source_output":output,"fit_beat":fit_beat,
                "setup":"Error","error":error(&StageAudioError::Plan(e))})),
        }
    }
    Ok(
        json!({"schema":"native-tape-source-qualified-wav-before-v1",
        "voice_scope":"base.stage().input_signal().source_voice, checked public descendant scope",
        "qualification_scope":"real register_source in private tempfile store; receipt retrieved from store; setup outside read ledger",
        "fixture_path":"native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav",
        "original_record":record,"registration":outcome,
        "receipt_id":receipt.id(),"receipt_original":receipt.original(),
        "qualification_snapshot":receipt.snapshot(),
        "qualification_snapshot_canonical_utf8":String::from_utf8(receipt.snapshot().to_json()?)?,
        "registered_asset":asset,"registered_project":serde_json::to_value(&registered)?,
        "checked_native_document_wire":wire,"native_provider":actual_native_contract,
        "attempts":attempts,
        "scope":"public Source projections are real attempts; setup/native errors retained; no whole-preparation ownership claim"}),
    )
}
pub(super) fn capture_family() -> Value {
    match family() {
        Ok(value) => value,
        Err(error) => {
            let mut chain = Vec::new();
            let mut current = error.source();
            while let Some(value) = current {
                chain.push(value.to_string());
                current = value.source();
            }
            json!({"schema":"native-tape-source-qualified-wav-before-v1","setup":"Error",
                "error":{"debug":format!("{error:?}"),"display":error.to_string(),"source_chain":chain},
                "attempts_not_run":"actual store/media/checked-document setup failed"})
        }
    }
}
