//! Complete forward phrase occurrences on the exact provider-picture clock.
//! This intentionally proves picture-provider coverage, not audible PCM.

use deadpan_analysis::Transcript;
use deadpan_core::{
    AssetId, BoundaryQueryLimits, EditError, EditErrorCode, ExactFrameRange, ExactRatio,
    FrameRange, InstancePath, MAX_EDIT_SET_COMPONENTS, NodeId, ProjectFrame, SourceFrameIndex,
};
use deadpan_plan::{Picture, PictureClockSlope, ProviderLeafKind, RenderPlan};

const MAX_SEARCH_MATCHES: usize = 4096;
const MAX_PROJECTION_WORK: usize = 1_000_000;

struct Work(usize);

impl Work {
    fn charge(&mut self) -> Result<(), EditError> {
        self.0 = self.0.checked_sub(1).ok_or_else(|| {
            limit("transcript search exceeded its aggregate projection work budget")
        })?;
        Ok(())
    }
}

struct Segment<'a> {
    root: ExactFrameRange,
    source: ExactFrameRange,
    slope: ExactRatio,
    instance: &'a InstancePath,
}

struct Chain {
    first: usize,
    end: usize,
}

/// Resolve all complete occurrences, then enclose them in editable frames.
/// The caller supplies the qualified Original identity and corrected transcript;
/// Core separately validates ordinary Sequence authority and edit geometry.
pub(super) fn project_matches(
    plan: &RenderPlan,
    asset: &AssetId,
    index: &SourceFrameIndex,
    transcript: &Transcript,
    parent: &NodeId,
    query: &str,
) -> Result<Vec<FrameRange>, EditError> {
    if index.asset() != asset {
        return Err(unavailable(
            "transcript search Original differs from its qualified picture index",
        ));
    }
    let scope = plan
        .single_occurrence_exact_range(parent)
        .map_err(|error| unavailable(&error.to_string()))?
        .ok_or_else(|| {
            unavailable("transcript search requires a scope beneath ordinary Sequences")
        })?;
    let extent = plan
        .node_exact_duration(&plan.metadata().root)
        .ok_or_else(|| unavailable("transcript search root extent is unavailable"))?;
    if !extent.compare(ExactRatio::ZERO).is_gt() {
        return Err(unavailable(
            "transcript search found no complete phrase in this empty edit",
        ));
    }
    let matches = transcript
        .search_for_selection(query, MAX_SEARCH_MATCHES)
        .map_err(|error| match error {
            deadpan_analysis::TranscriptError::Limit(_) => limit(&error.to_string()),
            _ => unavailable(&error.to_string()),
        })?;
    if matches.is_empty() {
        return Err(unavailable("transcript search found no matching words"));
    }
    let base = index.time_base();
    let ticks_per_second =
        ExactRatio::new(i128::from(base.denominator()), i128::from(base.numerator()))
            .map_err(|error| unavailable(&error.to_string()))?;
    let mut phrases = Vec::with_capacity(matches.len());
    for matched in matches {
        let words = &transcript.words()[matched];
        let first = words
            .first()
            .ok_or_else(|| unavailable("transcript search returned an empty phrase"))?;
        let last = words
            .last()
            .ok_or_else(|| unavailable("transcript search returned an empty phrase"))?;
        let start = transcript
            .seconds(first.start_cs)
            .map_err(|error| unavailable(&error.to_string()))?
            .checked_mul(ticks_per_second)
            .map_err(timing)?;
        let end = transcript
            .seconds(last.end_cs)
            .map_err(|error| unavailable(&error.to_string()))?
            .checked_mul(ticks_per_second)
            .map_err(timing)?;
        if !start.compare(end).is_lt() {
            return Err(unavailable(
                "a matching transcript phrase has no positive timed interval",
            ));
        }
        phrases.push(ExactFrameRange { start, end });
    }
    let coverage = plan
        .provider_clock_coverage(
            scope,
            BoundaryQueryLimits {
                max_scopes: 131_072,
                max_comparisons: 1_048_576,
            },
        )
        .map_err(|error| match error {
            deadpan_plan::PlanError::PictureQueryLimit(_) => limit(&error.to_string()),
            _ => unavailable(&error.to_string()),
        })?;
    let measured_start = ExactRatio::integer(
        index
            .frames()
            .first()
            .ok_or_else(|| unavailable("transcript search picture index is empty"))?
            .pts,
    );
    let measured_end = ExactRatio::integer(index.terminal_end());
    let mut work = Work(MAX_PROJECTION_WORK);
    let mut segments = Vec::new();
    let mut chains: Vec<Chain> = Vec::new();
    for span in &coverage.spans {
        work.charge()?;
        if span.leaf_kind != ProviderLeafKind::Source
            || span.in_repeat_gap
            || span.gap_after.is_some()
        {
            continue;
        }
        let (
            Picture::Source {
                asset: shown,
                point,
                selection,
                ..
            },
            PictureClockSlope::SourceTicks(slope),
        ) = (&span.picture, span.clock)
        else {
            continue;
        };
        if shown != asset || !slope.compare(ExactRatio::ZERO).is_gt() {
            continue;
        }
        if point.time_base != base
            || selection.start().time_base != base
            || selection.end().time_base != base
        {
            return Err(unavailable(
                "transcript search Original picture clock differs from its qualified index",
            ));
        }
        let raw_end = point
            .ticks
            .checked_add(
                span.range
                    .end
                    .checked_sub(span.range.start)
                    .map_err(timing)?
                    .checked_mul(slope)
                    .map_err(timing)?,
            )
            .map_err(timing)?;
        // Endpoint holding may display a valid picture outside this intersection.
        // Only the raw forward clock inside measured and selected support counts.
        let start = max(point.ticks, max(selection.start().ticks, measured_start));
        let end = min(raw_end, min(selection.end().ticks, measured_end));
        if !start.compare(end).is_lt() {
            continue;
        }
        let root_at = |ticks: ExactRatio| -> Result<ExactRatio, EditError> {
            span.range
                .start
                .checked_add(
                    ticks
                        .checked_sub(point.ticks)
                        .map_err(timing)?
                        .checked_div(slope)
                        .map_err(timing)?,
                )
                .map_err(timing)
        };
        let segment = Segment {
            root: ExactFrameRange {
                start: root_at(start)?,
                end: root_at(end)?,
            },
            source: ExactFrameRange { start, end },
            slope,
            instance: &span.instance,
        };
        let continues = segments
            .last()
            .map(|previous| continuous(previous, &segment, &mut work))
            .transpose()?
            .unwrap_or(false);
        if continues {
            chains
                .last_mut()
                .expect("a previous segment has a chain")
                .end += 1;
        } else {
            chains.push(Chain {
                first: segments.len(),
                end: segments.len() + 1,
            });
        }
        segments.push(segment);
    }
    let mut ranges = Vec::new();
    // Both chains and transcript matches are in start order. Therefore each
    // emitted enclosure is ordered, permitting bounded incremental normalization.
    for chain in chains {
        let chain = &segments[chain.first..chain.end];
        let first = &chain[0];
        let last = &chain[chain.len() - 1];
        for phrase in &phrases {
            work.charge()?;
            if first.source.start.compare(phrase.start).is_gt()
                || last.source.end.compare(phrase.end).is_lt()
            {
                continue;
            }
            let start = invert(chain, phrase.start, false, &mut work)?;
            let end = invert(chain, phrase.end, true, &mut work)?;
            if start.compare(scope.start).is_lt() || end.compare(scope.end).is_gt() {
                continue;
            }
            let start = i64::try_from(start.floor())
                .map_err(|_| unavailable("transcript search frame start overflow"))?;
            let end = i64::try_from(end.ceil().map_err(timing)?)
                .map_err(|_| unavailable("transcript search frame end overflow"))?;
            // Ordinary edit authority owns complete integral frames. Exact
            // coverage at a fractional terminal cannot grant the frame beyond it.
            if ExactRatio::integer(start).compare(scope.start).is_lt()
                || ExactRatio::integer(end).compare(scope.end).is_gt()
            {
                continue;
            }
            let range = FrameRange::new(ProjectFrame(start), ProjectFrame(end)).map_err(timing)?;
            append_range(&mut ranges, range)?;
        }
    }
    if ranges.is_empty() {
        return Err(unavailable(
            "transcript search found no complete forward phrase inside the current Sequence",
        ));
    }
    Ok(ranges)
}

fn continuous(left: &Segment<'_>, right: &Segment<'_>, work: &mut Work) -> Result<bool, EditError> {
    if left.root.end != right.root.start || left.source.end != right.source.start {
        return Ok(false);
    }
    // Shared Repeat ancestors form a common prefix in the validated owner tree.
    // Entering/leaving a Repeat alone is harmless; changing its play is a wrap.
    for (left, right) in left.instance.repeats.iter().zip(&right.instance.repeats) {
        work.charge()?;
        if left.node != right.node {
            break;
        }
        if left.iteration != right.iteration {
            return Ok(false);
        }
    }
    Ok(true)
}

fn invert(
    chain: &[Segment<'_>],
    ticks: ExactRatio,
    right_endpoint: bool,
    work: &mut Work,
) -> Result<ExactRatio, EditError> {
    for segment in chain {
        work.charge()?;
        if segment.source.end.compare(ticks).is_gt()
            || (right_endpoint && segment.source.end == ticks)
        {
            return segment
                .root
                .start
                .checked_add(
                    ticks
                        .checked_sub(segment.source.start)
                        .map_err(timing)?
                        .checked_div(segment.slope)
                        .map_err(timing)?,
                )
                .map_err(timing);
        }
    }
    Err(unavailable(
        "transcript search phrase escaped its proven provider chain",
    ))
}

fn append_range(ranges: &mut Vec<FrameRange>, range: FrameRange) -> Result<(), EditError> {
    if let Some(previous) = ranges.last_mut() {
        if range.start() < previous.start() {
            return Err(unavailable(
                "transcript search results are not in root-clock order",
            ));
        }
        if range.start() <= previous.end() {
            *previous = FrameRange::new(previous.start(), previous.end().max(range.end()))
                .map_err(timing)?;
            return Ok(());
        }
    }
    if ranges.len() == MAX_EDIT_SET_COMPONENTS {
        return Err(limit(
            "transcript search exceeds 64 disjoint Edit intervals",
        ));
    }
    ranges.push(range);
    Ok(())
}

fn min(left: ExactRatio, right: ExactRatio) -> ExactRatio {
    if left.compare(right).is_lt() {
        left
    } else {
        right
    }
}

fn max(left: ExactRatio, right: ExactRatio) -> ExactRatio {
    if left.compare(right).is_gt() {
        left
    } else {
        right
    }
}

fn timing(error: deadpan_core::TimeError) -> EditError {
    unavailable(&format!("transcript search timing failed: {error}"))
}

fn unavailable(message: &str) -> EditError {
    EditError {
        code: EditErrorCode::SelectionUnavailable,
        message: message.into(),
        current_revision: None,
    }
}

fn limit(message: &str) -> EditError {
    EditError {
        code: EditErrorCode::LimitExceeded,
        message: message.into(),
        current_revision: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_analysis::{AnalysedAudio, Word};
    use deadpan_core::*;
    use std::collections::BTreeMap;

    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }
    fn asset() -> AssetId {
        AssetId::new("original").unwrap()
    }
    fn rev(value: &str) -> RevisionId {
        RevisionId::new(value).unwrap()
    }
    fn duration(frames: i64) -> FrameDuration {
        FrameDuration::new(frames).unwrap()
    }
    fn range(start: i64, end: i64) -> FrameRange {
        FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
    }
    fn base() -> SourceTimeBase {
        SourceTimeBase::new(1, 1000).unwrap()
    }
    fn source_span(start: i64, end: i64) -> SourceSpan {
        SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base: base(),
            },
            SourceTimestamp {
                ticks: end,
                time_base: base(),
            },
        )
        .unwrap()
    }
    fn node(kind: NodeKind) -> BeatNode {
        BeatNode {
            label: "fixture".into(),
            kind,
            framing: None,
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            cutaways: Vec::new(),
            captions: Vec::new(),
        }
    }
    fn source(frames: i64, start: i64, end: i64) -> BeatNode {
        node(NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: duration(frames),
                video_mapping: SourceVideoMapping::FitBeat,
                video: SourceVideo::Stream {
                    asset: asset(),
                    span: source_span(start, end),
                },
                audio: None,
                link: LinkRelation::Independent,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
            },
        })
    }
    fn hold(frames: i64, video: HoldVideo) -> BeatNode {
        node(NodeKind::Hold {
            recipe: HoldRecipe {
                duration: duration(frames),
                video,
                picture_context: None,
                audio: HoldAudio::Silence,
            },
        })
    }
    fn repeated(child: &str, plays: u32) -> BeatNode {
        node(NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(rev("plays"), plays).unwrap(),
            gap: None,
            escalation: None,
        })
    }
    fn retime(child: &str, frames: i64, end: i64) -> BeatNode {
        node(NodeKind::Retime {
            child: id(child),
            duration: duration(frames),
            mapping: range(0, end),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        })
    }
    fn document(roots: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
        let empty = ProjectDocument::new(
            ProjectId::new("project").unwrap(),
            rev("initial"),
            PresentationBasis {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let mut wire = serde_json::to_value(empty).unwrap();
        let mut nodes: BTreeMap<_, _> = nodes
            .into_iter()
            .map(|(name, node)| (id(name), node))
            .collect();
        nodes.insert(
            id("root"),
            BeatNode::sequence("root", roots.iter().map(|name| id(name)).collect()),
        );
        wire["nodes"] = serde_json::to_value(nodes).unwrap();
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            asset(),
            AssetRecord {
                source_qualification: None,
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: Some(source_span(-10000, 100000)),
                audio: Some(source_span(-10000, 100000)),
                still_image: false,
                frame_count: Some(duration(10000)),
            },
        )]))
        .unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }
    fn index(pts: &[i64], end: i64) -> SourceFrameIndex {
        SourceFrameIndex::new(
            asset(),
            base(),
            pts.iter()
                .enumerate()
                .map(|(ordinal, pts)| IndexedSourceFrame {
                    identity: SourceFrameId(u64::try_from(ordinal).unwrap()),
                    pts: *pts,
                    reported_duration: None,
                    keyframe: ordinal == 0,
                    seek_from: Some(SourceFrameId(0)),
                    decode_timestamp: None,
                })
                .collect(),
            end,
            TerminalProvenance::Explicit,
        )
        .unwrap()
    }
    fn transcript(origin: i64, words: &[(&str, u32, u32)]) -> Transcript {
        Transcript::new(
            AnalysedAudio {
                origin,
                sample_rate: 48000,
                duration_cs: 100000,
            },
            words
                .iter()
                .map(|(text, start_cs, end_cs)| Word {
                    text: (*text).into(),
                    start_cs: *start_cs,
                    end_cs: *end_cs,
                    probability: 1.0,
                    segment: 0,
                })
                .collect(),
        )
        .unwrap()
    }
    fn phrase() -> Transcript {
        transcript(0, &[("hello", 10, 20), ("world", 80, 90)])
    }
    fn search(
        plan: &RenderPlan,
        transcript: &Transcript,
        query: &str,
    ) -> Result<Vec<FrameRange>, EditError> {
        project_matches(
            plan,
            &asset(),
            &index(&[0, 17, 333, 900], 1000),
            transcript,
            &id("root"),
            query,
        )
    }
    fn compile(roots: &[&str], nodes: Vec<(&str, BeatNode)>) -> RenderPlan {
        RenderPlan::compile(&document(roots, nodes)).unwrap()
    }
    fn refused(result: Result<Vec<FrameRange>, EditError>) {
        assert!(result.is_err_and(|error| error.code == EditErrorCode::SelectionUnavailable));
    }

    #[test]
    fn complete_phrase_crosses_split_source_ids_and_changed_positive_slopes() {
        let plan = compile(
            &["left", "speed"],
            vec![
                ("left", source(5, 0, 500)),
                ("right", source(5, 500, 1000)),
                ("speed", retime("right", 10, 5)),
            ],
        );
        assert_eq!(
            search(&plan, &phrase(), "hello wor").unwrap(),
            [range(1, 13)]
        );
        // Exact source [100,900) maps to root [1,13), including the long pause.
        assert_eq!(search(&plan, &phrase(), "world").unwrap(), [range(11, 13)]);
    }

    #[test]
    fn vfr_subpicture_words_and_fractional_retime_use_continuous_clock() {
        let words = transcript(0, &[("tiny", 1, 2), ("phrase", 3, 4)]);
        let plan = compile(&["source"], vec![("source", source(1, 0, 1000))]);
        assert_eq!(
            project_matches(
                &plan,
                &asset(),
                &index(&[0], 1000),
                &words,
                &id("root"),
                "tiny phrase"
            )
            .unwrap(),
            [range(0, 1)]
        );
        let plan = compile(
            &["speed"],
            vec![
                ("source", source(10, 0, 1000)),
                ("speed", retime("source", 5, 10)),
            ],
        );
        assert_eq!(
            search(&plan, &phrase(), "hello world").unwrap(),
            [range(0, 5)]
        );
        assert_eq!(
            search(&plan, &transcript(0, &[("last", 80, 100)]), "last").unwrap(),
            [range(4, 5)]
        );
    }

    #[test]
    fn fractional_terminal_is_proved_but_an_enclosure_outside_scope_is_not_admitted() {
        let mut speed = retime("source", 2, 10);
        let NodeKind::Retime { pitch, .. } = &mut speed.kind else {
            unreachable!()
        };
        *pitch = PitchPolicy::FollowSpeed;
        let mut tail = retime("tail-source", 1, 1);
        let NodeKind::Retime { pitch, .. } = &mut tail.kind else {
            unreachable!()
        };
        *pitch = PitchPolicy::FollowSpeed;
        let base_doc = document(
            &["scope", "tail"],
            vec![
                ("scope", BeatNode::sequence("scope", vec![id("speed")])),
                ("source", source(10, 0, 1000)),
                ("speed", speed),
                ("tail", tail),
                ("tail-source", source(1, 1000, 1100)),
            ],
        );
        let mut wire = serde_json::to_value(base_doc).unwrap();
        // Keep the public document's root integral while the ordinary scope
        // ends halfway through frame 1. The tail cannot grant scope authority.
        wire["derived_clocks"] = serde_json::to_value(BTreeMap::from([
            (
                id("speed"),
                DerivedRetimeClock {
                    extent: ExactRatio::new(3, 2).unwrap(),
                    child_start: ExactRatio::ZERO,
                    child_end: ExactRatio::integer(10),
                    preserve_input: None,
                },
            ),
            (
                id("tail"),
                DerivedRetimeClock {
                    extent: ExactRatio::new(1, 2).unwrap(),
                    child_start: ExactRatio::ZERO,
                    child_end: ExactRatio::ONE,
                    preserve_input: None,
                },
            ),
        ]))
        .unwrap();
        let plan =
            RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
        assert_eq!(
            plan.node_exact_duration(&id("root")),
            Some(ExactRatio::integer(2))
        );
        assert_eq!(
            plan.single_occurrence_exact_range(&id("scope")).unwrap(),
            Some(ExactFrameRange {
                start: ExactRatio::ZERO,
                end: ExactRatio::new(3, 2).unwrap(),
            })
        );
        let scoped_search = |words: &Transcript, query: &str| {
            project_matches(
                &plan,
                &asset(),
                &index(&[0, 17, 333, 900], 1000),
                words,
                &id("scope"),
                query,
            )
        };
        assert_eq!(
            scoped_search(&transcript(0, &[("inside", 10, 50)]), "inside").unwrap(),
            [range(0, 1)]
        );
        refused(scoped_search(
            &transcript(0, &[("terminal", 80, 100)]),
            "terminal",
        ));
    }

    #[test]
    fn nonzero_and_negative_audio_origins_share_container_time_with_video() {
        for (origin, source_start, expected) in [(-4800, 0, range(0, 3)), (4800, 100, range(1, 4))]
        {
            let plan = compile(
                &["source"],
                vec![("source", source(10, source_start, source_start + 1000))],
            );
            let words = transcript(origin, &[("hello", 10, 20), ("world", 30, 40)]);
            let measured = index(&[-100, 17, 333, 900], 1100);
            assert_eq!(
                project_matches(
                    &plan,
                    &asset(),
                    &measured,
                    &words,
                    &id("root"),
                    "hello world"
                )
                .unwrap(),
                [expected]
            );
        }
        // These ends lie after the irregular index's last PTS (900), before
        // its actual terminal. Last PTS is never substituted for terminal end.
        for (origin, source_start, words, expected) in [
            (
                -4800,
                0,
                transcript(-4800, &[("tail", 103, 109)]),
                range(9, 10),
            ),
            (
                4800,
                100,
                transcript(4800, &[("tail", 83, 89)]),
                range(8, 9),
            ),
        ] {
            assert_eq!(
                words.seconds(words.words()[0].end_cs).unwrap(),
                ExactRatio::new(99, 100).unwrap()
            );
            let plan = compile(
                &["source"],
                vec![("source", source(10, source_start, source_start + 1000))],
            );
            let measured = index(&[-100, 17, 333, 900], 1100);
            assert_eq!(
                project_matches(&plan, &asset(), &measured, &words, &id("root"), "tail").unwrap(),
                [expected]
            );
            assert_eq!(words.audio().origin, origin);
        }
    }

    #[test]
    fn removed_pause_or_middle_word_and_backtracking_cannot_complete_a_phrase() {
        let words = transcript(
            0,
            &[("hello", 10, 20), ("middle", 40, 60), ("world", 80, 90)],
        );
        for (left_end, right_start) in [(400, 600), (500, 700), (600, 500)] {
            let plan = compile(
                &["left", "right"],
                vec![
                    ("left", source(5, 0, left_end)),
                    ("right", source(5, right_start, 1000)),
                ],
            );
            refused(search(&plan, &words, "hello middle world"));
        }
        let removed_pause = compile(
            &["left", "right"],
            vec![("left", source(4, 0, 400)), ("right", source(4, 600, 1000))],
        );
        refused(search(&removed_pause, &phrase(), "hello world"));
        assert!(search(&removed_pause, &phrase(), "hello").is_ok());
        assert!(search(&removed_pause, &phrase(), "world").is_ok());
    }

    #[test]
    fn repeated_complete_occurrences_stay_separate_and_wraps_break_contiguous_overrides() {
        let repeated_plan = compile(
            &["repeat"],
            vec![
                ("repeat", repeated("source", 2)),
                ("source", source(10, 0, 1000)),
            ],
        );
        assert_eq!(
            search(
                &repeated_plan,
                &transcript(0, &[("hello", 10, 20)]),
                "hello"
            )
            .unwrap(),
            [range(1, 2), range(11, 12)]
        );
        let base_doc = document(
            &["repeat"],
            vec![
                ("repeat", repeated("source", 2)),
                ("source", source(5, 0, 500)),
            ],
        );
        let mut wire = serde_json::to_value(base_doc).unwrap();
        wire["nodes"]["override"] = serde_json::to_value(source(5, 500, 1000)).unwrap();
        wire["overrides"]["repeat"] = serde_json::json!([
            {"iteration":{"allocation":"plays","ordinal":1},"root":"override"}
        ]);
        let plan =
            RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
        refused(search(
            &plan,
            &transcript(0, &[("hello", 40, 45), ("world", 55, 60)]),
            "hello world",
        ));
    }

    #[test]
    fn split_chains_continue_within_a_play_and_across_repeat_entry_and_exit() {
        let plan = compile(
            &["repeat"],
            vec![
                ("repeat", repeated("group", 2)),
                (
                    "group",
                    BeatNode::sequence("group", vec![id("left"), id("right")]),
                ),
                ("left", source(5, 0, 500)),
                ("right", source(5, 500, 1000)),
            ],
        );
        assert_eq!(
            search(&plan, &phrase(), "hello world").unwrap(),
            [range(1, 9), range(11, 19)]
        );
        let plan = compile(
            &["left", "repeat", "right"],
            vec![
                ("left", source(3, 0, 300)),
                ("repeat", repeated("middle", 1)),
                ("middle", source(4, 300, 700)),
                ("right", source(3, 700, 1000)),
            ],
        );
        assert_eq!(
            search(&plan, &phrase(), "hello world").unwrap(),
            [range(1, 9)]
        );
    }

    #[test]
    fn hold_play_and_default_or_explicit_source_gaps_never_supply_phrase_coverage() {
        let plan = compile(
            &["hold"],
            vec![(
                "hold",
                hold(
                    10,
                    HoldVideo::Play {
                        asset: asset(),
                        span: source_span(0, 1000),
                        origin: None,
                    },
                ),
            )],
        );
        refused(search(&plan, &phrase(), "hello world"));
        for explicit in [false, true] {
            let mut repeat = repeated("source", 2);
            let NodeKind::Repeat { gap, .. } = &mut repeat.kind else {
                unreachable!()
            };
            *gap = Some(HoldRecipe {
                duration: duration(6),
                video: HoldVideo::Play {
                    asset: asset(),
                    span: source_span(200, 800),
                    origin: None,
                },
                picture_context: None,
                audio: HoldAudio::Silence,
            });
            let mut wire = serde_json::to_value(document(
                &["repeat"],
                vec![("repeat", repeat), ("source", source(2, 0, 200))],
            ))
            .unwrap();
            wire["nodes"]["next"] = serde_json::to_value(source(2, 800, 1000)).unwrap();
            wire["overrides"]["repeat"] = serde_json::json!([
                {"iteration":{"allocation":"plays","ordinal":1},"root":"next"}
            ]);
            if explicit {
                wire["nodes"]["gap"] = serde_json::to_value(source(6, 200, 800)).unwrap();
                wire["gap_overrides"]["repeat"] = serde_json::json!([
                    {"iteration":{"allocation":"plays","ordinal":0},"root":"gap"}
                ]);
            }
            let plan = RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap())
                .unwrap();
            refused(search(&plan, &phrase(), "hello world"));
            refused(search(
                &plan,
                &transcript(0, &[("gapword", 40, 50)]),
                "gapword",
            ));
        }
    }

    #[test]
    fn cutaways_do_not_change_provider_results() {
        for removed in [false, true] {
            let mut host = source(10, 0, 1000);
            host.cutaways.push(Cutaway {
                range: range(2, 8),
                asset: asset(),
                selection: source_span(2000, 3000).into(),
                fit: CutawayFit::Hold,
                removed,
            });
            let plan = compile(&["source"], vec![("source", host)]);
            assert_eq!(
                search(&plan, &phrase(), "hello world").unwrap(),
                [range(1, 9)]
            );
        }
    }

    #[test]
    fn held_selection_slack_and_unmeasured_picture_tail_do_not_manufacture_hits() {
        let mut host = source(10, 0, 500);
        let NodeKind::Source { source: mapping } = &mut host.kind else {
            unreachable!()
        };
        mapping.video_mapping = SourceVideoMapping::Duration {
            frames: ExactRatio::integer(5),
            endpoints: EndpointPolicy::HoldAdjacent,
        };
        let plan = compile(&["source"], vec![("source", host)]);
        refused(search(&plan, &transcript(0, &[("held", 60, 80)]), "held"));
        assert_eq!(
            search(&plan, &transcript(0, &[("real", 10, 20)]), "real").unwrap(),
            [range(1, 2)]
        );
        let plan = compile(&["source"], vec![("source", source(10, 0, 1000))]);
        refused(project_matches(
            &plan,
            &asset(),
            &index(&[0, 17, 333], 500),
            &transcript(0, &[("tail", 60, 80)]),
            &id("root"),
            "tail",
        ));
    }

    #[test]
    fn scope_filters_complete_occurrences_without_clipping_partial_phrases() {
        let plan = compile(
            &["before", "group", "partial"],
            vec![
                ("before", source(10, 0, 1000)),
                ("group", BeatNode::sequence("group", vec![id("inside")])),
                ("inside", source(10, 0, 1000)),
                ("partial", BeatNode::sequence("partial", vec![id("tail")])),
                ("tail", source(4, 600, 1000)),
            ],
        );
        let measured = index(&[0, 17, 333, 900], 1000);
        assert_eq!(
            project_matches(
                &plan,
                &asset(),
                &measured,
                &phrase(),
                &id("group"),
                "hello world"
            )
            .unwrap(),
            [range(11, 19)]
        );
        refused(project_matches(
            &plan,
            &asset(),
            &measured,
            &phrase(),
            &id("partial"),
            "hello world",
        ));
        refused(project_matches(
            &plan,
            &asset(),
            &measured,
            &phrase(),
            &id("missing"),
            "hello world",
        ));
    }

    #[test]
    fn unrelated_repeat_spans_do_not_exhaust_a_small_group_search_budget() {
        let plan = compile(
            &["outside", "group"],
            vec![
                ("outside", repeated("before", 513)),
                ("before", source(10, 0, 1000)),
                ("group", BeatNode::sequence("local", vec![id("inside")])),
                ("inside", source(10, 0, 1000)),
            ],
        );
        // The excluded earlier Repeat alone exceeds the provider span cap.
        assert!(matches!(
            plan.provider_clock_coverage(
                ExactFrameRange {
                    start: ExactRatio::ZERO,
                    end: ExactRatio::integer(5140)
                },
                BoundaryQueryLimits {
                    max_scopes: 131_072,
                    max_comparisons: 1_048_576
                },
            ),
            Err(deadpan_plan::PlanError::PictureQueryLimit(_))
        ));
        assert_eq!(
            project_matches(
                &plan,
                &asset(),
                &index(&[0, 17, 333, 900], 1000),
                &phrase(),
                &id("group"),
                "hello world"
            )
            .unwrap(),
            [range(5131, 5139)]
        );
    }

    #[test]
    fn overlapping_and_touching_enclosures_normalize_without_a_hull_over_gaps() {
        let plan = compile(&["source"], vec![("source", source(10, 0, 1000))]);
        let words = transcript(
            0,
            &[
                ("hit", 1, 2),
                ("hit", 3, 4),
                ("hit", 10, 20),
                ("hit", 60, 70),
            ],
        );
        assert_eq!(
            search(&plan, &words, "hit").unwrap(),
            [range(0, 2), range(6, 7)]
        );
    }

    #[test]
    fn unavailable_words_zero_timing_alias_and_limits_return_no_partial_geometry() {
        let plan = compile(&["source"], vec![("source", source(10, 0, 1000))]);
        refused(search(&plan, &phrase(), "absent"));
        refused(search(&plan, &transcript(0, &[("zero", 10, 10)]), "zero"));
        refused(project_matches(
            &plan,
            &AssetId::new("wrong").unwrap(),
            &index(&[0], 1000),
            &phrase(),
            &id("root"),
            "hello",
        ));
        let repeated_plan = compile(
            &["repeat"],
            vec![
                ("repeat", repeated("source", 65)),
                ("source", source(10, 0, 1000)),
            ],
        );
        assert!(
            search(&repeated_plan, &transcript(0, &[("hit", 10, 20)]), "hit")
                .is_err_and(|error| error.code == EditErrorCode::LimitExceeded
                    && error.message.contains("64"))
        );
        let words: Vec<_> = (0..4097_u32).map(|i| ("hit", i * 2, i * 2 + 1)).collect();
        assert!(
            search(&plan, &transcript(0, &words), "hit")
                .is_err_and(|error| error.code == EditErrorCode::LimitExceeded
                    && error.message.contains("search matches"))
        );
        let excessive_plan = compile(
            &["repeat"],
            vec![
                ("repeat", repeated("source", 513)),
                ("source", source(10, 0, 1000)),
            ],
        );
        assert!(
            search(&excessive_plan, &phrase(), "hello")
                .is_err_and(|error| error.code == EditErrorCode::LimitExceeded
                    && error.message.contains("picture spans"))
        );
        let repeated_plan = compile(
            &["repeat"],
            vec![
                ("repeat", repeated("source", 256)),
                ("source", source(10, 0, 1000)),
            ],
        );
        let outside: Vec<_> = (0..4096_u32)
            .map(|i| ("hit", 1000 + i * 2, 1001 + i * 2))
            .collect();
        assert!(
            search(&repeated_plan, &transcript(0, &outside), "hit")
                .is_err_and(|error| error.code == EditErrorCode::LimitExceeded
                    && error.message.contains("work budget"))
        );
    }
}
