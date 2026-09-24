//! Consumer regressions deliberately change only retained sampling/allocation.
//! They do not model authored Hold insertion or retained nested policy masks.
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

use deadpan_core::*;
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_plan::RenderPlan;

use crate::{PcmWindow, ResampleRecipe, Resampler, StereoMatrix};

pub(crate) fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

pub(crate) fn fixture_plan(preserve: bool) -> RenderPlan {
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
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: FrameDuration::new(30).unwrap(),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
                audio: Some(audio.clone()),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
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
                audio_edges: Default::default(),
                kind: NodeKind::Retime {
                    purpose: RetimePurpose::Edit,
                    child: id("source"),
                    duration: FrameDuration::new(60).unwrap(),
                    mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(30)).unwrap(),
                    pitch: PitchPolicy::Preserve,
                },
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
            label: "Generated PCM".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio.span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap()
}

pub(crate) fn generated_pcm(length: usize) -> Vec<[f32; 2]> {
    (0..length)
        .map(|index| {
            let time = index as f64;
            [
                (0.4 * (time * 0.047).sin() + 0.1 * (time * 0.373).cos()) as f32,
                (0.3 * (time * 0.083).cos() + if index % 317 == 0 { 0.25 } else { 0.0 }) as f32,
            ]
        })
        .collect()
}

pub(crate) fn render(
    input: &[[f32; 2]],
    recipe: &ResampleRecipe,
    start: i64,
    frames: u32,
) -> Vec<[f32; 2]> {
    let sampler = Resampler::new(
        recipe.clone(),
        StereoMatrix::new(AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        })
        .unwrap(),
    );
    let window = sampler
        .required_source_range(AudioSample(start), frames)
        .unwrap()
        .map(|range| PcmWindow {
            start: range.start,
            samples: input[range.start as usize..range.end as usize]
                .iter()
                .flat_map(|sample| *sample)
                .collect(),
        });
    sampler
        .render(AudioSample(start), frames, window, &AtomicBool::new(false))
        .unwrap()
        .samples
}

pub(crate) fn assert_resumed_pcm(
    input: &[[f32; 2]],
    original: &ResampleRecipe,
    resumed: &ResampleRecipe,
    old_start: i64,
    new_start: i64,
) {
    assert_eq!(resumed.selection(), original.selection());
    assert_eq!(resumed.source_step(), original.source_step());
    assert_eq!(
        resumed.source_at(AudioSample(new_start)).unwrap(),
        original.source_at(AudioSample(old_start)).unwrap()
    );
    // Ask for the suffix first, then irregular partitions. Every request retains
    // the same full input/filter support and independently computes its phase.
    let suffix = render(input, resumed, new_start + 173, 83);
    let expected = render(input, original, old_start, 256);
    assert_eq!(suffix, expected[173..]);
    let mut partitioned = Vec::new();
    for (offset, frames) in [(0, 17), (17, 1), (18, 155), (173, 83)] {
        partitioned.extend(render(input, resumed, new_start + offset, frames));
    }
    assert_eq!(partitioned, expected);
    assert_ne!(render(input, original, new_start, 256), expected);
}

#[test]
fn sequence_source_recipe_resumes_current_ntsc_phase_with_full_44100_support() {
    let plan = fixture_plan(false);
    let original = plan
        .audio(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap()
        .spans
        .remove(0);
    let original_recipe = super::source_recipe(&original, 44_100).unwrap().unwrap();
    assert_eq!(original_recipe.selection(), 0..44_100);
    assert_eq!(original_recipe.source_step(), ratio(147, 160));
    let input = generated_pcm(44_100);
    let mut resumed = original.clone();
    for (old_cut, new_anchor, old_pcm) in [(1602, 3203, 1602), (4805, 6406, 3204)] {
        resumed.sampling = resumed
            .sampling
            .resume(AudioSample(old_cut), AudioSample(new_anchor))
            .unwrap();
        resumed.allocated_samples.start = AudioSample(new_anchor);
        resumed.samples = AudioSample(new_anchor)..AudioSample(new_anchor + 256);
        assert_eq!(resumed.transform, original.transform);
        assert_eq!(resumed.content, original.content);
        let recipe = super::source_recipe(&resumed, 44_100).unwrap().unwrap();
        assert_resumed_pcm(&input, &original_recipe, &recipe, old_pcm, new_anchor);
        let mut suffix = resumed.clone();
        suffix.samples.start = AudioSample(new_anchor + 173);
        assert_eq!(
            super::source_recipe(&suffix, 44_100).unwrap().unwrap(),
            recipe
        );
        // A separately requested allocation crop may reanchor the recipe, but
        // must keep the full original filter support and retained phase.
        suffix.allocated_samples.start = suffix.samples.start;
        let cropped = super::source_recipe(&suffix, 44_100).unwrap().unwrap();
        assert_eq!(cropped.selection(), 0..44_100);
        assert_eq!(
            render(&input, &cropped, new_anchor + 173, 83),
            render(&input, &recipe, new_anchor + 173, 83)
        );
    }
    assert_eq!(
        super::source_recipe(&resumed, 44_100)
            .unwrap()
            .unwrap()
            .source_origin(),
        ratio(117747, 40)
    );
}
