//! Requested-layout cleanup witness on the supported arm64 macOS toolchain.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_audio::{AudioSourceProvider, PreparationError, PreparedSource, StageAudio};
use deadpan_chaos::{allocation_baseline, allocation_peak_since, allocator_installed};
use deadpan_core::*;
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::RenderPlan;
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

struct Provider {
    prepared: PreparedSource,
    calls: usize,
}

impl AudioSourceProvider for Provider {
    fn source(
        &mut self,
        _: &ProjectId,
        _: &RevisionId,
        asset: &AssetId,
        _: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(asset.as_str(), "media");
        self.calls += 1;
        Ok(&self.prepared)
    }
}

fn fixture() -> (Arc<RenderPlan>, Provider) {
    let bytes = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
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
    let prepared = PreparedSource::with_layout(
        session,
        &index,
        AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        },
        &cancelled,
    )
    .unwrap();
    let rate = FrameRate::new(48_000, 1).unwrap();
    let id = |value: &str| NodeId::new(value).unwrap();
    let audio = |end| SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
            SourceTimestamp {
                ticks: end,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
        )
        .unwrap(),
    };
    let source = audio(512);
    let picture = BeatNode {
        framing: None,
        label: "Original".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: FrameDuration::new(512).unwrap(),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
                audio: Some(source.clone()),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let sound = SoundEvent {
        owner: id("root"),
        label: "Independent trimmed sound".into(),
        mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
        source,
        offset: AudioSample(0),
        treatments: AudioTreatments::default()
            .with_trim(GainDb::new(-6000).unwrap())
            .unwrap(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let document = ProjectDocument::new(
        ProjectId::new("original-cache-cleanup").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("source")])),
        (id("source"), picture),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Real PCM fixture".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio(8197).span),
            still_image: true,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    wire["sounds"] = serde_json::json!({"effect": sound});
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    (plan, Provider { prepared, calls: 0 })
}

#[test]
fn shared_original_cache_releases_actual_controller_allocations_after_each_request() {
    let (plan, mut provider) = fixture();
    let cancelled = AtomicBool::new(false);
    // These predicates select the public shared path. Constant sound trim
    // needs neither a prepared stage nor a persistent sound-processing plan.
    assert!(!plan.sounds().is_empty());
    assert!(plan.has_sound_treatments());
    assert!(!plan.has_audio_equalizers());
    assert!(!plan.has_audio_wet_sends());
    let reference = {
        let mut warm = StageAudio::new(Arc::clone(&plan));
        let output = warm
            .prepare_authored_bus(&mut provider, AudioSample(0), 64, TIMEOUT, &cancelled)
            .unwrap();
        assert_eq!(warm.cached_stage_count(), 0);
        assert_eq!(warm.cached_resident_frames(), 0);
        output.samples
    };
    assert!(reference.iter().any(|sample| *sample != [0.0; 2]));
    assert!(allocator_installed());
    // Its scope Arc belongs to the controller, so construct it before B.
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    for request in 0..2 {
        let calls = provider.calls;
        let before = allocation_baseline();
        let output = renderer
            .prepare_authored_bus(&mut provider, AudioSample(0), 64, TIMEOUT, &cancelled)
            .unwrap();
        let peak = allocation_peak_since(before);
        assert_eq!(output.samples, reference);
        drop(output);
        // prepare_authored_bus has also dropped its request-local budget and
        // ReadWork products before returning. Observe before diagnostics run.
        let after = allocation_baseline();
        assert_eq!(
            after, before,
            "shared request {request} retained actual allocation bytes after cleanup"
        );
        assert!(peak > 0, "the request must exercise real heap allocation");
        assert!(provider.calls > calls, "real sources must be readmitted");
        assert_eq!(renderer.cached_stage_count(), 0);
        assert_eq!(renderer.cached_resident_frames(), 0);
    }
}
