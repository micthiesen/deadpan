//! Random sequences across the structural command family, checking the
//! transaction invariants every authored edit must keep:
//!
//! - the result validates and survives a JSON round trip;
//! - the forward patch applied to the input reproduces the result exactly and
//!   the inverse patch restores the input exactly (and replaying every inverse
//!   in reverse order restores the generated document);
//! - `duration_delta` equals the actual change and the change each command
//!   family promises;
//! - marks follow their loss policy: `KeepUnresolved` marks keep their logical
//!   identity, labels and policies never change, unresolved bindings never
//!   become bound, and time-neutral edits lose no bound mark;
//! - time-neutral edits keep every retained audio clock: each surviving
//!   unrepeated physical owner resolves the same lattice and resume, which is
//!   the core-level proxy for unchanged plan sample mapping (deadpan-plan is
//!   not a dependency of this crate);
//! - a refused command leaves its input untouched.
//!
//! Operations are generated abstractly (picks and fractions) and resolved
//! against the current document, so every case explores a different tree.
//! See docs/STRUCTURAL_PROPERTIES.md.

use std::collections::{BTreeMap, BTreeSet};

use deadpan_core::*;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::Config;
use serde_json::{Value, json};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn rev(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 48_000,
            time_base,
        },
    )
    .unwrap()
}
fn source(duration: i64) -> BeatNode {
    BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "Source".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(duration),
                video: SourceVideo::Stream {
                    asset: AssetId::new("original").unwrap(),
                    span: span(),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: AssetId::new("original").unwrap(),
                    span: span(),
                }),
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Linked,
            },
        },
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}
fn recipe(duration: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: frames(duration),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}

/// One generated initial beat: a Source, a Hold or a small group of both.
#[derive(Debug, Clone)]
enum Beat {
    Source(i64),
    Hold(i64),
    Group(Vec<(bool, i64)>),
}

fn beat() -> impl Strategy<Value = Beat> {
    prop_oneof![
        3 => (4_i64..=24).prop_map(Beat::Source),
        2 => (1_i64..=10).prop_map(Beat::Hold),
        1 => prop::collection::vec((any::<bool>(), 2_i64..=12), 1..=3).prop_map(Beat::Group),
    ]
}

fn document(beats: &[Beat]) -> ProjectDocument {
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    for (index, beat) in beats.iter().enumerate() {
        let name = id(&format!("b{index}"));
        match beat {
            Beat::Source(duration) => {
                nodes.insert(name.clone(), source(*duration));
            }
            Beat::Hold(duration) => {
                nodes.insert(name.clone(), BeatNode::hold("Hold", recipe(*duration)));
            }
            Beat::Group(members) => {
                let mut inner = Vec::new();
                for (offset, (is_source, duration)) in members.iter().enumerate() {
                    let member = id(&format!("b{index}-{offset}"));
                    nodes.insert(
                        member.clone(),
                        if *is_source {
                            source(*duration)
                        } else {
                            BeatNode::hold("Hold", recipe(*duration))
                        },
                    );
                    inner.push(member);
                }
                nodes.insert(name.clone(), BeatNode::sequence("Group", inner));
            }
        }
        children.push(name);
    }
    nodes.insert(id("root"), BeatNode::sequence("Project", children));
    let asset = AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: Some(span()),
        audio: Some(span()),
        still_image: false,
        frame_count: None,
        source_qualification: None,
    };
    ProjectDocument::from_json(
        &json!({
            "schema_version": DOCUMENT_SCHEMA_VERSION, "project_id": "properties",
            "revision_id": "initial",
            "presentation_basis": {"width": 16, "height": 16,
                "frame_rate": {"numerator": 30000, "denominator": 1001},
                "color_policy": "sdr_rec709"},
            "basis_state": {"rate_origin": "explicit", "geometry_origin": "explicit", "primary": null},
            "root": "root", "assets": {"original": asset}, "marks": {}, "overrides": {},
            "nodes": nodes,
        })
        .to_string(),
    )
    .unwrap()
}

// ---------------------------------------------------------------------------
// EXTENSION POINT: generated operations.
//
// Each variant is an abstract intent resolved against the current document by
// `Runner::resolve`. To cover a new structural command (for example Explode or
// Duplicate), add a variant here, a weighted arm in `operation()`, a resolution
// arm in `Runner::resolve` returning the `Command` and its promised duration
// change (`None` when the command family promises no fixed delta), and list it
// in `time_neutral` when it must preserve every retained audio clock.
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
enum Op {
    Split {
        pick: u16,
        at: u16,
    },
    DeleteRipple {
        pick: u16,
    },
    DeleteChildren {
        pick: u16,
        count: u8,
    },
    DeleteRange {
        start: u16,
        len: u16,
    },
    InsertTime {
        at: u16,
        frames: u8,
    },
    Paste {
        start: u16,
        len: u16,
        dest: u16,
    },
    PasteChild {
        pick: u16,
        dest: u16,
    },
    Replace {
        start: u16,
        len: u16,
        dest: u16,
        dest_len: u16,
    },
    Move {
        start: u16,
        len: u16,
        dest: u16,
    },
    GroupRange {
        start: u16,
        len: u16,
    },
    GroupChild {
        pick: u16,
    },
    Ungroup {
        pick: u16,
    },
    RepeatChild {
        pick: u16,
        plays: u8,
    },
    RepeatRange {
        start: u16,
        len: u16,
        plays: u8,
    },
    SetRepeatPlays {
        pick: u16,
        plays: u8,
    },
    SetRepeatGaps {
        pick: u16,
        frames: u8,
    },
    IsolateGap {
        pick: u16,
        play: u8,
    },
    WrapRetime {
        pick: u16,
        output: u8,
    },
    SetRetime {
        pick: u16,
        output: u8,
    },
    SetHoldDuration {
        pick: u16,
        frames: u8,
    },
    ScopedRename {
        pick: u16,
        play: u8,
    },
    OccurrenceRename {
        pick: u16,
        play: u8,
    },
    SetMark {
        pick: u16,
        at: u16,
        right: bool,
        keep: bool,
    },
    Explode {
        pick: u16,
    },
    DuplicateChild {
        pick: u16,
    },
    DuplicateRange {
        start: u16,
        len: u16,
    },
    ScopedRenameMany {
        pick: u16,
        first: u8,
        count: u8,
    },
}

fn operation() -> impl Strategy<Value = Op> {
    let p = any::<u16>;
    prop_oneof![
        3 => (p(), p()).prop_map(|(pick, at)| Op::Split { pick, at }),
        1 => p().prop_map(|pick| Op::DeleteRipple { pick }),
        1 => (p(), 1_u8..=3).prop_map(|(pick, count)| Op::DeleteChildren { pick, count }),
        2 => (p(), p()).prop_map(|(start, len)| Op::DeleteRange { start, len }),
        2 => (p(), 1_u8..=8).prop_map(|(at, frames)| Op::InsertTime { at, frames }),
        2 => (p(), p(), p()).prop_map(|(start, len, dest)| Op::Paste { start, len, dest }),
        1 => (p(), p()).prop_map(|(pick, dest)| Op::PasteChild { pick, dest }),
        1 => (p(), p(), p(), p())
            .prop_map(|(start, len, dest, dest_len)| Op::Replace { start, len, dest, dest_len }),
        2 => (p(), p(), p()).prop_map(|(start, len, dest)| Op::Move { start, len, dest }),
        2 => (p(), p()).prop_map(|(start, len)| Op::GroupRange { start, len }),
        1 => p().prop_map(|pick| Op::GroupChild { pick }),
        1 => p().prop_map(|pick| Op::Ungroup { pick }),
        2 => (p(), 2_u8..=4).prop_map(|(pick, plays)| Op::RepeatChild { pick, plays }),
        1 => (p(), p(), 2_u8..=3)
            .prop_map(|(start, len, plays)| Op::RepeatRange { start, len, plays }),
        2 => (p(), 1_u8..=5).prop_map(|(pick, plays)| Op::SetRepeatPlays { pick, plays }),
        2 => (p(), 0_u8..=4).prop_map(|(pick, frames)| Op::SetRepeatGaps { pick, frames }),
        2 => (p(), any::<u8>()).prop_map(|(pick, play)| Op::IsolateGap { pick, play }),
        1 => (p(), 1_u8..=30).prop_map(|(pick, output)| Op::WrapRetime { pick, output }),
        1 => (p(), 1_u8..=30).prop_map(|(pick, output)| Op::SetRetime { pick, output }),
        1 => (p(), 1_u8..=12).prop_map(|(pick, frames)| Op::SetHoldDuration { pick, frames }),
        2 => (p(), any::<u8>()).prop_map(|(pick, play)| Op::ScopedRename { pick, play }),
        1 => (p(), any::<u8>()).prop_map(|(pick, play)| Op::OccurrenceRename { pick, play }),
        2 => (p(), p(), any::<bool>(), any::<bool>())
            .prop_map(|(pick, at, right, keep)| Op::SetMark { pick, at, right, keep }),
        2 => p().prop_map(|pick| Op::Explode { pick }),
        1 => p().prop_map(|pick| Op::DuplicateChild { pick }),
        1 => (p(), p()).prop_map(|(start, len)| Op::DuplicateRange { start, len }),
        1 => (p(), any::<u8>(), 1_u8..=3)
            .prop_map(|(pick, first, count)| Op::ScopedRenameMany { pick, first, count }),
    ]
}

/// Edits whose rendered output and every retained clock must be unchanged.
fn time_neutral(op: &Op) -> bool {
    matches!(
        op,
        Op::Split { .. }
            | Op::GroupRange { .. }
            | Op::GroupChild { .. }
            | Op::Ungroup { .. }
            | Op::IsolateGap { .. }
            | Op::ScopedRename { .. }
            | Op::OccurrenceRename { .. }
            | Op::SetMark { .. }
            | Op::Explode { .. }
            | Op::ScopedRenameMany { .. }
    )
}

/// Edits that may not lose any bound mark binding's logical mark.
fn lossless(op: &Op) -> bool {
    time_neutral(op)
        || matches!(
            op,
            Op::Move { .. }
                | Op::Paste { .. }
                | Op::PasteChild { .. }
                | Op::DuplicateChild { .. }
                | Op::DuplicateRange { .. }
        )
}

/// Whether any binding names an authored node the edit removed. Ungroup
/// removes its wrapper: marks owned or hosted there follow their loss policy
/// (docs/GROUP_EDITING.md), while every other mark is unaffected.
fn touches_removed(mark: &Mark, before: &ProjectDocument, after: &ProjectDocument) -> bool {
    let removed =
        |node: &NodeId| before.nodes().contains_key(node) && !after.nodes().contains_key(node);
    mark.bindings().any(|binding| {
        removed(&binding.owner)
            || match &binding.coordinate {
                Anchor::Local { node, .. } => removed(node),
                Anchor::Occurrence { instance, .. } => removed(&instance.node),
                Anchor::Source { .. } | Anchor::Sequence { .. } => false,
            }
    })
}

struct Resolved {
    command: Command,
    /// The duration change this command family promises, when fixed.
    delta: Option<i64>,
}

struct Runner {
    document: ProjectDocument,
    step: u32,
    ids: u32,
    history: Vec<EditTransaction>,
}

fn pick<T: Clone>(values: &[T], pick: u16) -> Option<T> {
    (!values.is_empty()).then(|| values[usize::from(pick) % values.len()].clone())
}

impl Runner {
    fn fresh(&mut self, prefix: &str) -> NodeId {
        self.ids += 1;
        id(&format!("{prefix}{}", self.ids))
    }
    fn fresh_many(&mut self, prefix: &str, count: usize) -> Vec<NodeId> {
        (0..count).map(|_| self.fresh(prefix)).collect()
    }
    fn fresh_marks(&mut self, count: usize) -> Vec<MarkId> {
        (0..count)
            .map(|_| {
                self.ids += 1;
                MarkId::new(format!("m{}", self.ids)).unwrap()
            })
            .collect()
    }
    fn root(&self) -> NodeId {
        self.document.root().clone()
    }
    fn children(&self) -> Vec<NodeId> {
        self.document
            .children(self.document.root())
            .cloned()
            .collect()
    }
    fn total(&self) -> i64 {
        self.document.duration().unwrap().frames()
    }
    fn duration_of(&self, node: &NodeId) -> i64 {
        self.document.node_duration(node).unwrap().frames()
    }
    /// Root children with their absolute starts.
    fn spans(&self) -> Vec<(NodeId, i64, i64)> {
        let mut start = 0;
        self.children()
            .into_iter()
            .map(|child| {
                let end = start + self.duration_of(&child);
                let entry = (child, start, end);
                start = end;
                entry
            })
            .collect()
    }
    fn kind_children(&self, accept: impl Fn(&NodeKind) -> bool) -> Vec<NodeId> {
        self.children()
            .into_iter()
            .filter(|child| accept(&self.document.nodes()[child].kind))
            .collect()
    }
    /// A nonempty half-open root range from two generated values.
    fn range(&self, start: u16, len: u16) -> Option<FrameRange> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let start = i64::from(start) % total;
        let len = 1 + i64::from(len) % (total - start);
        FrameRange::new(ProjectFrame(start), ProjectFrame(start + len)).ok()
    }
    fn split_pool(&mut self, count: usize) -> SplitIdentities {
        SplitIdentities {
            nodes: self.fresh_many("s", count),
        }
    }
    fn paste_pool(&mut self, slice: &CapturedEditSlice) -> Option<SlicePasteIdentities> {
        let required = slice.identity_requirements().ok()?;
        Some(SlicePasteIdentities {
            authored: OccurrenceIdentities {
                nodes: self.fresh_many("p", required.nodes),
                marks: self.fresh_marks(required.marks),
            },
            aliases: self.fresh_many("a", required.aliases),
        })
    }
    fn capture(&self, selection: SliceCaptureSelection) -> Option<CapturedEditSlice> {
        CapturedEditSlice::capture_selection(
            &self.document,
            self.document.root(),
            &selection,
            AudioTimingId {
                allocation: rev("scratch-capture"),
                ordinal: 0,
            },
        )
        .ok()
    }
    /// A seam slot or a strictly interior child boundary at Edit frame `at`.
    fn placement(
        &mut self,
        at: i64,
        slice: CapturedEditSlice,
        timing: AudioTimingId,
    ) -> Option<Command> {
        let identities = self.paste_pool(&slice)?;
        let spans = self.spans();
        if let Some(index) = spans.iter().position(|(_, start, _)| *start == at) {
            return Some(Command::SpliceSlice {
                parent: self.root(),
                index,
                slice,
                identities,
                timing,
            });
        }
        if at == self.total() {
            return Some(Command::SpliceSlice {
                parent: self.root(),
                index: spans.len(),
                slice,
                identities,
                timing,
            });
        }
        let (target, start, _) = spans
            .into_iter()
            .find(|(_, start, end)| *start < at && at < *end)?;
        let local = frames(at - start);
        let interior = self
            .document
            .slice_splice_interior(&self.root(), &target, local, &slice)
            .ok()?;
        let split_identities = self.split_pool(interior.required_ids);
        Some(Command::SpliceSliceAt {
            parent: self.root(),
            target,
            at: local,
            slice,
            identities,
            split_identities,
            timing,
        })
    }

    fn duplicate(
        &mut self,
        selection: SliceCaptureSelection,
        timing: AudioTimingId,
    ) -> Option<Command> {
        let needs = self
            .document
            .duplicate_requirements(&self.root(), &selection, &timing)
            .ok()?;
        Some(Command::Duplicate {
            parent: self.root(),
            selection,
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: self.fresh_many("d", needs.slice.nodes),
                    marks: self.fresh_marks(needs.slice.marks),
                },
                aliases: self.fresh_many("da", needs.slice.aliases),
            },
            split_identities: self.split_pool(needs.split_nodes),
            timing,
        })
    }

    fn resolve(&mut self, op: &Op, revision: &RevisionId) -> Option<Resolved> {
        let timing = AudioTimingId {
            allocation: revision.clone(),
            ordinal: 0,
        };
        let root = self.root();
        let command = match op {
            Op::Split { pick: p, at } => {
                let candidates: Vec<_> = self
                    .spans()
                    .into_iter()
                    .filter(|(_, start, end)| end - start >= 2)
                    .collect();
                let (node, start, end) = pick(&candidates, *p)?;
                let local = 1 + i64::from(*at) % (end - start - 1);
                return Some(Resolved {
                    command: Command::Split {
                        node,
                        at: frames(local),
                        identities: self.split_pool(16),
                    },
                    delta: Some(0),
                });
            }
            Op::DeleteRipple { pick: p } => {
                let node = pick(&self.children(), *p)?;
                let delta = -self.duration_of(&node);
                return Some(Resolved {
                    command: Command::DeleteRipple { node, timing },
                    delta: Some(delta),
                });
            }
            Op::DeleteChildren { pick: p, count } => {
                let children = self.children();
                let first = usize::from(*p) % children.len().max(1);
                let last = (first + usize::from(*count) - 1).min(children.len().checked_sub(1)?);
                let delta = -children[first..=last]
                    .iter()
                    .map(|child| self.duration_of(child))
                    .sum::<i64>();
                Command::DeleteChildren {
                    parent: root,
                    first: children.get(first)?.clone(),
                    last: children[last].clone(),
                    timing,
                }
                .with(delta)
            }
            Op::DeleteRange { start, len } => {
                let range = self.range(*start, *len)?;
                let required = self
                    .document
                    .range_deletion(&root, range)
                    .ok()?
                    .required_ids;
                Command::DeleteRange {
                    parent: root,
                    range,
                    identities: self.split_pool(required),
                    timing,
                }
                .with(-range.duration().frames())
            }
            Op::InsertTime { at, frames: length } => {
                let at = i64::from(*at) % (self.total() + 1);
                Command::InsertTime {
                    at: ProjectFrame(at),
                    hold: recipe(i64::from(*length)),
                    id: self.fresh("t"),
                    identities: self.split_pool(4),
                    timing,
                }
                .with(i64::from(*length))
            }
            Op::Paste { start, len, dest } => {
                let range = self.range(*start, *len)?;
                let slice = self.capture(SliceCaptureSelection::Range { range })?;
                let delta = slice.duration().frames();
                let at = i64::from(*dest) % (self.total() + 1);
                self.placement(at, slice, timing)?.with(delta)
            }
            Op::PasteChild { pick: p, dest } => {
                let node = pick(&self.children(), *p)?;
                let slice = self.capture(SliceCaptureSelection::Child { node })?;
                let delta = slice.duration().frames();
                let at = i64::from(*dest) % (self.total() + 1);
                self.placement(at, slice, timing)?.with(delta)
            }
            Op::Replace {
                start,
                len,
                dest,
                dest_len,
            } => {
                let source = self.range(*start, *len)?;
                let slice = self.capture(SliceCaptureSelection::Range { range: source })?;
                let range = self.range(*dest, *dest_len)?;
                let plan = self.document.slice_replacement(&root, range, &slice).ok()?;
                let delta = slice.duration().frames() - range.duration().frames();
                let identities = self.paste_pool(&slice)?;
                Command::ReplaceSlice {
                    parent: root,
                    range,
                    slice,
                    identities,
                    split_identities: self.split_pool(plan.required_ids),
                    timing,
                }
                .with(delta)
            }
            Op::Move { start, len, dest } => {
                let range = self.range(*start, *len)?;
                let at = i64::from(*dest) % (self.total() + 1);
                let spans = self.spans();
                let destination = match spans.iter().position(|(_, start, _)| *start == at) {
                    Some(index) => MoveRangeDestination::Seam {
                        parent: root.clone(),
                        index,
                    },
                    None if at == self.total() => MoveRangeDestination::Seam {
                        parent: root.clone(),
                        index: spans.len(),
                    },
                    None => {
                        let (target, start, _) =
                            spans.into_iter().find(|(_, s, e)| *s < at && at < *e)?;
                        MoveRangeDestination::Interior {
                            parent: root.clone(),
                            target,
                            at: frames(at - start),
                        }
                    }
                };
                let plan = self.document.range_move(&root, range, &destination).ok()?;
                Command::MoveRange {
                    source_revision: self.document.revision_id().clone(),
                    source_parent: root,
                    range,
                    destination,
                    identities: self.split_pool(plan.required_ids),
                    timing,
                }
                .with(0)
            }
            Op::GroupRange { start, len } => {
                let selection = SliceCaptureSelection::Range {
                    range: self.range(*start, *len)?,
                };
                let plan = self.document.group_selection(&root, &selection).ok()?;
                Command::GroupSelection {
                    parent: root,
                    selection,
                    label: "Group".into(),
                    identities: GroupSelectionIdentities {
                        group: self.fresh("g"),
                        split: self.split_pool(plan.required_split_ids),
                    },
                    timing,
                }
                .with(0)
            }
            Op::GroupChild { pick: p } => {
                let selection = SliceCaptureSelection::Child {
                    node: pick(&self.children(), *p)?,
                };
                Command::GroupSelection {
                    parent: root,
                    selection,
                    label: "Group".into(),
                    identities: GroupSelectionIdentities {
                        group: self.fresh("g"),
                        split: self.split_pool(0),
                    },
                    timing,
                }
                .with(0)
            }
            Op::Ungroup { pick: p } => {
                let node = pick(
                    &self.kind_children(|kind| matches!(kind, NodeKind::Sequence { .. })),
                    *p,
                )?;
                Command::Ungroup { node }.with(0)
            }
            Op::RepeatChild { pick: p, plays } => {
                let selection = SliceCaptureSelection::Child {
                    node: pick(&self.children(), *p)?,
                };
                self.repeat(selection, u32::from(*plays), timing)?
            }
            Op::RepeatRange { start, len, plays } => {
                let selection = SliceCaptureSelection::Range {
                    range: self.range(*start, *len)?,
                };
                self.repeat(selection, u32::from(*plays), timing)?
            }
            Op::SetRepeatPlays { pick: p, plays } => {
                let node = pick(&self.repeats(), *p)?;
                let plays = u32::from(*plays);
                let delta = self.uniform_repeat(&node, plays, None);
                Resolved {
                    command: Command::SetRepeatPlays {
                        node,
                        plays,
                        timing,
                    },
                    delta,
                }
            }
            Op::SetRepeatGaps {
                pick: p,
                frames: gap,
            } => {
                let node = pick(&self.repeats(), *p)?;
                let gap = (*gap > 0).then(|| recipe(i64::from(*gap)));
                let delta = self.uniform_repeat(
                    &node,
                    self.plays(&node),
                    Some(gap.as_ref().map_or(0, |gap| gap.duration.frames())),
                );
                Resolved {
                    command: Command::SetRepeatGaps {
                        node,
                        gap,
                        branches: Vec::new(),
                        timing,
                    },
                    delta,
                }
            }
            Op::IsolateGap { pick: p, play } => {
                let node = pick(&self.repeats(), *p)?;
                let plays = self.plays(&node);
                let iteration = self.iteration(&node, u32::from(*play) % plays)?;
                Command::IsolateGap {
                    node,
                    iteration,
                    id: self.fresh("i"),
                    timing,
                }
                .with(0)
            }
            Op::WrapRetime { pick: p, output } => {
                let node = pick(&self.children(), *p)?;
                let delta = i64::from(*output) - self.duration_of(&node);
                Command::WrapRetime {
                    node,
                    id: self.fresh("w"),
                    duration: frames(i64::from(*output)),
                    pitch: PitchPolicy::Preserve,
                }
                .with(delta)
            }
            Op::SetRetime { pick: p, output } => {
                let node = pick(
                    &self.kind_children(|kind| {
                        matches!(kind, NodeKind::Retime { purpose, .. } if purpose.is_edit())
                    }),
                    *p,
                )?;
                let delta = i64::from(*output) - self.duration_of(&node);
                Command::SetRetime {
                    node,
                    duration: frames(i64::from(*output)),
                    pitch: PitchPolicy::FollowSpeed,
                }
                .with(delta)
            }
            Op::SetHoldDuration {
                pick: p,
                frames: length,
            } => {
                let node = pick(
                    &self.kind_children(|kind| matches!(kind, NodeKind::Hold { .. })),
                    *p,
                )?;
                let delta = i64::from(*length) - self.duration_of(&node);
                Command::SetHoldDuration {
                    node,
                    duration: frames(i64::from(*length)),
                }
                .with(delta)
            }
            Op::ScopedRename { pick: p, play } => {
                let repeat = pick(&self.repeats(), *p)?;
                let child = self.repeat_child(&repeat);
                let iteration = self.iteration(&repeat, u32::from(*play) % self.plays(&repeat))?;
                let target = ScopedNodeTarget {
                    node: child,
                    repeats: vec![RepeatEditStep {
                        repeat,
                        branch: RepeatEditBranch::Play { iteration },
                    }],
                };
                let edit = ScopedNodeEdit::Rename {
                    label: format!("Renamed {revision}"),
                };
                let required = self
                    .document
                    .scoped_edit_requirements(&target, &edit)
                    .ok()?;
                let identities = OccurrenceIdentities {
                    nodes: self.fresh_many("o", required.nodes),
                    marks: self.fresh_marks(required.marks),
                };
                Command::EditScoped {
                    target,
                    edit,
                    identities,
                }
                .with(0)
            }
            Op::Explode { pick: p } => {
                let node = pick(&self.repeats(), *p)?;
                let needs = self.document.explode_requirements(&node).ok()?;
                Command::Explode {
                    node,
                    identities: OccurrenceIdentities {
                        nodes: self.fresh_many("x", needs.nodes),
                        marks: self.fresh_marks(needs.marks),
                    },
                    timing,
                }
                .with(0)
            }
            Op::DuplicateChild { pick: p } => {
                let node = pick(&self.children(), *p)?;
                let delta = self.duration_of(&node);
                self.duplicate(SliceCaptureSelection::Child { node }, timing)?
                    .with(delta)
            }
            Op::DuplicateRange { start, len } => {
                let range = self.range(*start, *len)?;
                self.duplicate(SliceCaptureSelection::Range { range }, timing)?
                    .with(range.duration().frames())
            }
            Op::ScopedRenameMany {
                pick: p,
                first,
                count,
            } => {
                let repeat = pick(&self.repeats(), *p)?;
                let child = self.repeat_child(&repeat);
                let plays = self.plays(&repeat);
                let edits = (0..u32::from(*count).min(plays))
                    .map(|offset| {
                        let index = (u32::from(*first) + offset) % plays;
                        Some(ScopedTargetEdit {
                            target: ScopedNodeTarget {
                                node: child.clone(),
                                repeats: vec![RepeatEditStep {
                                    repeat: repeat.clone(),
                                    branch: RepeatEditBranch::Play {
                                        iteration: self.iteration(&repeat, index)?,
                                    },
                                }],
                            },
                            edit: ScopedNodeEdit::Rename {
                                label: format!("Several {revision}"),
                            },
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                let needs = self.document.scoped_many_requirements(&edits).ok()?;
                let identities = needs
                    .iter()
                    .map(|needs| OccurrenceIdentities {
                        nodes: self.fresh_many("m", needs.nodes),
                        marks: self.fresh_marks(needs.marks),
                    })
                    .collect();
                Command::EditScopedMany { edits, identities }.with(0)
            }
            Op::OccurrenceRename { pick: p, play } => {
                let repeat = pick(&self.repeats(), *p)?;
                let child = self.repeat_child(&repeat);
                let iteration = self.iteration(&repeat, u32::from(*play) % self.plays(&repeat))?;
                let nodes = self.document.nodes().len();
                let marks = self.document.marks().len();
                let identities = OccurrenceIdentities {
                    nodes: self.fresh_many("o", nodes),
                    marks: self.fresh_marks(marks),
                };
                Command::EditOccurrence {
                    instance: InstancePath {
                        node: child,
                        repeats: vec![RepeatInstance {
                            node: repeat,
                            iteration,
                        }],
                    },
                    edit: OccurrenceEdit::Rename {
                        label: format!("Occurrence {revision}"),
                    },
                    identities,
                }
                .with(0)
            }
            Op::SetMark {
                pick: p,
                at,
                right,
                keep,
            } => {
                let node = pick(&self.children(), *p)?;
                let position = i64::from(*at) % (self.duration_of(&node) + 1);
                let id = self.fresh_marks(1).remove(0);
                Command::SetMark {
                    id,
                    owner: node.clone(),
                    label: "Mark".into(),
                    boundary: BoundaryAnchor {
                        coordinate: Anchor::Local {
                            node,
                            position: ExactRatio::integer(position),
                        },
                        bias: if *right {
                            InsertionBias::Right
                        } else {
                            InsertionBias::Left
                        },
                    },
                    loss_policy: if *keep {
                        AnchorLossPolicy::KeepUnresolved
                    } else {
                        AnchorLossPolicy::DeleteOwned
                    },
                }
                .with(0)
            }
        };
        Some(command)
    }

    fn repeat(
        &mut self,
        selection: SliceCaptureSelection,
        plays: u32,
        timing: AudioTimingId,
    ) -> Option<Resolved> {
        let root = self.root();
        let plan = self
            .document
            .repeat_selection(&root, &selection, plays)
            .ok()?;
        let delta = plan.output_duration.frames() - plan.range.duration().frames();
        let identities = RepeatSelectionIdentities {
            repeat: self.fresh("r"),
            group: plan.needs_group.then(|| self.fresh("rg")),
            split: self.split_pool(plan.required_split_ids),
        };
        Some(
            Command::RepeatSelection {
                parent: root,
                selection,
                plays,
                identities,
                timing,
            }
            .with(delta),
        )
    }
    fn repeats(&self) -> Vec<NodeId> {
        self.kind_children(|kind| matches!(kind, NodeKind::Repeat { .. }))
    }
    fn repeat_child(&self, repeat: &NodeId) -> NodeId {
        match &self.document.nodes()[repeat].kind {
            NodeKind::Repeat { child, .. } => child.clone(),
            _ => unreachable!("selected a Repeat"),
        }
    }
    fn plays(&self, repeat: &NodeId) -> u32 {
        match &self.document.nodes()[repeat].kind {
            NodeKind::Repeat { iterations, .. } => iterations.len(),
            _ => unreachable!("selected a Repeat"),
        }
    }
    fn iteration(&self, repeat: &NodeId, index: u32) -> Option<IterationId> {
        match &self.document.nodes()[repeat].kind {
            NodeKind::Repeat { iterations, .. } => iterations.at(index),
            _ => None,
        }
    }
    /// The promised change for a Repeat with only default plays and gaps:
    /// `plays × child + (plays − 1) × gap`. Overrides make it data-dependent.
    fn uniform_repeat(&self, repeat: &NodeId, plays: u32, gap: Option<i64>) -> Option<i64> {
        if self.document.overrides().contains_key(repeat)
            || self.document.gap_overrides().contains_key(repeat)
        {
            return None;
        }
        let NodeKind::Repeat {
            child, gap: old, ..
        } = &self.document.nodes()[repeat].kind
        else {
            return None;
        };
        let gap = gap.unwrap_or_else(|| old.as_ref().map_or(0, |gap| gap.duration.frames()));
        let plays = i64::from(plays);
        let after = plays * self.duration_of(child) + (plays - 1) * gap;
        Some(after - self.duration_of(repeat))
    }
}

trait With {
    fn with(self, delta: i64) -> Resolved;
}
impl With for Command {
    fn with(self, delta: i64) -> Resolved {
        Resolved {
            command: self,
            delta: Some(delta),
        }
    }
}

/// Resolved clocks of unrepeated physical owners, without work counters or
/// birth indexes (which describe evaluation, not the resulting clock).
fn clocks(document: &ProjectDocument) -> BTreeMap<NodeId, Value> {
    let bindings = document.audio_bindings();
    bindings
        .bindings()
        .keys()
        .filter_map(|owner| {
            let instance = InstancePath {
                node: owner.clone(),
                repeats: Vec::new(),
            };
            instance.validate(document).ok()?;
            let resolved = bindings.resolve(owner, &instance, 100_000).ok()?;
            let mut lattice = serde_json::to_value(&resolved.lattice).unwrap();
            let object = lattice.as_object_mut().unwrap();
            object.remove("work");
            object.remove("birth");
            Some((
                owner.clone(),
                json!({"lattice": lattice, "resume": resolved.resume}),
            ))
        })
        .collect()
}

fn bound(mark: &Mark) -> bool {
    mark.bindings()
        .any(|binding| binding.state == MarkState::Bound)
}

/// Check one committed step against every transaction invariant.
fn check(
    op: &Op,
    before: &ProjectDocument,
    after: &ProjectDocument,
    edit: &EditTransaction,
    promised: Option<i64>,
) -> Result<(), TestCaseError> {
    after
        .validate()
        .map_err(|error| TestCaseError::fail(format!("{op:?}: invalid result: {error}")))?;
    let reread = ProjectDocument::from_json(&after.to_json().unwrap()).unwrap();
    prop_assert_eq!(
        &reread,
        after,
        "{:?}: JSON round trip changed the result",
        op
    );
    let forward = edit.forward.apply(before).unwrap();
    prop_assert_eq!(
        &forward,
        after,
        "{:?}: forward patch differs from result",
        op
    );
    let inverse = edit.inverse.apply(after).unwrap();
    // The inverse restores authored state; the revision identity is the input's.
    prop_assert_eq!(
        &inverse,
        before,
        "{:?}: inverse patch does not restore input",
        op
    );
    let actual = after.duration().unwrap().frames() - before.duration().unwrap().frames();
    prop_assert_eq!(edit.duration_delta, actual, "{:?}: duration_delta", op);
    if let Some(promised) = promised {
        prop_assert_eq!(actual, promised, "{:?}: promised duration change", op);
    }
    for (id, mark) in before.marks() {
        match after.marks().get(id) {
            Some(result) => {
                prop_assert_eq!(&result.label, &mark.label);
                prop_assert_eq!(result.loss_policy, mark.loss_policy);
                prop_assert_eq!(result.boundary.bias, mark.boundary.bias);
                if !bound(mark) {
                    prop_assert!(
                        !bound(result),
                        "{:?}: unresolved mark {} became bound",
                        op,
                        id
                    );
                }
                if lossless(op) && bound(mark) && !touches_removed(mark, before, after) {
                    prop_assert!(bound(result), "{:?}: lossless edit unbound mark {}", op, id);
                }
            }
            None => {
                prop_assert!(
                    mark.loss_policy == AnchorLossPolicy::DeleteOwned
                        && (!lossless(op) || touches_removed(mark, before, after)),
                    "{:?}: mark {} with {:?} disappeared",
                    op,
                    id,
                    mark.loss_policy
                );
            }
        }
    }
    if time_neutral(op) {
        let old = clocks(before);
        let new = clocks(after);
        for (owner, clock) in &old {
            if let Some(result) = new.get(owner) {
                prop_assert_eq!(
                    result,
                    clock,
                    "{:?}: retained clock of {} changed",
                    op,
                    owner
                );
            }
        }
    }
    Ok(())
}

fn run(beats: Vec<Beat>, ops: Vec<Op>) -> Result<(), TestCaseError> {
    let initial = document(&beats);
    let mut runner = Runner {
        document: initial.clone(),
        step: 0,
        ids: 0,
        history: Vec::new(),
    };
    for op in &ops {
        runner.step += 1;
        let revision = rev(&format!("step-{}", runner.step));
        let Some(resolved) = runner.resolve(op, &revision) else {
            continue;
        };
        let before = runner.document.clone();
        let request = CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision,
            command: resolved.command,
        };
        match apply_with_result(&runner.document, &request) {
            Ok((edit, after)) => {
                check(op, &before, &after, &edit, resolved.delta)?;
                runner.history.push(edit);
                runner.document = after;
            }
            Err(_) => {
                prop_assert_eq!(&runner.document, &before, "refusal changed its input");
            }
        }
    }
    // Undo everything: inverse patches in reverse order restore the start.
    let mut document = runner.document.clone();
    for edit in runner.history.iter().rev() {
        document = edit.inverse.apply(&document).unwrap();
    }
    prop_assert_eq!(&document, &initial, "full undo does not restore the start");
    // Redo everything again from the start.
    for edit in &runner.history {
        document = edit.forward.apply(&document).unwrap();
    }
    prop_assert_eq!(
        &document,
        &runner.document,
        "full redo does not reach the end"
    );
    Ok(())
}

/// A fixed seed: every run explores the same cases, so a coverage or
/// property result is reproducible rather than depending on the run.
fn config(cases: u32) -> Config {
    Config {
        cases,
        failure_persistence: None,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x6465_6164_7061_6e31),
        ..Config::default()
    }
}

proptest! {
    #![proptest_config(config(if cfg!(debug_assertions) { 64 } else { 512 }))]

    #[test]
    fn random_structural_sequences_keep_transaction_invariants(
        beats in prop::collection::vec(beat(), 2..=6),
        ops in prop::collection::vec(operation(), 1..=20),
    ) {
        run(beats, ops)?;
    }
}

/// The generator must actually reach the commands it claims to cover; a
/// resolver change that silently skips a family would otherwise pass.
#[test]
fn generated_sequences_commit_every_command_family() {
    let mut runner = proptest::test_runner::TestRunner::new(config(1));
    let mut committed = BTreeSet::new();
    // Apply `ops` to a fresh document of `beats`, recording each committed
    // family.
    let run_ops = |beats: &[Beat], ops: Vec<Op>, committed: &mut BTreeSet<String>| {
        let mut state = Runner {
            document: document(beats),
            step: 0,
            ids: 0,
            history: Vec::new(),
        };
        for op in ops {
            state.step += 1;
            let revision = rev(&format!("step-{}", state.step));
            let Some(resolved) = state.resolve(&op, &revision) else {
                continue;
            };
            let request = CommandRequest {
                project_id: state.document.project_id().clone(),
                expected_revision: state.document.revision_id().clone(),
                new_revision: revision,
                command: resolved.command,
            };
            if let Ok((_, after)) = apply_with_result(&state.document, &request) {
                committed.insert(
                    format!("{op:?}")
                        .split([' ', '{'])
                        .next()
                        .unwrap()
                        .to_owned(),
                );
                state.document = after;
            }
        }
    };
    // A directed prelude for the families that need a precondition the
    // generator reaches rarely: a Repeat with a positive gap to isolate.
    run_ops(
        &[Beat::Hold(4), Beat::Source(8), Beat::Hold(6)],
        vec![
            Op::RepeatChild { pick: 0, plays: 3 },
            Op::SetRepeatPlays { pick: 0, plays: 4 },
            Op::SetRepeatGaps { pick: 0, frames: 2 },
            Op::IsolateGap { pick: 0, play: 1 },
        ],
        &mut committed,
    );
    for family in [
        "RepeatChild",
        "SetRepeatPlays",
        "SetRepeatGaps",
        "IsolateGap",
    ] {
        assert!(
            committed.contains(family),
            "the prelude did not commit {family}"
        );
    }
    let ops_strategy = prop::collection::vec(operation(), 12);
    let beats_strategy = prop::collection::vec(beat(), 3..=6);
    for _ in 0..400 {
        let beats = beats_strategy.new_tree(&mut runner).unwrap().current();
        let ops = ops_strategy.new_tree(&mut runner).unwrap().current();
        run_ops(&beats, ops, &mut committed);
    }
    let expected = [
        "Split",
        "DeleteRipple",
        "DeleteChildren",
        "DeleteRange",
        "InsertTime",
        "Paste",
        "PasteChild",
        "Replace",
        "Move",
        "GroupRange",
        "GroupChild",
        "Ungroup",
        "RepeatChild",
        "RepeatRange",
        "SetRepeatPlays",
        "SetRepeatGaps",
        "IsolateGap",
        "WrapRetime",
        "SetRetime",
        "SetHoldDuration",
        "ScopedRename",
        "OccurrenceRename",
        "SetMark",
        "Explode",
        "DuplicateChild",
        "DuplicateRange",
        "ScopedRenameMany",
    ];
    let missing: Vec<_> = expected
        .iter()
        .filter(|name| !committed.contains(**name))
        .collect();
    assert!(missing.is_empty(), "never committed: {missing:?}");
}
