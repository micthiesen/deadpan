//! Exact, revision-aware boundary targeting. This module resolves coordinates;
//! it does not mutate a document or infer which repeated occurrence a user meant.

use std::{cmp::Ordering, collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    AssetId, ExactRatio, FrameDuration, FrameRange, InstancePath, MIX_SAMPLE_RATE, MarkId,
    MarkState, NodeId, NodeKind, ProjectDocument, ProjectFrame, ProjectId, RevisionId, SourceSpan,
    SourceTimeBase, SourceTimestamp, SourceVideo, TimeError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertionBias {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaRole {
    Linked,
    Video,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStream {
    Video,
    Audio,
}

/// Original stream coordinates. Audio sample indices use an explicit original
/// sample clock, including its signed origin, never the project's mix clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceMoment {
    Timestamp {
        stream: SourceStream,
        timestamp: SourceTimestamp,
    },
    AudioSample {
        sample: i64,
        sample_rate: u32,
    },
}

/// Positions are boundaries, so a host's end is legal. Local positions retain
/// exact fractions introduced by retiming; quantization occurs only on output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "space", rename_all = "snake_case", deny_unknown_fields)]
pub enum Anchor {
    Source {
        asset: AssetId,
        moment: SourceMoment,
    },
    Local {
        node: NodeId,
        position: ExactRatio,
    },
    Occurrence {
        instance: InstancePath,
        position: ExactRatio,
    },
    Sequence {
        frame: ProjectFrame,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryAnchor {
    pub coordinate: Anchor,
    pub bias: InsertionBias,
}

/// A Local anchor under a Repeat and every Source anchor require an explicit
/// occurrence. An Occurrence anchor already carries it; Sequence has no scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnchorTarget {
    pub boundary: BoundaryAnchor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<InstancePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedMarkTarget {
    pub id: MarkId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<InstancePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BoundarySelector {
    Point {
        target: AnchorTarget,
    },
    Range {
        start: AnchorTarget,
        end: AnchorTarget,
    },
    Mark {
        target: NamedMarkTarget,
    },
    MarkRange {
        start: NamedMarkTarget,
        end: NamedMarkTarget,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionRequest {
    pub project_id: ProjectId,
    pub expected_revision: RevisionId,
    pub role: MediaRole,
    pub selector: BoundarySelector,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedBoundary {
    pub target: AnchorTarget,
    /// Exact project-frame boundary before the single ties-to-even rounding.
    pub exact_frame: ExactRatio,
    pub frame: ProjectFrame,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolvedSelectionKind {
    Point {
        point: ResolvedBoundary,
    },
    Range {
        start: Box<ResolvedBoundary>,
        end: Box<ResolvedBoundary>,
        frames: FrameRange,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedSelection {
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub role: MediaRole,
    pub selection: ResolvedSelectionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AnchorErrorCode {
    ProjectConflict,
    RevisionConflict,
    InvalidAnchor,
    OccurrenceRequired,
    OccurrenceInvalid,
    SourceUnavailable,
    OutsideMapping,
    OutOfRange,
    InvalidRange,
    TimingOverflow,
    InvalidDocument,
    MarkMissing,
    MarkUnresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnchorError {
    pub code: AnchorErrorCode,
    pub message: String,
    pub current_revision: Option<RevisionId>,
}
impl AnchorError {
    fn new(code: AnchorErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            current_revision: None,
        }
    }
    pub fn code(&self) -> &'static str {
        match self.code {
            AnchorErrorCode::ProjectConflict => "ProjectConflict",
            AnchorErrorCode::RevisionConflict => "RevisionConflict",
            AnchorErrorCode::InvalidAnchor => "InvalidAnchor",
            AnchorErrorCode::OccurrenceRequired => "OccurrenceRequired",
            AnchorErrorCode::OccurrenceInvalid => "OccurrenceInvalid",
            AnchorErrorCode::SourceUnavailable => "SourceUnavailable",
            AnchorErrorCode::OutsideMapping => "OutsideMapping",
            AnchorErrorCode::OutOfRange => "OutOfRange",
            AnchorErrorCode::InvalidRange => "InvalidRange",
            AnchorErrorCode::TimingOverflow => "TimingOverflow",
            AnchorErrorCode::InvalidDocument => "InvalidDocument",
            AnchorErrorCode::MarkMissing => "MarkMissing",
            AnchorErrorCode::MarkUnresolved => "MarkUnresolved",
        }
    }
}
impl fmt::Display for AnchorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl Error for AnchorError {}
impl From<TimeError> for AnchorError {
    fn from(error: TimeError) -> Self {
        Self::new(AnchorErrorCode::TimingOverflow, error.to_string())
    }
}

/// A validated, borrowed immutable revision. Construction scales with authored
/// structure, not expanded plays; repeated queries reuse parent/prefix indexes.
/// Repeat identity lookups currently scan compact runs, never individual plays.
pub struct AnchorIndex<'a> {
    pub(crate) document: &'a ProjectDocument,
    pub(crate) durations: BTreeMap<NodeId, FrameDuration>,
    pub(crate) parents: BTreeMap<NodeId, (NodeId, i64)>,
    pub(crate) repeats: BTreeMap<NodeId, crate::RepeatLayout>,
}

impl<'a> AnchorIndex<'a> {
    pub fn new(document: &'a ProjectDocument) -> Result<Self, AnchorError> {
        let durations = document.durations().map_err(|error| {
            AnchorError::new(AnchorErrorCode::InvalidDocument, error.to_string())
        })?;
        Self::from_durations(document, durations)
            .map_err(|error| AnchorError::new(AnchorErrorCode::InvalidDocument, error.to_string()))
    }

    pub(crate) fn from_durations(
        document: &'a ProjectDocument,
        durations: BTreeMap<NodeId, FrameDuration>,
    ) -> Result<Self, crate::DocumentError> {
        let mut parents = BTreeMap::new();
        let mut repeats = BTreeMap::new();
        for (id, node) in document.nodes() {
            let mut offset = 0;
            for child in document.children(id) {
                parents.insert(child.clone(), (id.clone(), offset));
                if matches!(node.kind, NodeKind::Sequence { .. }) {
                    offset += durations[child].frames();
                }
            }
            if let NodeKind::Repeat {
                child,
                iterations,
                gap,
            } = &node.kind
            {
                repeats.insert(
                    id.clone(),
                    crate::RepeatLayout::compile(
                        iterations,
                        child,
                        document.overrides().get(id),
                        gap.as_ref().map_or(FrameDuration::ZERO, |gap| gap.duration),
                        &durations,
                    )?,
                );
            }
        }
        Ok(Self {
            document,
            durations,
            parents,
            repeats,
        })
    }

    pub fn resolve(&self, request: &SelectionRequest) -> Result<ResolvedSelection, AnchorError> {
        if &request.project_id != self.document.project_id() {
            return Err(AnchorError::new(
                AnchorErrorCode::ProjectConflict,
                "selector targets a different project",
            ));
        }
        if &request.expected_revision != self.document.revision_id() {
            return Err(AnchorError {
                code: AnchorErrorCode::RevisionConflict,
                message: format!(
                    "expected revision {}; current revision is {}",
                    request.expected_revision,
                    self.document.revision_id()
                ),
                current_revision: Some(self.document.revision_id().clone()),
            });
        }
        let selection = match &request.selector {
            BoundarySelector::Point { target } => ResolvedSelectionKind::Point {
                point: self.resolve_target(target)?,
            },
            BoundarySelector::Range { start, end } => {
                Self::range(self.resolve_target(start)?, self.resolve_target(end)?)?
            }
            BoundarySelector::Mark { target } => ResolvedSelectionKind::Point {
                point: self.resolve_mark(target)?,
            },
            BoundarySelector::MarkRange { start, end } => {
                Self::range(self.resolve_mark(start)?, self.resolve_mark(end)?)?
            }
        };
        Ok(ResolvedSelection {
            project_id: request.project_id.clone(),
            revision_id: request.expected_revision.clone(),
            role: request.role,
            selection,
        })
    }

    fn resolve_mark(&self, target: &NamedMarkTarget) -> Result<ResolvedBoundary, AnchorError> {
        let mark = self.document.marks().get(&target.id).ok_or_else(|| {
            AnchorError::new(
                AnchorErrorCode::MarkMissing,
                format!("mark {} does not exist", target.id),
            )
        })?;
        if let MarkState::Unresolved { reason } = mark.state {
            return Err(AnchorError::new(
                AnchorErrorCode::MarkUnresolved,
                format!("mark {} is unresolved: {reason:?}", target.id),
            ));
        }
        self.resolve_target(&AnchorTarget {
            boundary: mark.boundary.clone(),
            occurrence: target.occurrence.clone(),
        })
    }

    fn range(
        start: ResolvedBoundary,
        end: ResolvedBoundary,
    ) -> Result<ResolvedSelectionKind, AnchorError> {
        if end
            .exact_frame
            .checked_sub(start.exact_frame)?
            .compare_integer(0)
            != Ordering::Greater
        {
            return Err(AnchorError::new(
                AnchorErrorCode::InvalidRange,
                "range end must follow its start; reversed endpoints are not swapped",
            ));
        }
        if end.frame <= start.frame {
            return Err(AnchorError::new(
                AnchorErrorCode::InvalidRange,
                "range collapses after project-frame quantization",
            ));
        }
        let frames = FrameRange::new(start.frame, end.frame)?;
        Ok(ResolvedSelectionKind::Range {
            start: Box::new(start),
            end: Box::new(end),
            frames,
        })
    }

    pub fn resolve_target(&self, target: &AnchorTarget) -> Result<ResolvedBoundary, AnchorError> {
        let exact_frame = match &target.boundary.coordinate {
            Anchor::Sequence { frame } => {
                reject_scope(target)?;
                ExactRatio::integer(frame.0)
            }
            Anchor::Local { node, position } => {
                let path = target.occurrence.clone().unwrap_or(InstancePath {
                    node: node.clone(),
                    repeats: vec![],
                });
                if &path.node != node {
                    return Err(AnchorError::new(
                        AnchorErrorCode::OccurrenceInvalid,
                        "occurrence target differs from local anchor host",
                    ));
                }
                self.to_project(&path, *position, target.occurrence.is_none())?
            }
            Anchor::Occurrence { instance, position } => {
                reject_scope(target)?;
                self.to_project(instance, *position, false)?
            }
            Anchor::Source { asset, moment } => {
                let path = target.occurrence.as_ref().ok_or_else(|| {
                    AnchorError::new(
                        AnchorErrorCode::OccurrenceRequired,
                        "source anchors require an explicit source occurrence",
                    )
                })?;
                self.validate_path(path, false)?;
                let local = self.source_position(asset, *moment, &path.node)?;
                self.to_project(path, local, false)?
            }
        };
        within(
            exact_frame,
            self.durations[self.document.root()].frames(),
            AnchorErrorCode::OutOfRange,
        )?;
        let frame = ProjectFrame(
            i64::try_from(exact_frame.round_even()?).map_err(|_| TimeError::Overflow)?,
        );
        Ok(ResolvedBoundary {
            target: target.clone(),
            exact_frame,
            frame,
        })
    }

    fn validate_path(&self, path: &InstancePath, implicit: bool) -> Result<(), AnchorError> {
        path.validate_depth()
            .map_err(|e| AnchorError::new(AnchorErrorCode::OccurrenceInvalid, e.to_string()))?;
        if !self.document.nodes().contains_key(&path.node) {
            return Err(AnchorError::new(
                AnchorErrorCode::InvalidAnchor,
                "anchor host does not exist",
            ));
        }
        let mut node = &path.node;
        let mut step = path.repeats.len();
        while let Some((parent, _)) = self.parents.get(node) {
            if let NodeKind::Repeat { iterations, .. } = &self.document.nodes()[parent].kind {
                step = step.checked_sub(1).ok_or_else(|| {
                    AnchorError::new(
                        if implicit {
                            AnchorErrorCode::OccurrenceRequired
                        } else {
                            AnchorErrorCode::OccurrenceInvalid
                        },
                        "anchor occurrence omits a Repeat ancestor",
                    )
                })?;
                let selected = &path.repeats[step];
                if &selected.node != parent
                    || iterations.position(&selected.iteration).is_none()
                    || self.repeats[parent]
                        .play(&selected.iteration)
                        .is_none_or(|play| &play.child != node)
                {
                    return Err(AnchorError::new(
                        AnchorErrorCode::OccurrenceInvalid,
                        "occurrence names a wrong Repeat or retired play",
                    ));
                }
            }
            node = parent;
        }
        if step != 0 {
            return Err(AnchorError::new(
                AnchorErrorCode::OccurrenceInvalid,
                "occurrence has extra Repeat ancestors",
            ));
        }
        Ok(())
    }

    pub(crate) fn to_project(
        &self,
        path: &InstancePath,
        mut position: ExactRatio,
        implicit: bool,
    ) -> Result<ExactRatio, AnchorError> {
        self.validate_path(path, implicit)?;
        within(
            position,
            self.durations[&path.node].frames(),
            AnchorErrorCode::OutOfRange,
        )?;
        let mut node = &path.node;
        let mut step = path.repeats.len();
        while let Some((parent, offset)) = self.parents.get(node) {
            position = match &self.document.nodes()[parent].kind {
                NodeKind::Sequence { .. } => position.checked_add(ExactRatio::integer(*offset))?,
                NodeKind::Repeat { .. } => {
                    step -= 1; // validate_path checked ordered ancestry and effective child.
                    let play = self.repeats[parent]
                        .play(&path.repeats[step].iteration)
                        .ok_or_else(|| {
                            AnchorError::new(
                                AnchorErrorCode::OccurrenceInvalid,
                                "iteration disappeared from immutable index",
                            )
                        })?;
                    position.checked_add(ExactRatio::integer(play.start))?
                }
                NodeKind::Retime {
                    mapping, duration, ..
                } => {
                    let selected = position.checked_sub(ExactRatio::integer(mapping.start().0))?;
                    within(
                        selected,
                        mapping.duration().frames(),
                        AnchorErrorCode::OutsideMapping,
                    )?;
                    selected.checked_mul(ExactRatio::new(
                        i128::from(duration.frames()),
                        i128::from(mapping.duration().frames()),
                    )?)?
                }
                _ => {
                    return Err(AnchorError::new(
                        AnchorErrorCode::InvalidDocument,
                        "leaf cannot be an anchor parent",
                    ));
                }
            };
            node = parent;
        }
        Ok(position)
    }

    fn source_position(
        &self,
        asset: &AssetId,
        moment: SourceMoment,
        host: &NodeId,
    ) -> Result<ExactRatio, AnchorError> {
        let record = self.document.assets().get(asset).ok_or_else(|| {
            AnchorError::new(
                AnchorErrorCode::SourceUnavailable,
                "source asset does not exist",
            )
        })?;
        let (stream, timestamp) = match moment {
            SourceMoment::Timestamp { stream, timestamp } => (stream, timestamp),
            SourceMoment::AudioSample {
                sample,
                sample_rate,
            } => {
                let time_base = SourceTimeBase::new(1, sample_rate).map_err(|_| {
                    AnchorError::new(
                        AnchorErrorCode::InvalidAnchor,
                        "original audio sample rate must be positive",
                    )
                })?;
                (
                    SourceStream::Audio,
                    SourceTimestamp {
                        ticks: sample,
                        time_base,
                    },
                )
            }
        };
        let original = match stream {
            SourceStream::Video => record.video,
            SourceStream::Audio => record.audio,
        }
        .ok_or_else(|| {
            AnchorError::new(
                AnchorErrorCode::SourceUnavailable,
                "asset has no selected original stream",
            )
        })?;
        source_fraction(timestamp, original)?;
        let NodeKind::Source { source } = &self.document.nodes()[host].kind else {
            return Err(AnchorError::new(
                AnchorErrorCode::SourceUnavailable,
                "source anchors require a Source beat; held pictures do not have a unique source-to-project boundary",
            ));
        };
        let (selected, duration, offset) = match stream {
            SourceStream::Video => match &source.video {
                SourceVideo::Stream {
                    asset: selected_asset,
                    span,
                } if selected_asset == asset => (
                    *span,
                    ExactRatio::integer(source.duration.frames()),
                    ExactRatio::ZERO,
                ),
                _ => {
                    return Err(AnchorError::new(
                        AnchorErrorCode::SourceUnavailable,
                        "occurrence does not use this video stream",
                    ));
                }
            },
            SourceStream::Audio => {
                let audio = source
                    .audio
                    .as_ref()
                    .filter(|a| &a.asset == asset)
                    .ok_or_else(|| {
                        AnchorError::new(
                            AnchorErrorCode::SourceUnavailable,
                            "occurrence does not use this audio stream",
                        )
                    })?;
                let rate = self.document.presentation_basis().frame_rate;
                let offset =
                    ExactRatio::integer(source.audio_offset.0).checked_mul(ExactRatio::new(
                        i128::from(rate.numerator()),
                        i128::from(MIX_SAMPLE_RATE) * i128::from(rate.denominator()),
                    )?)?;
                (
                    audio.span,
                    source.audio_mapping.duration_frames(source.duration)?,
                    offset,
                )
            }
        };
        source_fraction(timestamp, selected)?
            .checked_mul(duration)?
            .checked_add(offset)
            .map_err(Into::into)
    }
}

fn reject_scope(target: &AnchorTarget) -> Result<(), AnchorError> {
    if target.occurrence.is_some() {
        return Err(AnchorError::new(
            AnchorErrorCode::InvalidAnchor,
            "this anchor already determines its occurrence; extra scope is not allowed",
        ));
    }
    Ok(())
}

fn within(position: ExactRatio, end: i64, code: AnchorErrorCode) -> Result<(), AnchorError> {
    if position.compare_integer(0) == Ordering::Less
        || position.compare_integer(end) == Ordering::Greater
    {
        return Err(AnchorError::new(
            code,
            "boundary is outside the selected interval",
        ));
    }
    Ok(())
}

pub(crate) fn source_fraction(
    timestamp: SourceTimestamp,
    span: SourceSpan,
) -> Result<ExactRatio, AnchorError> {
    let from = timestamp.time_base;
    let to = span.start().time_base;
    let ticks = ExactRatio::integer(timestamp.ticks).checked_mul(ExactRatio::new(
        i128::from(from.numerator()) * i128::from(to.denominator()),
        i128::from(from.denominator()) * i128::from(to.numerator()),
    )?)?;
    let relative = ticks.checked_sub(ExactRatio::integer(span.start().ticks))?;
    let length = span.end().ticks - span.start().ticks;
    within(relative, length, AnchorErrorCode::OutsideMapping)?;
    relative
        .checked_div(ExactRatio::integer(length))
        .map_err(Into::into)
}
