//! Captions and cutaways placed over a group.
//!
//! Attachments live on Source and Hold hosts, in their content's clock, so a
//! group carries none of its own. A caption or cutaway over group frames
//! `[a, b)` becomes, for each child whose span meets it, that intersection in
//! the child's host clock: through nested ordinary Sequences and unity
//! Partitions (as [`crate::cutaway_host`] descends). Every picture center of
//! the group lies in exactly one child, so each frame shows the same caption
//! or picture as one attachment over the whole range would. A Repeat or speed
//! change inside the range refuses: its content plays at several group times or
//! another rate, and a caption there would count that Repeat's plays for
//! `reveal` instead of the enclosing one. Because nothing is held by the
//! group, Ungroup carries these attachments unchanged.

use std::collections::BTreeMap;

use crate::{
    Caption, Command, Cutaway, CutawayFit, EditError, EditErrorCode, ExactRatio, ExactSourceSpan,
    FrameRange, FrameRate, MAX_DOCUMENT_DEPTH, NodeId, NodeKind, ProjectDocument, ProjectFrame,
    SourcePoint,
};

/// One host's share of an attachment over a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSpan {
    pub host: NodeId,
    /// The share in the host's own clock.
    pub range: FrameRange,
    /// Frames from the attachment's start to this share's start.
    pub elapsed: i64,
}

fn invalid(message: impl Into<String>) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

impl ProjectDocument {
    /// The hosts beneath `node` that carry an attachment over `range` of its
    /// clock, in time order; zero-duration children take no share.
    pub fn attachment_spans(
        &self,
        node: &NodeId,
        range: std::ops::Range<i64>,
        what: &str,
    ) -> Result<Vec<HostSpan>, EditError> {
        if range.start < 0 || range.end <= range.start {
            return Err(invalid(format!("a {what} covers a nonempty range")));
        }
        let durations = self.exact_durations()?;
        let mut spans = Vec::new();
        collect(self, &durations, node, range, 0, what, &mut spans, 0)?;
        Ok(spans)
    }

    /// `SetCaptions` for every host beneath `node` that shows `caption` over
    /// `range` of its clock (`caption.range` is replaced by each share).
    pub fn caption_over(
        &self,
        node: &NodeId,
        range: std::ops::Range<i64>,
        caption: &Caption,
    ) -> Result<Vec<Command>, EditError> {
        let mut hosts: Vec<(NodeId, Vec<Caption>)> = Vec::new();
        for span in self.attachment_spans(node, range, "caption")? {
            let captions = entry(&mut hosts, &span.host, || {
                self.nodes()[&span.host].captions.clone()
            });
            let position =
                captions.partition_point(|existing| existing.range.start() <= span.range.start());
            captions.insert(
                position,
                Caption {
                    range: span.range,
                    ..caption.clone()
                },
            );
            crate::caption::validate(captions).map_err(|_| {
                invalid("this beat already shows a caption there at that placement; clear it first")
            })?;
        }
        Ok(hosts
            .into_iter()
            .map(|(node, captions)| Command::SetCaptions { node, captions })
            .collect())
    }

    /// `SetCutaways` for every host beneath `node` that shows `cutaway` over
    /// `range` of its clock. Each share starts the selection where the whole
    /// cutaway would be at that frame, so every picture is the same.
    pub fn cutaway_over(
        &self,
        node: &NodeId,
        range: std::ops::Range<i64>,
        cutaway: &Cutaway,
        rate: FrameRate,
    ) -> Result<Vec<Command>, EditError> {
        let mut hosts: Vec<(NodeId, Vec<Cutaway>)> = Vec::new();
        for span in self.attachment_spans(node, range, "cutaway")? {
            let Some(selection) = share_selection(cutaway, &span, rate)? else {
                continue;
            };
            let cutaways = entry(&mut hosts, &span.host, || {
                self.nodes()[&span.host].cutaways.clone()
            });
            if cutaways.iter().any(|existing| {
                existing.range.start() < span.range.end()
                    && span.range.start() < existing.range.end()
            }) {
                return Err(invalid(
                    "this beat already shows a cutaway there; clear it first",
                ));
            }
            let position =
                cutaways.partition_point(|existing| existing.range.start() < span.range.start());
            cutaways.insert(
                position,
                Cutaway {
                    range: span.range,
                    selection,
                    ..cutaway.clone()
                },
            );
            crate::cutaway::validate(cutaways, self.assets())?;
        }
        Ok(hosts
            .into_iter()
            .map(|(node, cutaways)| Command::SetCutaways { node, cutaways })
            .collect())
    }
}

fn entry<'a, T>(
    hosts: &'a mut Vec<(NodeId, Vec<T>)>,
    host: &NodeId,
    current: impl FnOnce() -> Vec<T>,
) -> &'a mut Vec<T> {
    let index = match hosts.iter().position(|(id, _)| id == host) {
        Some(index) => index,
        None => {
            hosts.push((host.clone(), current()));
            hosts.len() - 1
        }
    };
    &mut hosts[index].1
}

#[allow(clippy::too_many_arguments)]
fn collect(
    document: &ProjectDocument,
    durations: &BTreeMap<NodeId, ExactRatio>,
    node: &NodeId,
    range: std::ops::Range<i64>,
    elapsed: i64,
    what: &str,
    spans: &mut Vec<HostSpan>,
    depth: usize,
) -> Result<(), EditError> {
    if depth > MAX_DOCUMENT_DEPTH {
        return Err(invalid("the group is nested too deeply"));
    }
    if let Some((host, offset)) = crate::cutaway_host(document, node) {
        let shift = |frame: i64| {
            frame
                .checked_add(offset)
                .map(ProjectFrame)
                .ok_or_else(|| EditError::new(EditErrorCode::TimingOverflow, "frame overflows"))
        };
        spans.push(HostSpan {
            host,
            range: FrameRange::new(shift(range.start)?, shift(range.end)?)
                .map_err(crate::DocumentError::from)?,
            elapsed,
        });
        return Ok(());
    }
    let beat = &document.nodes()[node];
    let NodeKind::Sequence { children } = &beat.kind else {
        let kind = match beat.kind {
            NodeKind::Repeat { .. } => "Repeat, whose plays would each need it",
            NodeKind::Retime { .. } => "speed change, whose content plays at another rate",
            _ => "beat that cannot carry it",
        };
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            format!(
                "a {what} over this group reaches \"{}\", a {kind}; place the {what} inside or around that beat instead",
                beat.label
            ),
        ));
    };
    let mut start = 0_i64;
    for child in children {
        let duration = durations[child];
        if duration.denominator() != 1 {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                format!(
                    "a {what} over this group meets a beat with a fractional duration; place it inside one beat instead"
                ),
            ));
        }
        let end = start
            .checked_add(i64::try_from(duration.numerator()).map_err(|_| overflow())?)
            .ok_or_else(overflow)?;
        let (from, to) = (range.start.max(start), range.end.min(end));
        if from < to {
            collect(
                document,
                durations,
                child,
                (from - start)..(to - start),
                elapsed + (from - range.start),
                what,
                spans,
                depth + 1,
            )?;
        }
        start = end;
    }
    Ok(())
}

fn overflow() -> EditError {
    EditError::new(EditErrorCode::TimingOverflow, "group duration overflows")
}

/// The selection a share shows so its pictures match the whole cutaway at the
/// same frames, or `None` for a `Gap` share after the pictures ran out.
fn share_selection(
    cutaway: &Cutaway,
    span: &HostSpan,
    rate: FrameRate,
) -> Result<Option<ExactSourceSpan>, EditError> {
    if span.elapsed == 0 {
        return Ok(Some(cutaway.selection));
    }
    let time = |error: crate::TimeError| EditError::from(crate::DocumentError::from(error));
    let start = cutaway.selection.start();
    let end = cutaway.selection.end();
    let base = start.time_base;
    // Source ticks per project frame, as `Cutaway::picture_point` counts them.
    let per_frame = ExactRatio::new(
        i128::from(rate.denominator()) * i128::from(base.denominator()),
        i128::from(rate.numerator()) * i128::from(base.numerator()),
    )
    .map_err(time)?;
    let length = end.ticks.checked_sub(start.ticks).map_err(time)?;
    let ticks = |frames: i64| ExactRatio::integer(frames).checked_mul(per_frame);
    let into = ticks(span.elapsed).map_err(time)?;
    let through = ticks(span.elapsed + span.range.duration().frames()).map_err(time)?;
    let point = |ticks: ExactRatio| SourcePoint {
        ticks,
        time_base: base,
    };
    let looping = matches!(cutaway.fit, CutawayFit::Loop | CutawayFit::Bounce);
    if looping && !cutaway.removed && through.compare(length).is_gt() {
        return Err(invalid(
            "a looping or bouncing cutaway over this group would repeat inside a later beat, which one cutaway per beat cannot show; use fit=hold or fit=gap, or place it inside one beat",
        ));
    }
    if into.compare(length).is_lt() {
        let start = start.ticks.checked_add(into).map_err(time)?;
        return ExactSourceSpan::new(point(start), end)
            .map(Some)
            .map_err(time);
    }
    if cutaway.removed {
        // Every removed picture was recorded by an earlier share; this one
        // still removes the host's picture and records the final one.
        let half = per_frame
            .checked_div(ExactRatio::integer(2))
            .map_err(time)?;
        let from = end.ticks.checked_sub(half).map_err(time)?;
        return ExactSourceSpan::new(point(from), end)
            .map(Some)
            .map_err(time);
    }
    match cutaway.fit {
        CutawayFit::Gap => Ok(None),
        // Every picture center of this share is at least half a frame into
        // the selection, so it holds the final picture as the whole would.
        CutawayFit::Hold => {
            let half = per_frame
                .checked_div(ExactRatio::integer(2))
                .map_err(time)?;
            let from = end.ticks.checked_sub(half).map_err(time)?;
            ExactSourceSpan::new(point(from), end)
                .map(Some)
                .map_err(time)
        }
        CutawayFit::Loop | CutawayFit::Bounce => unreachable!("refused above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AssetId, AssetRecord, BeatNode, CaptionPlacement, FrameDuration, HoldAudio, HoldRecipe,
        HoldVideo, IterationOrder, ProjectId, RevisionId, SourceSpan, SourceTimeBase,
        SourceTimestamp,
    };
    use std::num::NonZeroU32;

    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }

    fn hold(frames: i64) -> BeatNode {
        BeatNode::hold(
            "Held",
            HoldRecipe {
                duration: FrameDuration::new(frames).unwrap(),
                picture_context: None,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        )
    }

    fn stamp(ticks: i64) -> SourceTimestamp {
        SourceTimestamp {
            ticks,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        }
    }

    /// root: [lead 3, group [a 4, inner [b 2, c 3], d 5], tail 2], with an
    /// optional Repeat `r` of `d` in place of `d`.
    fn document(repeat: bool) -> ProjectDocument {
        let mut document = ProjectDocument::new(
            ProjectId::new("attachments").unwrap(),
            RevisionId::new("base").unwrap(),
            crate::basis::default_basis(),
            id("root"),
        )
        .unwrap();
        let last = if repeat { "r" } else { "d" };
        let mut nodes = vec![
            (
                "root",
                BeatNode::sequence("Root", vec![id("lead"), id("group"), id("tail")]),
            ),
            (
                "group",
                BeatNode::sequence("Group", vec![id("a"), id("inner"), id(last)]),
            ),
            ("inner", BeatNode::sequence("Inner", vec![id("b"), id("c")])),
            ("lead", hold(3)),
            ("a", hold(4)),
            ("b", hold(2)),
            ("c", hold(3)),
            ("d", hold(5)),
            ("tail", hold(2)),
        ];
        if repeat {
            let mut wrapped = BeatNode::sequence("Echo", vec![]);
            wrapped.kind = NodeKind::Repeat {
                child: id("d"),
                iterations: IterationOrder::new(RevisionId::new("r").unwrap(), 2).unwrap(),
                gap: None,
                escalation: None,
            };
            nodes.push(("r", wrapped));
        }
        document
            .nodes
            .extend(nodes.into_iter().map(|(name, node)| (id(name), node)));
        document.assets.insert(
            AssetId::new("original").unwrap(),
            AssetRecord {
                source_qualification: None,
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: Some(SourceSpan::new(stamp(0), stamp(300)).unwrap()),
                audio: None,
                still_image: false,
                frame_count: None,
            },
        );
        document.validate().unwrap();
        document
    }

    fn apply_all(document: &ProjectDocument, commands: Vec<Command>) -> ProjectDocument {
        let mut result = document.clone();
        for command in commands {
            match command {
                Command::SetCaptions { node, captions } => {
                    result.nodes.get_mut(&node).unwrap().captions = captions;
                }
                Command::SetCutaways { node, cutaways } => {
                    result.nodes.get_mut(&node).unwrap().cutaways = cutaways;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        result.validate().unwrap();
        result
    }

    /// The host and host-local picture center that shows group frame `frame`.
    fn host_at(frame: i64) -> (NodeId, i64) {
        [("a", 4), ("b", 2), ("c", 3), ("d", 5)]
            .iter()
            .scan(0, |start, (name, length)| {
                let span = (*start, *start + length, *name);
                *start += length;
                Some(span)
            })
            .find(|(start, end, _)| (*start..*end).contains(&frame))
            .map(|(start, _, name)| (id(name), frame - start))
            .unwrap()
    }

    #[test]
    fn a_caption_over_a_group_shows_on_exactly_the_same_frames_from_each_host() {
        let before = document(false);
        let caption = Caption {
            range: FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
            text: "Wait for it".into(),
            placement: CaptionPlacement::Top,
            reveal: NonZeroU32::new(2),
        };
        let spans = before
            .attachment_spans(&id("group"), 2..11, "caption")
            .unwrap();
        let shares: Vec<_> = spans
            .iter()
            .map(|span| {
                (
                    span.host.as_str().to_owned(),
                    span.range.start().0,
                    span.range.end().0,
                    span.elapsed,
                )
            })
            .collect();
        assert_eq!(
            shares,
            [
                ("a".into(), 2, 4, 0),
                ("b".into(), 0, 2, 2),
                ("c".into(), 0, 3, 4),
                ("d".into(), 0, 2, 7)
            ]
        );
        let after = apply_all(
            &before,
            before.caption_over(&id("group"), 2..11, &caption).unwrap(),
        );
        for frame in 0..14 {
            let (host, local) = host_at(frame);
            let center = ExactRatio::new(i128::from(local) * 2 + 1, 2).unwrap();
            for play in [0, 1] {
                let shown: Vec<_> = after.nodes()[&host]
                    .captions
                    .iter()
                    .filter(|caption| caption.shows(center, Some(play)))
                    .collect();
                let expected = (2..11).contains(&frame) && play >= 1;
                assert_eq!(
                    shown.len(),
                    usize::from(expected),
                    "frame {frame} play {play}"
                );
                for shown in shown {
                    assert_eq!(
                        (&shown.text, shown.placement, shown.reveal),
                        (&caption.text, caption.placement, caption.reveal)
                    );
                }
            }
        }
        // A second caption at the same placement refuses before any change.
        let error = after
            .caption_over(&id("group"), 0..3, &caption)
            .unwrap_err();
        assert!(error.message.contains("already shows a caption"), "{error}");
    }

    #[test]
    fn a_repeat_inside_the_range_refuses_because_reveal_would_count_its_plays() {
        let before = document(true);
        let caption = Caption {
            range: FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
            text: "Third time".into(),
            placement: CaptionPlacement::Bottom,
            reveal: NonZeroU32::new(3),
        };
        let error = before
            .caption_over(&id("group"), 0..12, &caption)
            .unwrap_err();
        assert_eq!(error.code, EditErrorCode::WrongNodeKind);
        assert_eq!(
            error.message,
            "a caption over this group reaches \"Echo\", a Repeat, whose plays would each need it; place the caption inside or around that beat instead"
        );
        // A range that stops before the Repeat needs no share there.
        assert_eq!(
            before
                .caption_over(&id("group"), 0..9, &caption)
                .unwrap()
                .len(),
            3
        );
    }

    fn cutaway(fit: CutawayFit, pictures: i64) -> Cutaway {
        Cutaway {
            range: FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
            asset: AssetId::new("original").unwrap(),
            selection: SourceSpan::new(stamp(100), stamp(100 + pictures))
                .unwrap()
                .into(),
            fit,
            removed: false,
        }
    }

    /// Every group frame shows the same picture (or the host) through the
    /// shares as one cutaway over `[from, to)` of the group clock would.
    fn assert_same_pictures(whole: &Cutaway, from: i64, to: i64) {
        let before = document(false);
        let rate = before.presentation_basis().frame_rate;
        let after = apply_all(
            &before,
            before
                .cutaway_over(&id("group"), from..to, whole, rate)
                .unwrap(),
        );
        let whole = Cutaway {
            range: FrameRange::new(ProjectFrame(from), ProjectFrame(to)).unwrap(),
            ..whole.clone()
        };
        for frame in 0..14 {
            let (host, local) = host_at(frame);
            let center = |at: i64| ExactRatio::new(i128::from(at) * 2 + 1, 2).unwrap();
            let expected = if (from..to).contains(&frame) {
                whole.picture_point(center(frame), rate).unwrap()
            } else {
                None
            };
            let shown = after.nodes()[&host]
                .cutaways
                .iter()
                .find(|cutaway| (cutaway.range.start().0..cutaway.range.end().0).contains(&local))
                .and_then(|cutaway| cutaway.picture_point(center(local), rate).unwrap());
            assert_eq!(shown, expected, "frame {frame}");
        }
    }

    #[test]
    fn cutaway_shares_show_the_same_picture_on_every_frame() {
        // Longer than the range, shorter than the first share, and between.
        for pictures in [40, 2, 7] {
            assert_same_pictures(&cutaway(CutawayFit::Hold, pictures), 1, 13);
            assert_same_pictures(&cutaway(CutawayFit::Gap, pictures), 1, 13);
        }
        assert_same_pictures(&cutaway(CutawayFit::Loop, 40), 1, 13);
        assert_same_pictures(&cutaway(CutawayFit::Bounce, 40), 1, 13);
        // A loop inside its first share needs no later share's repetition.
        assert_same_pictures(&cutaway(CutawayFit::Loop, 1), 0, 4);
        // A video-only delete removes the host picture in every share.
        let before = document(false);
        let removed = Cutaway {
            removed: true,
            ..cutaway(CutawayFit::Loop, 3)
        };
        let after = apply_all(
            &before,
            before
                .cutaway_over(
                    &id("group"),
                    1..13,
                    &removed,
                    before.presentation_basis().frame_rate,
                )
                .unwrap(),
        );
        let covered: i64 = ["a", "b", "c", "d"]
            .iter()
            .flat_map(|host| after.nodes()[&id(host)].cutaways.iter())
            .inspect(|cutaway| assert!(cutaway.removed))
            .map(|cutaway| cutaway.range.duration().frames())
            .sum();
        assert_eq!(covered, 12);
    }

    #[test]
    fn a_cutaway_that_loops_into_a_later_beat_refuses() {
        let before = document(false);
        let rate = before.presentation_basis().frame_rate;
        let error = before
            .cutaway_over(&id("group"), 0..8, &cutaway(CutawayFit::Loop, 3), rate)
            .unwrap_err();
        assert!(error.message.contains("use fit=hold or fit=gap"), "{error}");
    }
}
