#![cfg(any(target_os = "macos", target_os = "linux"))]

//! Derived Original audio timing (brief S3). Edit scripts on a one-Original
//! project at 30000/1001 and 24 fps render twice: as stored, and with every
//! owner and Repeat-gap binding removed (sound clocks kept). The renderer
//! derives sample placement from the current tree, so both renders must agree
//! within the accepted loss of at most one sample. Lengths must be identical
//! and the derived render must add no clicks. After a fractional-delay
//! alignment, segments must reach correlation 0.9999 with a residual below
//! -60 dBFS, or, inside a pitch-preserving speed change, correlation 0.98.
//!
//! Plain Original segments are also compared with the unedited Original: an
//! edit must play the same material within one sample of its exact position.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_audio::{AudioSourceProvider, PreparationError, PreparedSource, StageAudio};
use deadpan_core::*;
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{AudioQueryLimits, RenderPlan};
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(60);
const SECONDS: i64 = 4;
/// After fractional alignment, outside pitch-preserving speed changes.
const MIN_CORRELATION: f64 = 0.9999;
/// Inside a Preserve or Shift speed change: the same input at a different
/// sub-sample offset is a different but equivalent rendering of the stretch.
const MIN_STRETCHED_CORRELATION: f64 = 0.98;
/// -60 dBFS root-mean-square residual.
const MAX_RESIDUAL: f64 = 1e-3;
/// A new sample-to-sample step this much larger than the stored render's
/// local maximum counts as a click (about -26 dBFS).
const CLICK_STEP: f32 = 0.05;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn asset() -> AssetId {
    AssetId::new("original").unwrap()
}

#[derive(Clone, Copy, Debug)]
struct Setup {
    rate: (u32, u32),
    clock: u32,
}
impl Setup {
    fn rate(self) -> FrameRate {
        FrameRate::new(self.rate.0, self.rate.1).unwrap()
    }
    fn samples(self) -> i64 {
        SECONDS * i64::from(self.clock)
    }
    fn audio_span(self) -> SourceSpan {
        let base = SourceTimeBase::new(1, self.clock).unwrap();
        SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: base,
            },
            SourceTimestamp {
                ticks: self.samples(),
                time_base: base,
            },
        )
        .unwrap()
    }
    /// Whole pictures inside the audio, as a measured import would find.
    fn pictures(self) -> i64 {
        SECONDS * i64::from(self.rate.0) / i64::from(self.rate.1)
    }
    fn picture_span(self) -> SourceSpan {
        let base = SourceTimeBase::new(1, self.rate.0).unwrap();
        SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: base,
            },
            SourceTimestamp {
                ticks: self.pictures() * i64::from(self.rate.1),
                time_base: base,
            },
        )
        .unwrap()
    }
}

/// Speech-like stereo: a gliding harmonic voice with formants, syllable
/// envelopes and breath noise. Content matters: a fractional-sample phase
/// change is invisible on a ramp but measurable on real speech bandwidth.
fn speech(setup: Setup) -> Vec<u8> {
    let rate = f64::from(setup.clock);
    let count = usize::try_from(setup.samples()).unwrap();
    let mut noise = 0x2545_f491_4f6c_dd1d_u64;
    let mut filtered = 0.0f64;
    let mut phase = 0.0f64;
    let mut pcm = Vec::with_capacity(count * 4);
    for n in 0..count {
        let t = n as f64 / rate;
        let f0 = 140.0 + 30.0 * (2.0 * std::f64::consts::PI * 0.7 * t).sin();
        phase += f0 / rate;
        let mut voice = 0.0;
        for k in 1..=30 {
            let frequency = f0 * f64::from(k);
            if frequency > 6_000.0 {
                break;
            }
            let formant = (-((frequency - 700.0) / 400.0).powi(2)).exp()
                + 0.6 * (-((frequency - 1_200.0) / 500.0).powi(2)).exp()
                + 0.3 * (-((frequency - 2_600.0) / 700.0).powi(2)).exp();
            voice += formant / f64::from(k).sqrt()
                * (2.0 * std::f64::consts::PI * phase * f64::from(k)).sin();
        }
        let syllable = (2.0 * std::f64::consts::PI * 3.7 * t)
            .sin()
            .max(0.0)
            .powf(0.6);
        noise ^= noise << 13;
        noise ^= noise >> 7;
        noise ^= noise << 17;
        let white = (noise >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
        filtered = 0.85 * filtered + 0.15 * white;
        let left = 0.18 * voice * syllable + 0.02 * filtered;
        let right = 0.16 * voice * syllable - 0.02 * filtered;
        for value in [left, right] {
            let sample = (value.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
            pcm.extend_from_slice(&sample.to_le_bytes());
        }
    }
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&u32::try_from(36 + pcm.len()).unwrap().to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&setup.clock.to_le_bytes());
    wav.extend_from_slice(&(setup.clock * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&u32::try_from(pcm.len()).unwrap().to_le_bytes());
    wav.extend_from_slice(&pcm);
    wav
}

struct Provider(PreparedSource);
impl Provider {
    fn new(setup: Setup) -> Self {
        let bytes = speech(setup);
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
        Self(
            PreparedSource::with_layout(
                session,
                &index,
                AudioChannelLayout::Native {
                    channels: 2,
                    mask: 3,
                },
                &cancelled,
            )
            .unwrap(),
        )
    }
}
impl AudioSourceProvider for Provider {
    fn source(
        &mut self,
        _: &ProjectId,
        _: &RevisionId,
        _: &AssetId,
        _: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        Ok(&self.0)
    }
}

/// The whole measured Original as one linked Source, as import creates it.
fn original(setup: Setup) -> ProjectDocument {
    let rate = setup.rate();
    let audio_frames = ratio(
        i128::from(setup.samples()) * i128::from(setup.rate.0),
        i128::from(setup.clock) * i128::from(setup.rate.1),
    );
    let duration =
        (audio_frames.numerator() + audio_frames.denominator() - 1) / audio_frames.denominator();
    let source = SourceNode {
        edit_window: Some(SourceEditWindow::new(ExactRatio::ZERO, audio_frames).unwrap()),
        duration: frames(i64::try_from(duration).unwrap()),
        video: SourceVideo::Stream {
            asset: asset(),
            span: setup.picture_span(),
        },
        video_mapping: SourceVideoMapping::Placement {
            start: ExactRatio::ZERO,
            frames: ExactRatio::integer(setup.pictures()),
            endpoints: EndpointPolicy::HoldAdjacent,
        },
        audio: Some(SourceAudio {
            asset: asset(),
            span: setup.audio_span(),
        }),
        audio_mapping: SourceAudioMapping::Placement {
            start: ExactRatio::ZERO,
            frames: audio_frames,
        },
        audio_offset: AudioSample(0),
        link: LinkRelation::Linked,
    };
    let empty = ProjectDocument::new(
        ProjectId::new("derived-timing").unwrap(),
        revision("r0"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("original")])),
        (
            id("original"),
            BeatNode {
                framing: None,
                label: "Original".into(),
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Source { source },
                cutaways: Vec::new(),
                captions: Vec::new(),
            },
        ),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(setup.picture_span()),
            audio: Some(setup.audio_span()),
            still_image: false,
            frame_count: Some(frames(setup.pictures())),
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

struct Editor {
    setup: Setup,
    document: ProjectDocument,
    step: usize,
    name: String,
}

impl Editor {
    fn new(setup: Setup, name: &str) -> Self {
        Self {
            setup,
            document: original(setup),
            step: 0,
            name: name.into(),
        }
    }
    fn next_revision(&self) -> RevisionId {
        revision(&format!("{}-{}", self.name, self.step + 1))
    }
    fn timing(&self) -> AudioTimingId {
        AudioTimingId {
            allocation: self.next_revision(),
            ordinal: 0,
        }
    }
    fn fresh(&self, count: usize) -> SplitIdentities {
        SplitIdentities {
            nodes: (0..count)
                .map(|index| id(&format!("{}-{}-n{index}", self.name, self.step + 1)))
                .collect(),
        }
    }
    fn fresh_id(&self, label: &str) -> NodeId {
        id(&format!("{}-{}-{label}", self.name, self.step + 1))
    }
    #[track_caller]
    fn run(&mut self, command: Command) {
        let request = CommandRequest {
            project_id: self.document.project_id().clone(),
            expected_revision: self.document.revision_id().clone(),
            new_revision: self.next_revision(),
            command,
        };
        let transaction = apply(&self.document, &request)
            .unwrap_or_else(|error| panic!("{} step {}: {error:?}", self.name, self.step + 1));
        self.document = transaction.forward.apply(&self.document).unwrap();
        self.step += 1;
    }
    fn root_children(&self) -> Vec<NodeId> {
        match &self.document.nodes()[self.document.root()].kind {
            NodeKind::Sequence { children } => children.clone(),
            _ => unreachable!("root sequence"),
        }
    }
    /// The root child containing `frame` and its start.
    fn child_at(&self, frame: i64) -> (NodeId, i64) {
        let durations = self.document.durations().unwrap();
        let mut start = 0;
        for child in self.root_children() {
            let length = durations[&child].frames();
            if frame < start + length {
                return (child, start);
            }
            start += length;
        }
        panic!("{frame} is outside the project")
    }

    fn pause(&mut self, at: i64, length: i64, audio: HoldAudio) {
        let target = self.document.insert_time_target(ProjectFrame(at)).unwrap();
        let needed = target.split.map_or(0, |split| split.required_ids) + 1;
        self.run(Command::InsertTime {
            at: ProjectFrame(at),
            hold: hold(length, audio),
            id: self.fresh_id("pause"),
            identities: self.fresh(needed),
            timing: self.timing(),
        });
    }
    fn split(&mut self, at: i64) -> NodeId {
        let (node, start) = self.child_at(at);
        let mut count = 3;
        let mut pending = vec![node.clone()];
        while let Some(next) = pending.pop() {
            count += 1;
            pending.extend(self.document.children(&next).cloned());
        }
        self.run(Command::Split {
            node,
            at: frames(at - start),
            identities: self.fresh(count),
        });
        self.child_at(at).0
    }
    fn delete_range(&mut self, range: Range<i64>) {
        let range = FrameRange::new(ProjectFrame(range.start), ProjectFrame(range.end)).unwrap();
        let needed = self
            .document
            .range_deletion(self.document.root(), range)
            .unwrap()
            .required_ids;
        self.run(Command::DeleteRange {
            parent: self.document.root().clone(),
            range,
            identities: self.fresh(needed),
            timing: self.timing(),
        });
    }
    fn delete_child(&mut self, at: i64) {
        let node = self.child_at(at).0;
        self.run(Command::DeleteRipple {
            node,
            timing: self.timing(),
        });
    }
    fn trim(&mut self, at: i64, edge: SourceTrimEdge, delta: i64) {
        let node = self.child_at(at).0;
        let root = self.document.root().clone();
        let resolved = self
            .document
            .source_trim(&root, &node, edge, delta, SourceTrimMode::Ripple)
            .unwrap();
        assert_eq!(resolved.applied_delta_frames, delta, "{}", self.name);
        let wrapper = resolved.needs_wrapper.then(|| self.fresh_id("crop"));
        self.run(Command::TrimSource {
            parent: root,
            node,
            edge,
            delta_frames: delta,
            mode: SourceTrimMode::Ripple,
            wrapper,
            timing: self.timing(),
        });
    }
    fn slip(&mut self, at: i64, delta: i64) {
        let node = self.child_at(at).0;
        let root = self.document.root().clone();
        self.document.source_slip(&root, &node, delta).unwrap();
        self.run(Command::SlipSource {
            parent: root,
            node,
            delta_frames: delta,
        });
    }
    fn roll(&mut self, seam: i64, delta: i64) {
        let left = self.child_at(seam - 1).0;
        let right = self.child_at(seam).0;
        let root = self.document.root().clone();
        let resolution = self
            .document
            .source_roll(&root, &left, &right, delta)
            .unwrap();
        let left_wrapper = resolution
            .left
            .needs_wrapper
            .then(|| self.fresh_id("lcrop"));
        let right_wrapper = resolution
            .right
            .needs_wrapper
            .then(|| self.fresh_id("rcrop"));
        self.run(Command::RollSources {
            parent: root,
            left,
            right,
            delta_frames: delta,
            left_wrapper,
            right_wrapper,
            timing: self.timing(),
        });
    }
    fn wrap_retime(&mut self, at: i64, duration: i64, pitch: PitchPolicy) {
        let node = self.child_at(at).0;
        self.run(Command::WrapRetime {
            node,
            id: self.fresh_id("speed"),
            duration: frames(duration),
            pitch,
        });
    }
    fn wrap_repeat(&mut self, at: i64, plays: u32, gap: Option<HoldRecipe>) -> NodeId {
        let node = self.child_at(at).0;
        let repeat = self.fresh_id("repeat");
        self.run(Command::WrapRepeat {
            node,
            id: repeat.clone(),
            plays,
            gap,
            anchor_policy: WrapAnchorPolicy::First,
        });
        repeat
    }
    fn set_plays(&mut self, repeat: &NodeId, plays: u32) {
        self.run(Command::SetRepeatPlays {
            node: repeat.clone(),
            plays,
            timing: self.timing(),
        });
    }
    fn room_tone(&self, start_seconds: f64, seconds: f64) -> HoldAudio {
        let clock = f64::from(self.setup.clock);
        let base = SourceTimeBase::new(1, self.setup.clock).unwrap();
        let start = (start_seconds * clock) as i64;
        let end = ((start_seconds + seconds) * clock) as i64;
        HoldAudio::RoomTone {
            source: SourceAudio {
                asset: asset(),
                span: SourceSpan::new(
                    SourceTimestamp {
                        ticks: start,
                        time_base: base,
                    },
                    SourceTimestamp {
                        ticks: end,
                        time_base: base,
                    },
                )
                .unwrap(),
            },
        }
    }
}

fn hold(length: i64, audio: HoldAudio) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: frames(length),
        video: HoldVideo::Background,
        audio,
    }
}

/// Frames for one second, so scripts read the same at both rates.
fn second(setup: Setup) -> i64 {
    i64::from(setup.rate.0) / i64::from(setup.rate.1)
}

type Script = fn(&mut Editor);

fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        ("pause", |e| {
            let s = second(e.setup);
            e.pause(s + 3, 4, HoldAudio::Silence);
            e.pause(2 * s + 7, 1, HoldAudio::Silence);
            e.pause(s / 2, 9, HoldAudio::Silence);
        }),
        ("room-tone-pause", |e| {
            let s = second(e.setup);
            let tone = e.room_tone(0.37, 0.4);
            e.pause(s + 5, 13, tone);
            let tone = e.room_tone(2.11, 0.09);
            e.pause(3 * s, 7, tone);
        }),
        ("delete", |e| {
            let s = second(e.setup);
            e.delete_range(s + 2..s + 9);
            e.split(2 * s + 1);
            e.split(2 * s + 6);
            e.delete_child(2 * s + 3);
            e.delete_range(5..6);
        }),
        ("split-inside-preserve", |e| {
            let s = second(e.setup);
            e.split(s);
            e.split(2 * s);
            e.wrap_retime(s, s * 3 / 2 + 1, PitchPolicy::Preserve);
            e.pause(s + s / 2 + 1, 5, HoldAudio::Silence);
            e.pause(s / 3, 2, HoldAudio::Silence);
        }),
        ("trim", |e| {
            let s = second(e.setup);
            e.split(s + 1);
            e.split(2 * s + 2);
            e.trim(s + 4, SourceTrimEdge::In, 5);
            e.trim(s + 4, SourceTrimEdge::Out, -3);
            e.trim(3, SourceTrimEdge::Out, -2);
        }),
        ("slip", |e| {
            let s = second(e.setup);
            e.split(s + 1);
            e.split(2 * s + 2);
            e.trim(s + 4, SourceTrimEdge::In, 6);
            e.trim(s + 4, SourceTrimEdge::Out, -6);
            e.slip(s + 4, -3);
            e.slip(s + 4, 5);
        }),
        ("roll", |e| {
            let s = second(e.setup);
            e.split(s + 1);
            e.split(2 * s + 2);
            e.trim(s + 4, SourceTrimEdge::Out, -5);
            e.roll(s + 1, 3);
            e.roll(s + 4, -1);
        }),
        ("repeat-gap", |e| {
            let s = second(e.setup);
            e.split(s + 2);
            e.split(s + 11);
            let repeat = e.wrap_repeat(s + 4, 3, Some(hold(2, HoldAudio::Silence)));
            e.pause(s / 2 + 1, 3, HoldAudio::Silence);
            e.set_plays(&repeat, 4);
            let tone = e.room_tone(1.5, 0.2);
            e.split(3 * s);
            e.wrap_repeat(3 * s + 1, 2, Some(hold(3, tone)));
        }),
        ("mixed", |e| {
            let s = second(e.setup);
            e.pause(s + 1, 2, HoldAudio::Silence);
            e.split(2 * s);
            e.wrap_retime(2 * s + 1, s / 2, PitchPolicy::Preserve);
            e.delete_range(s / 3..s / 3 + 4);
            let tone = e.room_tone(0.8, 0.3);
            e.pause(s / 2, 5, tone);
            e.split(3 * s / 2);
            e.wrap_repeat(3 * s / 2 + 1, 2, Some(hold(1, HoldAudio::Silence)));
        }),
    ]
}

/// The stored document with every owner and Repeat-gap binding removed.
/// Timing tables and sound clocks stay.
fn without_bindings(document: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    if let Some(bindings) = wire
        .get_mut("audio_bindings")
        .and_then(|v| v.as_object_mut())
    {
        bindings.insert("bindings".into(), serde_json::json!({}));
        bindings.remove("gap_bindings");
        // Only sound clocks may still name a timing table.
        let clocks = bindings
            .get("sound_clocks")
            .map_or_else(String::new, |clocks| clocks.to_string());
        if let Some(serde_json::Value::Array(timings)) = bindings.get_mut("timings") {
            timings.retain(|timing| clocks.contains(&timing["id"].to_string()));
        }
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

/// A rendered segment: one query span, flagged when a pitch-preserving
/// speed change (Preserve or Shift) processes it.
struct Segment {
    samples: Range<usize>,
    stretched: bool,
    /// For plain Original audio, the exact position of `samples.start` in
    /// the unedited Original's 48 kHz render.
    original: Option<f64>,
}

fn render(document: &ProjectDocument, provider: &mut Provider) -> (Vec<[f32; 2]>, Vec<Segment>) {
    let plan = Arc::new(RenderPlan::compile(document).unwrap());
    let end = plan.audio_duration().unwrap().0;
    let mut segments = Vec::new();
    let mut at = 0;
    while at < end {
        let query = plan
            .audio(
                AudioSample(at)..AudioSample(end),
                AudioQueryLimits::default(),
            )
            .unwrap();
        for span in &query.spans {
            segments.push(Segment {
                samples: usize::try_from(span.samples.start.0).unwrap()
                    ..usize::try_from(span.samples.end.0).unwrap(),
                stretched: span.retimes.iter().any(|stage| {
                    matches!(
                        stage.pitch,
                        PitchPolicy::Preserve | PitchPolicy::Shift { .. }
                    )
                }),
                original: match &span.content {
                    deadpan_plan::AudioContent::Source { .. } if span.retimes.is_empty() => {
                        let point = span.source_point(span.samples.start).unwrap();
                        let ticks =
                            point.ticks.numerator() as f64 / point.ticks.denominator() as f64;
                        Some(
                            ticks * 48_000.0
                                / serde_json::to_value(point.time_base).unwrap()["denominator"]
                                    .as_f64()
                                    .unwrap(),
                        )
                    }
                    _ => None,
                },
            });
        }
        at = query.samples.end.0.max(at + 1);
    }
    let mut audio = StageAudio::new(plan);
    let mut bus = Vec::new();
    let mut start = 0;
    while start < end {
        let count = u32::try_from((end - start).min(8_192)).unwrap();
        let cancelled = AtomicBool::new(false);
        bus.extend(
            audio
                .prepare_authored_bus(provider, AudioSample(start), count, TIMEOUT, &cancelled)
                .unwrap()
                .samples,
        );
        start += i64::from(count);
    }
    (bus, segments)
}

#[derive(Debug, Default, Clone)]
struct Score {
    segments: usize,
    differing: usize,
    worst_correlation: f64,
    worst_residual_db: f64,
    /// Largest fractional shift found by alignment.
    worst_shift: f64,
    worst_click: f32,
    failures: Vec<String>,
}

fn rms(values: impl Iterator<Item = f64>) -> f64 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for value in values {
        sum += value * value;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        (sum / count as f64).sqrt()
    }
}

/// Best fractional alignment in [-1, 1] samples: the derived render delayed
/// by a windowed-sinc fractional delay, over at most 16,384 interior samples.
/// A pure sub-sample shift aligns to a correlation near 1.
fn aligned(stored: &[[f32; 2]], derived: &[[f32; 2]], range: Range<usize>) -> (f64, f64, f64) {
    let middle = (range.start + range.end) / 2;
    let half = (range.len() / 2)
        .saturating_sub(TAPS as usize + 2)
        .min(8_192);
    aligned_inner(stored, 0, derived, middle - half..middle + half)
}

const TAPS: isize = 16;

/// Align `derived[n]` to `reference[n + offset]` over `inner`.
fn aligned_inner(
    stored: &[[f32; 2]],
    offset: isize,
    derived: &[[f32; 2]],
    inner: Range<usize>,
) -> (f64, f64, f64) {
    let evaluate = |shift: f64| {
        let (mut ab, mut aa, mut bb, mut residual) = (0.0, 0.0, 0.0, 0.0);
        let mut count = 0usize;
        for n in inner.clone() {
            for c in 0..2 {
                let mut value = 0.0;
                for k in -TAPS..=TAPS {
                    let x = k as f64 - shift;
                    let sinc = if x == 0.0 {
                        1.0
                    } else {
                        (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                    };
                    let window = 0.5 + 0.5 * (std::f64::consts::PI * x / (TAPS as f64 + 1.0)).cos();
                    value +=
                        f64::from(derived[n.checked_add_signed(k).unwrap()][c]) * sinc * window;
                }
                let a = f64::from(stored[n.checked_add_signed(offset).unwrap()][c]);
                ab += a * value;
                aa += a * a;
                bb += value * value;
                residual += (a - value) * (a - value);
                count += 1;
            }
        }
        let correlation = if aa == 0.0 || bb == 0.0 {
            0.0
        } else {
            ab / (aa * bb).sqrt()
        };
        (correlation, (residual / count.max(1) as f64).sqrt())
    };
    let mut best = (0.0, -1.0, f64::INFINITY);
    let search = |shift: f64, best: &mut (f64, f64, f64)| {
        let (correlation, residual) = evaluate(shift);
        if correlation > best.1 {
            *best = (shift, correlation, residual);
        }
    };
    for step in -20..=20 {
        search(f64::from(step) * 0.05, &mut best);
    }
    let centre = best.0;
    for step in -10..=10 {
        search(centre + f64::from(step) * 0.005, &mut best);
    }
    best
}

/// Plain Original segments of an edited render must be the Original itself,
/// shifted by at most one sample. Interiors only: edge fades are authored.
fn against_original(
    name: &str,
    original: &[[f32; 2]],
    derived: &[[f32; 2]],
    segments: &[Segment],
) -> Score {
    const EDGE: usize = 160;
    let mut score = Score {
        worst_correlation: 1.0,
        worst_residual_db: f64::NEG_INFINITY,
        ..Score::default()
    };
    for segment in segments {
        let Some(position) = segment.original else {
            continue;
        };
        let range = segment.samples.clone();
        let exact = position - range.start as f64;
        let offset = exact.round() as isize;
        let margin = EDGE + TAPS as usize + 2;
        let start = range.start + margin;
        let end = range
            .end
            .saturating_sub(margin)
            .min((original.len() as isize - offset).max(0) as usize)
            .min(start + 16_384);
        if end <= start + 256 {
            continue;
        }
        score.segments += 1;
        let (shift, correlation, residual) = aligned_inner(original, offset, derived, start..end);
        // The derived sample n plays Original position n + exact; alignment
        // found it at n + offset - shift.
        let deviation = offset as f64 - shift - exact;
        score.worst_correlation = score.worst_correlation.min(correlation);
        score.worst_residual_db = score.worst_residual_db.max(db(residual));
        if deviation.abs() > score.worst_shift.abs() {
            score.worst_shift = deviation;
        }
        if correlation < MIN_CORRELATION || residual >= MAX_RESIDUAL || deviation.abs() > 1.0 {
            score.failures.push(format!(
                "{name}: segment {range:?} against the Original at {position:.3}: \
                 deviation {deviation:+.3} correlation {correlation:.7} residual {:.1} dB",
                db(residual)
            ));
        }
    }
    score
}

fn compare(name: &str, stored: &[[f32; 2]], derived: &[[f32; 2]], segments: &[Segment]) -> Score {
    assert_eq!(stored.len(), derived.len(), "{name}: length");
    let mut score = Score {
        worst_correlation: 1.0,
        worst_residual_db: f64::NEG_INFINITY,
        ..Score::default()
    };
    for segment in segments {
        let range = segment.samples.clone();
        if range.is_empty() {
            continue;
        }
        score.segments += 1;
        if stored[range.clone()] == derived[range.clone()] {
            continue;
        }
        score.differing += 1;
        let kind = if segment.stretched { "stretched " } else { "" };
        if range.len() < 64 {
            // Too short to align: the closest whole-sample lag must agree.
            let residual = [-1isize, 0, 1]
                .into_iter()
                .map(|lag| {
                    rms(range.clone().flat_map(|n| {
                        let m = n.checked_add_signed(lag).filter(|m| *m < derived.len());
                        (0..2).map(move |c| {
                            f64::from(stored[n][c]) - m.map_or(0.0, |m| f64::from(derived[m][c]))
                        })
                    }))
                })
                .fold(f64::INFINITY, f64::min);
            score.worst_residual_db = score.worst_residual_db.max(db(residual));
            if !segment.stretched && residual >= MAX_RESIDUAL {
                score.failures.push(format!(
                    "short {kind}segment {range:?} residual {:.1} dB",
                    db(residual)
                ));
            }
            continue;
        }
        let (shift, correlation, residual) = aligned(stored, derived, range.clone());
        score.worst_correlation = score.worst_correlation.min(correlation);
        score.worst_residual_db = score.worst_residual_db.max(db(residual));
        if shift.abs() > score.worst_shift.abs() {
            score.worst_shift = shift;
        }
        let passes = if segment.stretched {
            correlation >= MIN_STRETCHED_CORRELATION
        } else {
            // A silent stretch has no defined correlation; -100 dBFS is exact.
            (correlation >= MIN_CORRELATION || residual < 1e-5) && residual < MAX_RESIDUAL
        };
        if !passes {
            score.failures.push(format!(
                "{kind}segment {range:?} ({} samples) aligned at {shift:+.3}: \
                 correlation {correlation:.7} residual {:.1} dB",
                range.len(),
                db(residual)
            ));
        }
    }
    let step = |values: &[[f32; 2]], n: usize, c: usize| (values[n][c] - values[n - 1][c]).abs();
    for n in 1..stored.len() {
        for c in 0..2 {
            let local = (n.saturating_sub(2).max(1)..(n + 3).min(stored.len()))
                .map(|m| step(stored, m, c))
                .fold(0.0f32, f32::max);
            let excess = step(derived, n, c) - local;
            score.worst_click = score.worst_click.max(excess);
            if excess >= CLICK_STEP {
                score.failures.push(format!(
                    "new click at sample {n} channel {c}: step exceeds stored by {excess}"
                ));
            }
        }
    }
    score
}

fn db(value: f64) -> f64 {
    if value == 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * value.log10()
    }
}

#[test]
fn derived_original_audio_stays_within_one_sample_of_the_original() {
    let setups = [
        Setup {
            rate: (30_000, 1001),
            clock: 48_000,
        },
        Setup {
            rate: (24, 1),
            clock: 48_000,
        },
        Setup {
            rate: (30_000, 1001),
            clock: 44_100,
        },
    ];
    let mut report = Vec::new();
    for setup in setups {
        let mut provider = Provider::new(setup);
        let (original_pcm, _) = render(&original(setup), &mut provider);
        for (script, run) in scripts() {
            let name = format!("{script}@{}/{}-{}", setup.rate.0, setup.rate.1, setup.clock);
            let mut editor = Editor::new(setup, script);
            run(&mut editor);
            let stored = &editor.document;
            let derived = without_bindings(stored);
            let bound = !stored.audio_bindings().bindings().is_empty()
                || !stored.audio_bindings().gap_bindings().is_empty();
            let (stored_pcm, _) = render(stored, &mut provider);
            let (derived_pcm, segments) = render(&derived, &mut provider);
            let score = compare(&name, &stored_pcm, &derived_pcm, &segments);
            let edited = against_original(&name, &original_pcm, &derived_pcm, &segments);
            report.push((name.clone(), bound, score));
            report.push((format!("{name} vs Original"), bound, edited));
        }
    }
    let mut failures = 0;
    for (name, bound, score) in &report {
        eprintln!(
            "{name:38} bound={bound:5} segments={:3} differing={:3} \
             worst aligned corr={:.7} residual={:.1} dB shift={:+.3} click excess={:.4}",
            score.segments,
            score.differing,
            score.worst_correlation,
            score.worst_residual_db,
            score.worst_shift,
            score.worst_click
        );
        for failure in &score.failures {
            eprintln!("    FAIL {failure}");
            failures += 1;
        }
    }
    assert_eq!(failures, 0, "segments outside the one-sample tolerance");
}
