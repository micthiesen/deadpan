//! The frozen picture a new pause shows: the picture before its boundary,
//! with the composition below the pause's own Sequence captured. Shared by the
//! native `,h` edit and macro pauses so both author the same Hold.

use std::sync::Arc;

use deadpan_core::{
    AssetId, AudioSample, CapturedCanvas, CapturedFit, CapturedFraming, ExactRatio, FrameDuration,
    HoldAudio, HoldVideo, NodeId, PauseProvider, PauseSite, ProjectDocument, ProjectFrame,
    SourceAudio, SourceFrameIndex, SourceSpan, SourceTimestamp,
};
use deadpan_plan::{AudioContent, AudioQueryLimits, Picture, PictureSample, RenderPlan};

/// The longest stretch a pause reverses, bounded by the 1,048,576-sample
/// input limit of Hold effect preparation (about 21.8 s).
const MAX_REVERSE_SECONDS: i64 = 20;

type IndexLookup<'a> = dyn FnMut(&AssetId) -> Result<Arc<SourceFrameIndex>, String> + 'a;

/// Resolve the picture a pause at `site` holds.
pub fn site_provider(
    document: &ProjectDocument,
    plan: &RenderPlan,
    site: &PauseSite,
    index: &mut IndexLookup<'_>,
) -> Result<PauseProvider, String> {
    match site {
        PauseSite::Boundary { at } => pause_provider(document, plan, *at, index),
        PauseSite::RepeatGap { repeat, frame } => {
            gap_provider(document, plan, repeat, *frame, index)
        }
        PauseSite::Reverse { at, frames, bounce } => {
            reverse_provider(document, plan, *at, *frames, *bounce, index)
        }
        PauseSite::Bleep {
            at,
            frames,
            frequency_hz,
            level,
        } => bleep_provider(document, plan, *at, *frames, *frequency_hz, *level, index),
    }
}

/// The picture a gap of `repeat` holds: its play's picture at `frame`, with
/// the composition below the Repeat captured. The Repeat's own framing and
/// escalation, and everything above it, stay live on the gap.
pub fn gap_provider(
    document: &ProjectDocument,
    plan: &RenderPlan,
    repeat: &NodeId,
    frame: ProjectFrame,
    index: &mut IndexLookup<'_>,
) -> Result<PauseProvider, String> {
    let sample = plan.picture(frame).map_err(|error| error.to_string())?;
    let scope = sample
        .framing
        .iter()
        .position(|scope| &scope.instance.node == repeat)
        .ok_or("The play's picture is not inside the selected Repeat.")?;
    freeze(document, &sample, scope, index)
}

/// Resolve the provider for a pause at `at` in `document`, compiled as `plan`.
/// `index` supplies the measured picture index of a shown asset.
pub fn pause_provider(
    document: &ProjectDocument,
    plan: &RenderPlan,
    at: ProjectFrame,
    index: &mut IndexLookup<'_>,
) -> Result<PauseProvider, String> {
    if plan.duration() == FrameDuration::ZERO {
        return Ok(PauseProvider {
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        });
    }
    let insertion_parent = document
        .insert_time_target(at)
        .map_err(|error| error.to_string())?
        .parent;
    let sample = plan
        .picture(ProjectFrame(if at.0 == 0 { 0 } else { at.0 - 1 }))
        .map_err(|error| error.to_string())?;
    match &sample.picture {
        Picture::Source { .. } | Picture::Freeze { .. } => {
            // The new Hold is a child of the selected Sequence. Retain only
            // composition below that parent. The parent and its ancestors
            // stay live on the Hold and must not be captured a second time.
            let parent = sample
                .framing
                .iter()
                .position(|scope| {
                    scope.instance.node == insertion_parent && scope.instance.repeats.is_empty()
                })
                .ok_or("The stopped picture has no selected Sequence scope.")?;
            freeze(document, &sample, parent, index)
        }
        _ => freeze(document, &sample, 0, index),
    }
}

/// Freeze `sample`, capturing its framing scopes below `scope`.
fn freeze(
    document: &ProjectDocument,
    sample: &PictureSample,
    scope: usize,
    index: &mut IndexLookup<'_>,
) -> Result<PauseProvider, String> {
    let picture = &sample.picture;
    match picture {
        Picture::Source { asset, .. } | Picture::Freeze { asset, .. } => {
            let index = index(asset)?;
            let selected = picture
                .select_source_frame(&index)
                .map_err(|error| error.to_string())?;
            let lower = &sample.framing[..scope];
            // Even an unframed view retains its canvas and letterboxing. Fitting
            // the raw source directly into a later canvas is not equivalent to
            // fitting the already composed view into that canvas.
            let mut layers =
                Vec::with_capacity(lower.len() + usize::from(sample.gap_after.is_some()));
            if sample.gap_after.is_some() {
                layers.push(None);
            }
            layers.extend(lower.iter().map(|layer| layer.pose));
            let basis = document.presentation_basis();
            let picture_context = Some(
                CapturedFraming::capture(
                    sample.picture_context.as_deref(),
                    CapturedCanvas {
                        width: basis.width,
                        height: basis.height,
                        fit: CapturedFit::Fit,
                        layers,
                    },
                )
                .map_err(|error| error.to_string())?,
            );
            Ok(PauseProvider {
                video: HoldVideo::Freeze {
                    asset: asset.clone(),
                    timestamp: SourceTimestamp {
                        ticks: selected.pts,
                        time_base: index.time_base(),
                    },
                },
                picture_context,
                audio: HoldAudio::Silence,
            })
        }
        Picture::Blank | Picture::Background => Ok(PauseProvider {
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        }),
        Picture::Still { .. } | Picture::Accepted { .. } => Err(
            "Freezing still or accepted generated footage for a new pause is not available yet."
                .into(),
        ),
    }
}

/// A pause at `at` that plays the `frames` before it backwards: the exact
/// measured pictures of one continuous Original passage, played from its end,
/// and the sound heard under them reversed. With `bounce` the last picture is
/// left out, so a ping-pong does not show it twice.
pub fn reverse_provider(
    document: &ProjectDocument,
    plan: &RenderPlan,
    at: ProjectFrame,
    frames: FrameDuration,
    bounce: bool,
    index: &mut IndexLookup<'_>,
) -> Result<PauseProvider, String> {
    let (asset, span, picture_context) =
        passage(document, plan, at, frames, bounce, "Reverse", index)?;
    let first = ProjectFrame(at.0 - frames.frames());
    let heard_end = ProjectFrame(at.0 - i64::from(bounce));
    Ok(PauseProvider {
        video: HoldVideo::Reverse { asset, span },
        picture_context,
        audio: match heard_source(plan, first, heard_end)? {
            Some(source) => HoldAudio::Reverse { source },
            None => HoldAudio::Silence,
        },
    })
}

/// The pause a bleep puts in place of the `frames` before `at`: the same
/// pictures played forward at their natural rate, and a tone instead of
/// their sound.
pub fn bleep_provider(
    document: &ProjectDocument,
    plan: &RenderPlan,
    at: ProjectFrame,
    frames: FrameDuration,
    frequency_hz: u32,
    level: deadpan_core::GainDb,
    index: &mut IndexLookup<'_>,
) -> Result<PauseProvider, String> {
    let (asset, span, picture_context) =
        passage(document, plan, at, frames, false, "Bleep", index)?;
    Ok(PauseProvider {
        video: HoldVideo::Play { asset, span },
        picture_context,
        audio: HoldAudio::Tone {
            frequency_hz,
            level,
        },
    })
}

/// The measured picture span of one continuous natural-rate Original passage
/// over the `frames` before `at` (without its last picture when `bounce`),
/// and the composition a freeze at `at` would keep.
fn passage(
    document: &ProjectDocument,
    plan: &RenderPlan,
    at: ProjectFrame,
    frames: FrameDuration,
    bounce: bool,
    action: &str,
    index: &mut IndexLookup<'_>,
) -> Result<(AssetId, SourceSpan, Option<CapturedFraming>), String> {
    let rate = document.presentation_basis().frame_rate;
    let count = frames.frames();
    if count <= i64::from(bounce) || count > at.0 {
        return Err(format!(
            "There is not enough before the cursor to {}.",
            action.to_lowercase()
        ));
    }
    if count * i64::from(rate.denominator()) > MAX_REVERSE_SECONDS * i64::from(rate.numerator()) {
        return Err(format!(
            "{action} at most {MAX_REVERSE_SECONDS} s at a time."
        ));
    }
    let first = at.0 - count;
    let not_continuous =
        || format!("{action} needs one continuous Original passage at its natural speed.");
    let mut samples = Vec::with_capacity(usize::try_from(count).map_err(|_| not_continuous())?);
    for frame in first..at.0 {
        samples.push(
            plan.picture(ProjectFrame(frame))
                .map_err(|error| error.to_string())?,
        );
    }
    let (asset, origin) = match &samples[0].picture {
        Picture::Source { asset, point, .. } => (asset.clone(), *point),
        _ => return Err(not_continuous()),
    };
    let base = origin.time_base;
    // Source ticks per project frame: (fps_den / fps_num) / (tb_num / tb_den).
    let ticks_per_frame = ExactRatio::new(
        i128::from(rate.denominator()) * i128::from(base.denominator()),
        i128::from(rate.numerator()) * i128::from(base.numerator()),
    )
    .map_err(|error| error.to_string())?;
    for (offset, sample) in samples.iter().enumerate() {
        let Picture::Source {
            asset: shown,
            point,
            ..
        } = &sample.picture
        else {
            return Err(not_continuous());
        };
        let expected = origin
            .ticks
            .checked_add(
                ticks_per_frame
                    .checked_mul(ExactRatio::integer(offset as i64))
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        if *shown != asset
            || point.time_base != base
            || point.ticks != expected
            || sample.instance != samples[0].instance
        {
            return Err(not_continuous());
        }
    }
    let pictures = index(&asset)?;
    let frame_of = |sample: &PictureSample| {
        sample
            .picture
            .select_source_frame(&pictures)
            .map_err(|error| error.to_string())
    };
    let start = frame_of(&samples[0])?.pts;
    let last = frame_of(samples.last().expect("count is positive"))?;
    let end = if bounce {
        last.pts
    } else {
        pictures
            .interval(last.identity)
            .map_err(|error| error.to_string())?
            .1
    };
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: pictures.time_base(),
        },
        SourceTimestamp {
            ticks: end,
            time_base: pictures.time_base(),
        },
    )
    .map_err(|_| not_continuous())?;
    // The same composition a freeze at the cursor would keep.
    let frozen = pause_provider(document, plan, at, index)?;
    Ok((asset, span, frozen.picture_context))
}

/// The exact source audio heard over `[from, to)`: one contiguous run of a
/// single Original's samples, or `None` when that stretch is silent.
fn heard_source(
    plan: &RenderPlan,
    from: ProjectFrame,
    to: ProjectFrame,
) -> Result<Option<SourceAudio>, String> {
    let rate = plan.metadata().presentation_basis.frame_rate;
    let start = rate
        .audio_boundary(from)
        .map_err(|error| error.to_string())?;
    let end = rate.audio_boundary(to).map_err(|error| error.to_string())?;
    if start >= end {
        return Ok(None);
    }
    let query = plan
        .audio(start..end, AudioQueryLimits::default())
        .map_err(|error| error.to_string())?;
    if query
        .spans
        .iter()
        .all(|span| matches!(span.content, AudioContent::Silence { .. }))
    {
        return Ok(None);
    }
    contiguous_source(&query.spans, start, end)?
        .map(Some)
        .ok_or_else(|| {
            "The sound there is not one continuous Original passage; nothing was changed.".into()
        })
}

/// Whether `span` hears its source at the natural rate with no speed stage:
/// one 48 kHz sample advances the source by exactly `rate / 48000` of its
/// samples (its time base is `1/rate`). Stretched mappings and retimes would
/// make a reversed copy differ from what was heard.
fn natural_rate(span: &deadpan_plan::AudioSpan, source: &SourceAudio) -> Result<bool, String> {
    if !span.retimes.is_empty() {
        return Ok(false);
    }
    let base = source.span.start().time_base;
    if base.numerator() != 1 {
        return Ok(false);
    }
    let length = span.samples.end.0 - span.samples.start.0;
    let advance = span
        .source_point(span.samples.end)
        .and_then(|end| {
            span.source_point(span.samples.start)
                .map(|start| (start, end))
        })
        .map_err(|error| error.to_string())?;
    let expected = ExactRatio::new(
        i128::from(length) * i128::from(base.denominator()),
        i128::from(deadpan_core::MIX_SAMPLE_RATE),
    )
    .map_err(|error| error.to_string())?;
    Ok(advance
        .1
        .ticks
        .checked_sub(advance.0.ticks)
        .map_err(|error| error.to_string())?
        == expected)
}

/// One source span for consecutive `spans` covering `[start, end)` of the
/// same asset with no jump, in whole samples of its own audio clock, or
/// `None` when they do not form one.
fn contiguous_source(
    spans: &[deadpan_plan::AudioSpan],
    start: AudioSample,
    end: AudioSample,
) -> Result<Option<SourceAudio>, String> {
    let mut previous: Option<(&SourceAudio, deadpan_core::SourcePoint)> = None;
    let mut covered = start;
    for span in spans {
        let AudioContent::Source { source, .. } = &span.content else {
            return Ok(None);
        };
        if span.samples.start != covered || !natural_rate(span, source)? {
            return Ok(None);
        }
        let begin = span
            .source_point(span.samples.start)
            .map_err(|error| error.to_string())?;
        if let Some((earlier, point)) = previous
            && (earlier.asset != source.asset || point != begin)
        {
            return Ok(None);
        }
        previous = Some((
            source,
            span.source_point(span.samples.end)
                .map_err(|error| error.to_string())?,
        ));
        covered = span.samples.end;
    }
    let (Some(first), Some((source, last))) = (spans.first(), previous) else {
        return Ok(None);
    };
    if covered != end {
        return Ok(None);
    }
    let base = source.span.start().time_base;
    // Whole source samples need a 1/rate clock.
    if base.numerator() != 1 {
        return Ok(None);
    }
    let begin = first
        .source_point(start)
        .map_err(|error| error.to_string())?
        .ticks
        .ceil()
        .map_err(|error| error.to_string())?;
    let finish = last.ticks.floor();
    let clamp =
        |value: i128| i64::try_from(value).map_err(|_| "Audio position overflows.".to_string());
    let (begin, finish) = (
        clamp(begin)?.max(source.span.start().ticks),
        clamp(finish)?.min(source.span.end().ticks),
    );
    if begin >= finish {
        return Ok(None);
    }
    Ok(Some(SourceAudio {
        asset: source.asset.clone(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: begin,
                time_base: base,
            },
            SourceTimestamp {
                ticks: finish,
                time_base: base,
            },
        )
        .map_err(|error| error.to_string())?,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use deadpan_core::{
        AssetRecord, BeatNode, ColorPolicy, FrameRate, LinkRelation, NodeKind, PresentationBasis,
        ProjectId, RevisionId, SourceAudioMapping, SourceNode, SourceTimeBase, SourceVideo,
        SourceVideoMapping,
    };

    use super::*;

    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }

    /// One 48 kHz Original beat of 4,800 frames at 48,000 fps with `mapping`.
    fn plan(mapping: impl FnOnce(SourceSpan, FrameRate) -> SourceAudioMapping) -> RenderPlan {
        let rate = FrameRate::new(48_000, 1).unwrap();
        let clock = SourceTimeBase::new(1, 48_000).unwrap();
        let at = |ticks| SourceTimestamp {
            ticks,
            time_base: clock,
        };
        let selected = SourceSpan::new(at(0), at(4_800)).unwrap();
        let audio = SourceAudio {
            asset: AssetId::new("media").unwrap(),
            span: selected,
        };
        let beat = BeatNode {
            framing: None,
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            label: "Original".into(),
            kind: NodeKind::Source {
                source: SourceNode {
                    edit_window: None,
                    duration: FrameDuration::new(4_800).unwrap(),
                    video: SourceVideo::Blank,
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio_mapping: mapping(selected, rate),
                    audio: Some(audio),
                    audio_offset: AudioSample(0),
                    link: LinkRelation::Independent,
                },
            },
            cutaways: Vec::new(),
            captions: Vec::new(),
        };
        let empty = ProjectDocument::new(
            ProjectId::new("pause").unwrap(),
            RevisionId::new("r").unwrap(),
            PresentationBasis {
                width: 640,
                height: 480,
                frame_rate: rate,
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let mut wire = serde_json::to_value(empty).unwrap();
        wire["nodes"] = serde_json::to_value(BTreeMap::from([
            (
                id("root"),
                BeatNode::sequence("Sequence", vec![id("speech")]),
            ),
            (id("speech"), beat),
        ]))
        .unwrap();
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            AssetId::new("media").unwrap(),
            AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(SourceSpan::new(at(0), at(48_000)).unwrap()),
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        )]))
        .unwrap();
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap()
    }

    #[test]
    fn heard_sound_must_be_at_its_natural_rate() {
        let natural = plan(|span, rate| SourceAudioMapping::natural_rate(span, rate).unwrap());
        let heard = heard_source(&natural, ProjectFrame(1_000), ProjectFrame(2_000))
            .unwrap()
            .unwrap();
        assert_eq!(
            (heard.span.start().ticks, heard.span.end().ticks),
            (1_000, 2_000)
        );
        // The same 4,800 samples squeezed into 2,400 mix samples (double
        // speed) would reverse something other than what was heard.
        let stretched = plan(|_, _| SourceAudioMapping::Duration {
            frames: ExactRatio::integer(2_400),
        });
        assert!(heard_source(&stretched, ProjectFrame(1_000), ProjectFrame(2_000)).is_err());
    }
}
