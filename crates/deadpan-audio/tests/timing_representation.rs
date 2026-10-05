#![cfg(any(target_os = "macos", target_os = "linux"))]

//! Compact retained timing must be a pure storage change. Random edit
//! sequences run twice from the same document: once with compaction (sliced
//! timing tables, inert reanchor steps omitted, granular binding patches) and
//! once with the complete reference representation. The authored structure,
//! every resolved root binding and the decoded StageAudio bus must agree
//! sample for sample, and the compact document must never be larger.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, LimitedAudio, PreparationError, PreparedSource, StageAudio,
};
use deadpan_core::*;
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::RenderPlan;
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(30);
const MEDIA: Range<i64> = 100..20_100;

struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound.max(1)
    }
    fn range(&mut self, range: Range<i64>) -> i64 {
        range.start + i64::try_from(self.below((range.end - range.start) as u64)).unwrap()
    }
}

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn audio(selected: Range<i64>) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: selected.start,
                time_base,
            },
            SourceTimestamp {
                ticks: selected.end,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn node(label: &str, kind: NodeKind) -> BeatNode {
    BeatNode {
        framing: None,
        label: label.into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind,
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn source(rate: FrameRate, duration: i64, start: i64) -> BeatNode {
    let audio = audio(start..MEDIA.end);
    node(
        "Source",
        NodeKind::Source {
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
    )
}

fn hold(duration: i64, audio: HoldAudio) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: frames(duration),
        video: HoldVideo::Background,
        audio,
    }
}

fn room(random: &mut Lcg) -> HoldAudio {
    let start = random.range(MEDIA.start..MEDIA.end - 4_000);
    HoldAudio::RoomTone {
        source: audio(start..start + random.range(1_500..4_000)),
    }
}

/// A random root Sequence of Sources, room-tone and silent Holds, Preserve
/// retimes and Repeats with gaps, optionally with a root sound.
fn initial(random: &mut Lcg) -> ProjectDocument {
    let rate = match random.below(3) {
        0 => FrameRate::new(30_000, 1001).unwrap(),
        1 => FrameRate::new(24, 1).unwrap(),
        _ => FrameRate::new(25, 1).unwrap(),
    };
    let empty = ProjectDocument::new(
        ProjectId::new("timing-representation").unwrap(),
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
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    let mut total = 0;
    for index in 0..random.range(3..7) {
        let name = format!("c{index}");
        let start = random.range(MEDIA.start..MEDIA.start + 6_000);
        let beat = match random.below(5) {
            0 | 1 => source(rate, random.range(2..9), start),
            2 => BeatNode::hold("Room", hold(random.range(2..6), room(random))),
            3 => {
                let input = random.range(3..8);
                let mut output = random.range(2..10);
                if output == input {
                    output += 1;
                }
                let inner = format!("{name}-input");
                nodes.insert(id(&inner), source(rate, input, start));
                node(
                    "Preserve",
                    NodeKind::Retime {
                        child: id(&inner),
                        duration: frames(output),
                        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
                        pitch: PitchPolicy::Preserve,
                        purpose: RetimePurpose::Edit,
                    },
                )
            }
            _ => BeatNode::hold("Silence", hold(random.range(1..4), HoldAudio::Silence)),
        };
        total += match &beat.kind {
            NodeKind::Source { source } => source.duration.frames(),
            NodeKind::Hold { recipe } => recipe.duration.frames(),
            NodeKind::Retime { duration, .. } => duration.frames(),
            _ => unreachable!("generated root beats"),
        };
        nodes.insert(id(&name), beat);
        children.push(id(&name));
    }
    nodes.insert(id("root"), BeatNode::sequence("Root", children));
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Known 44.1 kHz PCM".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio(0..44_117).span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    if total >= 4 && random.below(3) == 0 {
        let source = audio(0..44_117);
        // Inside the project even after the sub-frame sample offset.
        let selection_end = random.range(3..total.min(12));
        wire["sounds"]["effect"] = serde_json::to_value(SoundEvent {
            owner: id("root"),
            label: "Root sound".into(),
            mapping: SourceAudioMapping::SelectedPlacement {
                start: ExactRatio::ZERO,
                frames: SourceAudioMapping::natural_rate(source.span, rate)
                    .unwrap()
                    .duration_frames(FrameDuration::ZERO)
                    .unwrap(),
                selection: ExactFrameRange::new(
                    ExactRatio::integer(1),
                    ExactRatio::integer(selection_end),
                )
                .unwrap(),
            },
            source,
            offset: AudioSample(random.range(0..800)),
            gain_millidecibels: 0,
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Hard,
            overflow: SoundOverflowPolicy::Reject,
        })
        .unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn root_children(document: &ProjectDocument) -> Vec<NodeId> {
    match &document.nodes()[document.root()].kind {
        NodeKind::Sequence { children } => children.clone(),
        _ => Vec::new(),
    }
}

fn fresh(document: &ProjectDocument, name: &str, count: usize) -> Vec<NodeId> {
    (0..count)
        .map(|index| id(&format!("{name}-n{index}")))
        .filter(|candidate| !document.nodes().contains_key(candidate))
        .collect()
}

/// One random command for `document`, or `None` when the draw is unusable.
fn command(document: &ProjectDocument, random: &mut Lcg, name: &str) -> Option<Command> {
    let total = document.duration().ok()?.frames();
    let children = root_children(document);
    if children.is_empty() || total == 0 {
        return None;
    }
    let timing = AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    };
    let range = |random: &mut Lcg| {
        let start = random.range(0..total);
        let end = random.range(start + 1..total + 1);
        FrameRange::new(ProjectFrame(start), ProjectFrame(end)).ok()
    };
    match random.below(8) {
        6 => {
            let range = range(random)?;
            let needed = document
                .range_deletion(document.root(), range)
                .ok()?
                .required_ids;
            Some(Command::DeleteRange {
                parent: document.root().clone(),
                range,
                identities: SplitIdentities {
                    nodes: fresh(document, name, needed),
                },
                timing,
            })
        }
        7 => {
            let range = range(random)?;
            let at = random.range(0..total + 1);
            let target = document.insert_time_target(ProjectFrame(at)).ok()?;
            let destination = match target.split {
                Some(split) => MoveRangeDestination::Interior {
                    parent: target.parent,
                    target: split.target,
                    at: split.at,
                },
                None => MoveRangeDestination::Seam {
                    parent: target.parent,
                    index: target.index,
                },
            };
            let needed = document
                .range_move(document.root(), range, &destination)
                .ok()?
                .required_ids;
            Some(Command::MoveRange {
                source_revision: document.revision_id().clone(),
                source_parent: document.root().clone(),
                range,
                destination,
                identities: SplitIdentities {
                    nodes: fresh(document, name, needed),
                },
                timing,
            })
        }
        0..=2 => {
            let at = random.range(0..total + 1);
            let target = document.insert_time_target(ProjectFrame(at)).ok()?;
            let needed = target.split.map_or(0, |split| split.required_ids);
            let audio = if random.below(3) == 0 {
                room(random)
            } else {
                HoldAudio::Silence
            };
            Some(Command::InsertTime {
                at: ProjectFrame(at),
                hold: hold(random.range(1..4), audio),
                id: id(&format!("{name}-pause")),
                identities: SplitIdentities {
                    nodes: fresh(document, name, needed),
                },
                timing,
            })
        }
        3 => {
            let child = &children[usize::try_from(random.below(children.len() as u64)).ok()?];
            let length = document.durations().ok()?[child].frames();
            if length < 2 || matches!(document.nodes()[child].kind, NodeKind::Repeat { .. }) {
                return None;
            }
            let mut count = 3;
            let mut pending = vec![child.clone()];
            while let Some(next) = pending.pop() {
                count += 1;
                pending.extend(document.children(&next).cloned());
            }
            Some(Command::Split {
                node: child.clone(),
                at: frames(random.range(1..length)),
                identities: SplitIdentities {
                    nodes: fresh(document, name, count),
                },
            })
        }
        4 => {
            let child = &children[usize::try_from(random.below(children.len() as u64)).ok()?];
            let gap = match random.below(3) {
                0 => None,
                1 => Some(hold(random.range(1..3), HoldAudio::Silence)),
                _ => Some(hold(random.range(1..3), room(random))),
            };
            Some(Command::WrapRepeat {
                node: child.clone(),
                id: id(&format!("{name}-repeat")),
                plays: u32::try_from(random.range(2..4)).ok()?,
                gap,
                anchor_policy: WrapAnchorPolicy::First,
            })
        }
        _ => {
            if children.len() < 2 {
                return None;
            }
            let child = &children[usize::try_from(random.below(children.len() as u64)).ok()?];
            Some(Command::DeleteRipple {
                node: child.clone(),
                timing,
            })
        }
    }
}

struct Provider(PreparedSource);
impl Provider {
    fn new() -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
        Self(
            PreparedSource::with_layout(
                session,
                &index,
                AudioChannelLayout::Native {
                    channels: 1,
                    mask: 4,
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

/// The complete authored bus before the limiter (Original, root and beat
/// voices with edges and gain) and the final limited output, in uneven blocks.
fn bus(
    document: &ProjectDocument,
    provider: &mut Provider,
    limiter: bool,
) -> (Vec<[f32; 2]>, Vec<[f32; 2]>) {
    let plan = Arc::new(RenderPlan::compile(document).unwrap());
    let end = plan.audio_duration().unwrap().0;
    let mut authored = StageAudio::new(Arc::clone(&plan));
    let mut limited = LimitedAudio::new(plan);
    let mut bus = Vec::new();
    let mut output = Vec::new();
    let mut start = 0;
    while start < end {
        let count = u32::try_from((end - start).min(3_001)).unwrap();
        let cancelled = AtomicBool::new(false);
        bus.extend(
            authored
                .prepare_authored_bus(provider, AudioSample(start), count, TIMEOUT, &cancelled)
                .unwrap()
                .samples,
        );
        if limiter {
            output.extend(
                limited
                    .read(provider, AudioSample(start), count, TIMEOUT, &cancelled)
                    .unwrap()
                    .samples,
            );
        }
        start += i64::from(count);
    }
    (bus, output)
}

#[track_caller]
fn assert_bit_exact(name: &str, stage: &str, left: &[[f32; 2]], right: &[[f32; 2]]) {
    assert_eq!(left.len(), right.len(), "{name} {stage}");
    if let Some(at) = left
        .iter()
        .zip(right)
        .position(|(a, b)| a[0].to_bits() != b[0].to_bits() || a[1].to_bits() != b[1].to_bits())
    {
        panic!(
            "{name}: {stage} PCM differs at sample {at}: {:?} vs {:?}",
            left[at], right[at]
        );
    }
}

fn without_bindings(document: &ProjectDocument) -> serde_json::Value {
    let mut value: serde_json::Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("audio_bindings");
    value
}

/// Every bound root-level owner resolves to the same lattice and resume.
fn assert_same_root_resolution(compact: &ProjectDocument, reference: &ProjectDocument) {
    let mut owners: Vec<_> = compact.audio_bindings().bindings().keys().collect();
    owners.sort();
    assert_eq!(
        owners,
        reference
            .audio_bindings()
            .bindings()
            .keys()
            .collect::<Vec<_>>()
    );
    for owner in owners {
        let instance = InstancePath {
            node: owner.clone(),
            repeats: Vec::new(),
        };
        let left = compact
            .audio_bindings()
            .resolve(owner, &instance, MAX_AUDIO_BINDING_ENTRIES);
        let right = reference
            .audio_bindings()
            .resolve(owner, &instance, MAX_AUDIO_BINDING_ENTRIES);
        match (left, right) {
            (Ok(mut left), Ok(mut right)) => {
                // Work counts the retained structure walked, which slicing
                // shortens; the resolved clock must not change.
                left.work = 0;
                right.work = 0;
                left.lattice.work = 0;
                right.lattice.work = 0;
                let effective = |resolved: &ResolvedAudioBinding| {
                    resolved.resume.as_ref().map_or(
                        (resolved.lattice.local_support.start, ExactRatio::ZERO),
                        |resume| (resume.local_boundary, resume.reference_local_delta),
                    )
                };
                assert_eq!(left.lattice, right.lattice, "{owner}");
                assert_eq!(effective(&left), effective(&right), "{owner}");
            }
            // Owners under a Repeat need their live play arguments.
            (Err(left), Err(right)) => assert_eq!(left.code, right.code),
            (left, right) => panic!("{owner}: {left:?} vs {right:?}"),
        }
    }
}

#[test]
fn compact_timing_matches_the_reference_representation_sample_for_sample() {
    let mut provider = Provider::new();
    let mut committed = 0;
    let mut sliced = 0;
    let mut audible = 0;
    let mut kinds = BTreeMap::<String, (usize, usize)>::new();
    // DEADPAN_TIMING_SEEDS widens the search; qualification used 300 seeds
    // in a release build. Four keep the debug suite near a minute.
    let seeds: u64 = std::env::var("DEADPAN_TIMING_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    for seed in 0..seeds {
        let mut random = Lcg::new(seed);
        let start = initial(&mut random);
        let mut compact = start.clone();
        let mut reference = start;
        let mut undo: Vec<(EditTransaction, EditTransaction)> = Vec::new();
        for step in 0..10 {
            let name = format!("s{seed}-e{step}");
            if !undo.is_empty() && random.below(5) == 0 {
                let (left, right) = undo.pop().unwrap();
                let undo_revision = revision(&format!("{name}-undo"));
                let left = left
                    .inverse
                    .rebased(compact.revision_id().clone(), undo_revision.clone());
                let right = right
                    .inverse
                    .rebased(reference.revision_id().clone(), undo_revision);
                compact = left.apply(&compact).unwrap();
                reference = right.apply(&reference).unwrap();
            } else {
                let Some(command) = command(&compact, &mut random, &name) else {
                    continue;
                };
                let kind = format!("{command:?}")
                    .split([' ', '{'])
                    .next()
                    .unwrap()
                    .to_owned();
                let request = CommandRequest {
                    project_id: compact.project_id().clone(),
                    expected_revision: compact.revision_id().clone(),
                    new_revision: revision(&name),
                    command,
                };
                let left = apply(&compact, &request);
                let right = with_reference_timing_representation(|| apply(&reference, &request));
                let (left, right) = match (left, right) {
                    (Ok(left), Ok(right)) => (left, right),
                    (Err(left), Err(right)) => {
                        assert_eq!(left.code, right.code, "{name}: {left:?} / {right:?}");
                        kinds.entry(kind).or_insert((0, 0)).1 += 1;
                        continue;
                    }
                    (left, right) => panic!("{name}: {left:?} vs {right:?}"),
                };
                // The granular patch survives its bounded wire and restores
                // the exact previous revision.
                let wire = serde_json::to_string(&left).unwrap();
                assert_eq!(
                    serde_json::from_str::<EditTransaction>(&wire).unwrap(),
                    left
                );
                let next = left.forward.apply(&compact).unwrap();
                assert_eq!(left.inverse.apply(&next).unwrap(), compact);
                let reference_next = right.forward.apply(&reference).unwrap();
                committed += 1;
                kinds.entry(kind).or_insert((0, 0)).0 += 1;
                compact = next;
                reference = reference_next;
                undo.push((left, right));
            }
            assert_eq!(without_bindings(&compact), without_bindings(&reference));
            assert!(
                compact.to_json().unwrap().len() <= reference.to_json().unwrap().len(),
                "{name}: compact representation is larger"
            );
            sliced += usize::from(compact.audio_bindings() != reference.audio_bindings());
            assert_same_root_resolution(&compact, &reference);
            // The limiter is costly in debug builds; check it on final states.
            let limiter = step == 9;
            let (left_bus, left_output) = bus(&compact, &mut provider, limiter);
            let (right_bus, right_output) = bus(&reference, &mut provider, limiter);
            assert_bit_exact(&name, "authored bus", &left_bus, &right_bus);
            assert_bit_exact(&name, "limited output", &left_output, &right_output);
            audible += usize::from(left_bus.iter().any(|sample| sample[0] != 0.));
        }
    }
    // The sequences must exercise both commits and genuinely smaller state.
    eprintln!("{committed} commits, {sliced} compacted states, {audible} audible");
    eprintln!("committed and refused by command: {kinds:?}");
    let seeds = usize::try_from(seeds).unwrap();
    assert!(committed >= 4 * seeds, "only {committed} commits");
    assert!(sliced >= 2 * seeds, "only {sliced} compacted states");
    assert!(audible >= 4 * seeds, "only {audible} audible comparisons");
}
