//! Authored inspector navigation with separately qualified picture occurrences.

use std::{collections::BTreeMap, ops::Range, sync::Arc};

use deadpan_core::{
    ExactRatio, FrameDuration, InsertionBias, InstancePath, MAX_DOCUMENT_DEPTH, NodeId, NodeKind,
    ProjectDocument, ProjectFrame, RepeatEditBranch, RepeatEditStep, RepeatInstance, RepeatLayout,
    ScopedNodeTarget,
};
use deadpan_plan::RenderPlan;

use crate::project::{
    SequenceScope, Workspace,
    scoped::{Commit, Target},
};

const MAX_PROJECTION_WORK: usize = 4096;
const DORMANT_DEFAULT: &str = "All plays override this default; it has no active picture.";
const DORMANT_GAP: &str = "This owned gap is inactive after the final play or has zero duration.";
const CROPPED: &str = "This node is empty or outside the ancestor Retime crop.";
const UNSAMPLED: &str = "This representative has no sampled picture; choose another play.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::preview) struct Presentation {
    pub instance: InstancePath,
    pub exact: Range<ExactRatio>,
    /// Root frames whose centers lie in `exact`, not rounded intrinsic time.
    pub frames: Range<u64>,
}

#[derive(Clone, Debug)]
pub(in crate::preview) struct Row {
    pub target: ScopedNodeTarget,
    pub label: String,
    pub kind: &'static str,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::preview) struct RepeatChoice {
    pub owner_label: String,
    pub label: String,
    pub one_based: Option<u32>,
    pub plays: u32,
}

#[derive(Clone)]
struct Level {
    owner: ScopedNodeTarget,
    branch: RepeatEditBranch,
    selected: usize,
}

#[derive(Clone)]
pub(in crate::preview) struct State {
    session: u64,
    scope: SequenceScope,
    root: NodeId,
    index: Arc<Index>,
    levels: Vec<Level>,
    selected: ScopedNodeTarget,
    cursor: ProjectFrame,
    preferred: Option<InstancePath>,
    rows: Arc<Vec<Row>>,
    projection: Projection,
    breadcrumbs: Arc<Vec<String>>,
    scope_label: String,
    repeat_choice: Option<RepeatChoice>,
}

#[derive(Clone)]
struct Index {
    document: Arc<ProjectDocument>,
    plan: Arc<RenderPlan>,
    parents: BTreeMap<NodeId, NodeId>,
    sequence_offsets: BTreeMap<NodeId, i64>,
    durations: BTreeMap<NodeId, FrameDuration>,
    repeats: BTreeMap<NodeId, RepeatLayout>,
    overridden: BTreeMap<NodeId, Vec<u32>>,
    overridden_runs: BTreeMap<NodeId, Vec<Range<u32>>>,
}

impl State {
    pub fn new(
        workspace: &Workspace,
        scope: SequenceScope,
        root: NodeId,
        cursor: ProjectFrame,
    ) -> Result<Self, String> {
        Self::new_in(
            workspace.session,
            scope,
            root,
            cursor,
            Index::new(workspace.document.clone(), workspace.plan.clone())?,
        )
    }

    fn new_in(
        session: u64,
        scope: SequenceScope,
        root: NodeId,
        cursor: ProjectFrame,
        index: Index,
    ) -> Result<Self, String> {
        let view = scope.resolve_document(&index.document, &index.plan)?;
        if !view.children.contains(&root)
            || !matches!(
                index.document.nodes()[&root].kind,
                NodeKind::Repeat { .. } | NodeKind::Retime { .. }
            )
        {
            return Err("Inspect a direct Repeat or Retime child of the current Sequence.".into());
        }
        if cursor.0 < 0 || cursor.0 > index.plan.duration().frames() {
            return Err("Inspector cursor is outside the project.".into());
        }
        let owner = ScopedNodeTarget {
            node: root.clone(),
            repeats: Vec::new(),
        };
        let mut state = Self {
            session,
            scope,
            root,
            index: Arc::new(index),
            selected: owner.clone(),
            cursor,
            preferred: None,
            rows: Arc::default(),
            projection: Projection::default(),
            breadcrumbs: Arc::default(),
            scope_label: String::new(),
            repeat_choice: None,
            levels: vec![Level {
                owner,
                branch: RepeatEditBranch::Default,
                selected: 0,
            }],
        };
        state.refresh_selected()?;
        Ok(state)
    }

    /// A typed mark-only receipt proves every authored navigation edge and
    /// projected interval unchanged. Rebase the current inspector, including
    /// navigation after mark entry, without rebuilding its captured selection.
    pub fn rebase_mark(
        &mut self,
        workspace: &Workspace,
        saved: &crate::project::marks::Saved,
    ) -> bool {
        self.rebase_mark_in(
            workspace.session,
            &workspace.document,
            &workspace.plan,
            saved,
        )
    }

    fn rebase_mark_in(
        &mut self,
        session: u64,
        document: &Arc<ProjectDocument>,
        plan: &Arc<RenderPlan>,
        saved: &crate::project::marks::Saved,
    ) -> bool {
        if session != saved.id.session
            || document.project_id() != &saved.id.project
            || document.revision_id() != &saved.revision
            || self.session != saved.id.session
            || self.index.document.project_id() != &saved.id.project
        {
            return false;
        }
        if self.index.document.revision_id() == &saved.revision {
            return true;
        }
        if self.index.document.revision_id() != &saved.id.revision {
            return false;
        }
        let index = Arc::make_mut(&mut self.index);
        index.document = document.clone();
        index.plan = plan.clone();
        true
    }

    pub fn matches_workspace(&self, workspace: &Workspace) -> bool {
        self.matches_identity(workspace.session, &workspace.document)
    }

    fn matches_identity(&self, session: u64, document: &ProjectDocument) -> bool {
        self.session == session
            && self.index.document.project_id() == document.project_id()
            && self.index.document.revision_id() == document.revision_id()
    }

    fn check(&self, workspace: &Workspace) -> Result<(), String> {
        if self.matches_workspace(workspace) {
            Ok(())
        } else {
            Err("Project revision changed; reopen the scoped inspector.".into())
        }
    }

    pub fn selected_target(&self) -> &ScopedNodeTarget {
        &self.selected
    }
    pub fn selected(&self) -> usize {
        self.levels.last().expect("inspector has a level").selected
    }

    pub fn target(&self, workspace: &Workspace, cursor: ProjectFrame) -> Result<Target, String> {
        self.check(workspace)?;
        if cursor.0 < 0 || cursor.0 > self.index.plan.duration().frames() {
            return Err("Captured scoped cursor is outside the project.".into());
        }
        let target = Target {
            session: self.session,
            project: self.index.document.project_id().clone(),
            revision: self.index.document.revision_id().clone(),
            scope: self.scope.clone(),
            root: self.root.clone(),
            target: self.selected.clone(),
            presentation: self
                .projection
                .presentation
                .as_ref()
                .map(|p| p.instance.clone()),
            cursor,
        };
        Ok(target)
    }

    pub fn rows(&self, workspace: &Workspace) -> Result<Arc<Vec<Row>>, String> {
        self.check(workspace)?;
        Ok(self.rows.clone())
    }

    pub fn presentation(&self, workspace: &Workspace) -> Result<Option<Presentation>, String> {
        self.check(workspace)?;
        Ok(self.projection.presentation.clone())
    }
    pub fn visible_range(
        &self,
        workspace: &Workspace,
    ) -> Result<Option<Range<ExactRatio>>, String> {
        self.check(workspace)?;
        Ok(self.projection.exact.clone())
    }
    pub fn no_picture_reason(&self, workspace: &Workspace) -> Result<Option<String>, String> {
        self.check(workspace)?;
        Ok(self.projection.reason.clone())
    }
    pub fn breadcrumb(&self, workspace: &Workspace) -> Result<Arc<Vec<String>>, String> {
        self.check(workspace)?;
        Ok(self.breadcrumbs.clone())
    }
    pub fn scope_label(&self, workspace: &Workspace) -> Result<String, String> {
        self.check(workspace)?;
        Ok(self.scope_label.clone())
    }
    pub fn repeat_choice(&self, workspace: &Workspace) -> Result<Option<RepeatChoice>, String> {
        self.check(workspace)?;
        Ok(self.repeat_choice.clone())
    }

    pub fn select(&mut self, workspace: &Workspace, index: usize) -> Result<(), String> {
        self.check(workspace)?;
        self.select_in(index)
    }
    fn select_in(&mut self, index: usize) -> Result<(), String> {
        let selected = self
            .rows
            .get(index)
            .ok_or("Inspector row is outside this level")?
            .target
            .clone();
        let projection = self.project(&selected)?;
        let previous = self.selected();
        let rows = Arc::make_mut(&mut self.rows);
        rows[previous].selected = false;
        rows[index].selected = true;
        self.levels.last_mut().expect("level").selected = index;
        self.selected = selected;
        self.projection = projection;
        Ok(())
    }
    pub fn step(&mut self, workspace: &Workspace, forward: bool, count: u32) -> Result<(), String> {
        self.check(workspace)?;
        self.step_in(forward, count)
    }
    fn step_in(&mut self, forward: bool, count: u32) -> Result<(), String> {
        let count = usize::try_from(count).map_err(|error| error.to_string())?;
        let end = self.rows.len().saturating_sub(1);
        let selected = if forward {
            self.selected().saturating_add(count).min(end)
        } else {
            self.selected().saturating_sub(count)
        };
        self.select_in(selected)
    }
    pub fn enter(&mut self, workspace: &Workspace) -> Result<bool, String> {
        self.check(workspace)?;
        self.enter_in()
    }
    fn enter_in(&mut self) -> Result<bool, String> {
        if !matches!(self.index.document.nodes()[&self.selected.node].kind, NodeKind::Sequence { ref children } if !children.is_empty())
            && !matches!(
                self.index.document.nodes()[&self.selected.node].kind,
                NodeKind::Repeat { .. } | NodeKind::Retime { .. }
            )
        {
            return Ok(false);
        }
        if self.levels.len() >= MAX_DOCUMENT_DEPTH {
            return Err("Inspector depth limit reached.".into());
        }
        let mut next = self.clone();
        next.levels.push(Level {
            owner: self.selected.clone(),
            branch: RepeatEditBranch::Default,
            selected: 0,
        });
        next.refresh_selected()?;
        *self = next;
        Ok(true)
    }
    pub fn leave(&mut self, workspace: &Workspace) -> Result<bool, String> {
        self.check(workspace)?;
        self.leave_in()
    }
    fn leave_in(&mut self) -> Result<bool, String> {
        if self.levels.len() == 1 {
            return Ok(false);
        }
        let mut next = self.clone();
        next.levels.pop();
        next.refresh_selected()?;
        *self = next;
        Ok(true)
    }
    pub fn switch_all(&mut self, workspace: &Workspace) -> Result<(), String> {
        self.check(workspace)?;
        self.switch_in(RepeatEditBranch::Default)
    }
    pub fn switch_play(&mut self, workspace: &Workspace, one_based: u32) -> Result<(), String> {
        self.check(workspace)?;
        self.switch_play_in(one_based)
    }
    fn switch_play_in(&mut self, one_based: u32) -> Result<(), String> {
        let level = self
            .repeat_level()
            .ok_or("This level has no Repeat scope")?;
        let NodeKind::Repeat { iterations, .. } =
            &self.index.document.nodes()[&self.levels[level].owner.node].kind
        else {
            unreachable!()
        };
        let iteration = one_based
            .checked_sub(1)
            .and_then(|index| iterations.at(index))
            .ok_or("Play number is outside this Repeat")?;
        self.switch_in(RepeatEditBranch::Play { iteration })
    }
    fn switch_in(&mut self, branch: RepeatEditBranch) -> Result<(), String> {
        let level = self
            .repeat_level()
            .ok_or("This level has no Repeat scope")?;
        let mut next = self.clone();
        next.levels.truncate(level + 1);
        next.levels[level].branch = branch;
        next.levels[level].selected = 0;
        next.preferred = None;
        next.refresh_selected()?;
        *self = next;
        Ok(())
    }
    fn repeat_level(&self) -> Option<usize> {
        self.levels.iter().rposition(|level| {
            matches!(
                self.index.document.nodes()[&level.owner.node].kind,
                NodeKind::Repeat { .. }
            )
        })
    }
    fn row_targets(&self) -> Result<Vec<ScopedNodeTarget>, String> {
        let level = self.levels.last().ok_or("Inspector has no level")?;
        let mut repeats = level.owner.repeats.clone();
        let children = match &self.index.document.nodes()[&level.owner.node].kind {
            NodeKind::Sequence { children } => children.clone(),
            NodeKind::Retime { child, .. } => vec![child.clone()],
            NodeKind::Repeat {
                child, iterations, ..
            } => {
                repeats.push(RepeatEditStep {
                    repeat: level.owner.node.clone(),
                    branch: level.branch.clone(),
                });
                match &level.branch {
                    RepeatEditBranch::Default => vec![child.clone()],
                    RepeatEditBranch::Play { iteration } => {
                        if iterations.position(iteration).is_none() {
                            return Err("Selected play retired".into());
                        }
                        let mut children = vec![
                            self.index
                                .document
                                .overrides()
                                .get(&level.owner.node)
                                .and_then(|entries| entries.get(iteration))
                                .unwrap_or(child)
                                .clone(),
                        ];
                        if let Some(gap) = self
                            .index
                            .document
                            .gap_overrides()
                            .get(&level.owner.node)
                            .and_then(|entries| entries.get(iteration))
                        {
                            children.push(gap.clone());
                        }
                        children
                    }
                }
            }
            _ => Vec::new(),
        };
        Ok(children
            .into_iter()
            .map(|node| ScopedNodeTarget {
                node,
                repeats: repeats.clone(),
            })
            .collect())
    }
    fn refresh_selected(&mut self) -> Result<(), String> {
        let targets = self.row_targets()?;
        self.selected = targets
            .get(self.selected())
            .ok_or("Inspector level has no selected row")?
            .clone();
        self.selected
            .validate(&self.index.document)
            .map_err(|error| error.to_string())?;
        self.rows = Arc::new(
            targets
                .into_iter()
                .enumerate()
                .map(|(index, target)| {
                    let node = &self.index.document.nodes()[&target.node];
                    Row {
                        target,
                        label: node.label.clone(),
                        kind: kind(&node.kind),
                        selected: index == self.selected(),
                    }
                })
                .collect(),
        );
        self.projection = self.project(&self.selected)?;
        self.refresh_labels()?;
        Ok(())
    }

    fn refresh_labels(&mut self) -> Result<(), String> {
        let mut breadcrumbs = Vec::with_capacity(self.levels.len());
        let mut scopes = Vec::new();
        let mut nearest = None;
        for level in &self.levels {
            let node = &self.index.document.nodes()[&level.owner.node];
            let NodeKind::Repeat { iterations, .. } = &node.kind else {
                breadcrumbs.push(node.label.clone());
                continue;
            };
            let one_based = match &level.branch {
                RepeatEditBranch::Default => None,
                RepeatEditBranch::Play { iteration } => Some(
                    iterations
                        .position(iteration)
                        .ok_or("Selected play retired")?
                        .checked_add(1)
                        .ok_or("Repeat position overflow")?,
                ),
            };
            let plays = iterations.len();
            let choice = RepeatChoice {
                owner_label: node.label.clone(),
                label: one_based.map_or_else(
                    || "all plays".into(),
                    |index| format!("play {index}/{plays}"),
                ),
                one_based,
                plays,
            };
            breadcrumbs.push(format!("{} [{}]", choice.owner_label, choice.label));
            scopes.push(format!("{} · {}", choice.owner_label, choice.label));
            nearest = Some(choice);
        }
        self.breadcrumbs = Arc::new(breadcrumbs);
        self.scope_label = if scopes.is_empty() {
            "Retime child".into()
        } else {
            scopes.join(" › ")
        };
        self.repeat_choice = nearest;
        Ok(())
    }

    /// The host separately checks its pending token and current cursor. An
    /// unrelated revision never rebuilds this state by positional coincidence.
    pub fn reconcile(&mut self, workspace: &Workspace, commit: &Commit) -> Result<bool, String> {
        if workspace.session != self.session
            || workspace.document.project_id() != &commit.before.project
            || workspace.document.revision_id() != &commit.revision
        {
            return Ok(false);
        }
        self.reconcile_in(
            Index::new(workspace.document.clone(), workspace.plan.clone())?,
            commit,
        )
    }

    fn reconcile_in(&mut self, index: Index, commit: &Commit) -> Result<bool, String> {
        let before = &commit.before;
        if before.session != self.session
            || &before.project != self.index.document.project_id()
            || &before.revision != self.index.document.revision_id()
            || before.scope != self.scope
            || before.root != self.root
            || before.target != self.selected
            || before.presentation
                != self
                    .projection
                    .presentation
                    .as_ref()
                    .map(|value| value.instance.clone())
            || index.document.project_id() != &before.project
            || index.document.revision_id() != &commit.revision
        {
            return Ok(false);
        }
        let mut next = self.clone();
        next.index = Arc::new(index);
        if !next
            .scope
            .resolve_document(&next.index.document, &next.index.plan)?
            .children
            .contains(&next.root)
        {
            return Ok(false);
        }
        commit
            .target
            .validate(&next.index.document)
            .map_err(|error| error.to_string())?;
        let path = next.index.path(&commit.target.node)?;
        let Some(start) = path.iter().position(|node| node == &next.root) else {
            return Ok(false);
        };
        next.levels.clear();
        for (depth, pair) in path[start..].windows(2).enumerate() {
            let repeats = commit
                .target
                .repeats
                .iter()
                .filter(|step| path[..start + depth].contains(&step.repeat))
                .cloned()
                .collect();
            let branch = commit
                .target
                .repeats
                .iter()
                .find(|step| step.repeat == pair[0])
                .map_or(RepeatEditBranch::Default, |step| step.branch.clone());
            next.levels.push(Level {
                owner: ScopedNodeTarget {
                    node: pair[0].clone(),
                    repeats,
                },
                branch,
                selected: 0,
            });
            let row = next
                .row_targets()?
                .iter()
                .position(|target| target.node == pair[1])
                .ok_or("Mapped scoped target left its authored parent")?;
            next.levels.last_mut().expect("level").selected = row;
        }
        if next.levels.is_empty() {
            return Ok(false);
        }
        next.preferred = commit.presentation.clone();
        next.cursor = before.cursor;
        next.refresh_selected()?;
        if next.selected != commit.target {
            return Ok(false);
        }
        if let Some(instance) = &next.preferred
            && !next
                .selected
                .matches_instance(&next.index.document, instance)
                .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        if next
            .projection
            .presentation
            .as_ref()
            .map(|value| &value.instance)
            != commit.presentation.as_ref()
        {
            return Ok(false);
        }
        *self = next;
        Ok(true)
    }
}

fn kind(node: &NodeKind) -> &'static str {
    match node {
        NodeKind::Source { .. } => "Source",
        NodeKind::Hold { .. } => "Hold",
        NodeKind::Sequence { .. } => "Sequence",
        NodeKind::Repeat { .. } => "Repeat",
        NodeKind::Retime { .. } => "Retime",
    }
}

impl Index {
    fn new(document: Arc<ProjectDocument>, plan: Arc<RenderPlan>) -> Result<Self, String> {
        let durations = document.durations().map_err(|error| error.to_string())?;
        let parents = document
            .nodes()
            .keys()
            .flat_map(|owner| {
                document
                    .children(owner)
                    .map(move |child| (child.clone(), owner.clone()))
            })
            .collect();
        let mut sequence_offsets = BTreeMap::new();
        let mut repeats = BTreeMap::new();
        let mut overridden = BTreeMap::new();
        let mut overridden_runs = BTreeMap::new();
        for (id, node) in document.nodes() {
            if let NodeKind::Sequence { children } = &node.kind {
                let mut offset = 0_i64;
                for child in children {
                    sequence_offsets.insert(child.clone(), offset);
                    offset = offset
                        .checked_add(durations[child].frames())
                        .ok_or("Sequence offset overflow")?;
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
                    RepeatLayout::compile_with_gap_overrides(
                        iterations,
                        child,
                        document.overrides().get(id),
                        gap.as_ref().map_or(FrameDuration::ZERO, |gap| gap.duration),
                        document.gap_overrides().get(id),
                        &durations,
                    )
                    .map_err(|error| error.to_string())?,
                );
                // Resolve all sparse identities against compact runs once.
                // Calling IterationOrder::position for every override would
                // multiply fragmented-run count by sparse-override count.
                let identities = document
                    .overrides()
                    .get(id)
                    .into_iter()
                    .flat_map(|entries| entries.iter())
                    .map(|(iteration, _)| {
                        ((&iteration.allocation, u64::from(iteration.ordinal)), ())
                    })
                    .collect::<BTreeMap<_, _>>();
                let mut positions = Vec::with_capacity(identities.len());
                let mut offset = 0_u32;
                for (allocation, first, count) in iterations.segments() {
                    let start = u64::from(first);
                    let end = start + u64::from(count);
                    for ((_, ordinal), ()) in
                        identities.range((allocation, start)..(allocation, end))
                    {
                        let in_run =
                            u32::try_from(*ordinal - start).map_err(|error| error.to_string())?;
                        positions.push(offset.checked_add(in_run).ok_or("Repeat index overflow")?);
                    }
                    offset = offset.checked_add(count).ok_or("Repeat index overflow")?;
                }
                positions.sort_unstable();
                let mut runs: Vec<Range<u32>> = Vec::new();
                for position in &positions {
                    let end = position.checked_add(1).ok_or("Repeat index overflow")?;
                    if let Some(last) = runs.last_mut()
                        && last.end == *position
                    {
                        last.end = end;
                    } else {
                        runs.push(*position..end);
                    }
                }
                overridden_runs.insert(id.clone(), runs);
                overridden.insert(id.clone(), positions);
            }
        }
        Ok(Self {
            document,
            plan,
            parents,
            sequence_offsets,
            durations,
            repeats,
            overridden,
            overridden_runs,
        })
    }
    fn path(&self, node: &NodeId) -> Result<Vec<NodeId>, String> {
        let mut path = vec![node.clone()];
        while let Some(parent) = self.parents.get(path.last().expect("path")) {
            if path.len() > MAX_DOCUMENT_DEPTH {
                return Err("Inspector path exceeds depth limit".into());
            }
            path.push(parent.clone());
        }
        if path.last() != Some(self.document.root()) {
            return Err("Scoped target is outside the project".into());
        }
        path.reverse();
        Ok(path)
    }
}

#[derive(Clone, Default)]
struct Projection {
    presentation: Option<Presentation>,
    exact: Option<Range<ExactRatio>>,
    reason: Option<String>,
}
impl Projection {
    fn unavailable(reason: &str) -> Self {
        Self {
            reason: Some(reason.into()),
            ..Self::default()
        }
    }
}

#[derive(Clone)]
struct Affine {
    offset: ExactRatio,
    scale: ExactRatio,
    clip: Range<ExactRatio>,
}
impl Affine {
    fn map(&self, value: ExactRatio) -> Result<ExactRatio, String> {
        self.scale
            .checked_mul(value)
            .and_then(|value| self.offset.checked_add(value))
            .map_err(|error| error.to_string())
    }
    fn local(&self, root: ExactRatio) -> Result<ExactRatio, String> {
        root.checked_sub(self.offset)
            .and_then(|value| value.checked_div(self.scale))
            .map_err(|error| error.to_string())
    }
    fn child(
        &self,
        offset: ExactRatio,
        scale: ExactRatio,
        duration: FrameDuration,
    ) -> Result<Option<Self>, String> {
        let next_offset = self.map(offset)?;
        let next_scale = self
            .scale
            .checked_mul(scale)
            .map_err(|error| error.to_string())?;
        let end = next_scale
            .checked_mul(ExactRatio::integer(duration.frames()))
            .and_then(|span| next_offset.checked_add(span))
            .map_err(|error| error.to_string())?;
        let start = if next_offset.compare(self.clip.start).is_gt() {
            next_offset
        } else {
            self.clip.start
        };
        let end = if end.compare(self.clip.end).is_lt() {
            end
        } else {
            self.clip.end
        };
        Ok(start.compare(end).is_lt().then_some(Self {
            offset: next_offset,
            scale: next_scale,
            clip: start..end,
        }))
    }
}

struct ProjectionVisit {
    depth: usize,
    affine: Affine,
    instances: Vec<RepeatInstance>,
}

struct RepeatProjection {
    depth: usize,
    affine: Affine,
    instances: Vec<RepeatInstance>,
    indices: std::vec::IntoIter<u32>,
    best: Projection,
}

enum ProjectionStep {
    Visit(ProjectionVisit),
    Returned(Projection),
}

impl State {
    fn project(&self, target: &ScopedNodeTarget) -> Result<Projection, String> {
        let path = self.index.path(&target.node)?;
        let mut work = MAX_PROJECTION_WORK;
        let mut parents: Vec<RepeatProjection> = Vec::new();
        let mut step = ProjectionStep::Visit(ProjectionVisit {
            depth: 0,
            affine: Affine {
                offset: ExactRatio::ZERO,
                scale: ExactRatio::ONE,
                clip: ExactRatio::ZERO..ExactRatio::integer(self.index.plan.duration().frames()),
            },
            instances: Vec::new(),
        });
        loop {
            match step {
                ProjectionStep::Returned(projection) => {
                    if projection.presentation.is_some() {
                        return Ok(projection);
                    }
                    let Some(parent) = parents.last_mut() else {
                        return Ok(projection);
                    };
                    if parent.best.exact.is_none() || projection.exact.is_some() {
                        parent.best = projection;
                    }
                    step = if let Some(next) = self.next_projection(&path, parent)? {
                        ProjectionStep::Visit(next)
                    } else {
                        ProjectionStep::Returned(parents.pop().expect("pending Repeat").best)
                    };
                }
                ProjectionStep::Visit(visit) => {
                    if work == 0 {
                        step = ProjectionStep::Returned(Projection::unavailable(
                            "Representative picture search reached its work limit; choose a specific play.",
                        ));
                        continue;
                    }
                    work -= 1;
                    let ProjectionVisit {
                        depth,
                        affine,
                        instances,
                    } = visit;
                    if depth + 1 == path.len() {
                        step = ProjectionStep::Returned(self.sampled(
                            target,
                            affine.clip,
                            instances,
                        )?);
                        continue;
                    }
                    let node = &path[depth];
                    let child = &path[depth + 1];
                    let duration = self.index.durations[child];
                    let next = match &self.index.document.nodes()[node].kind {
                        NodeKind::Sequence { .. } => affine.child(
                            ExactRatio::integer(self.index.sequence_offsets[child]),
                            ExactRatio::ONE,
                            duration,
                        )?,
                        NodeKind::Retime {
                            duration: output,
                            mapping,
                            ..
                        } => {
                            let scale = ExactRatio::new(
                                i128::from(output.frames()),
                                i128::from(mapping.duration().frames()),
                            )
                            .map_err(|error| error.to_string())?;
                            let offset = ExactRatio::integer(mapping.start().0)
                                .checked_mul(scale)
                                .and_then(|value| ExactRatio::ZERO.checked_sub(value))
                                .map_err(|error| error.to_string())?;
                            affine.child(offset, scale, duration)?
                        }
                        NodeKind::Repeat { .. } => {
                            let Some(indices) = self.projection_indices(target, node, &affine)?
                            else {
                                step = ProjectionStep::Returned(Projection::unavailable(
                                    DORMANT_DEFAULT,
                                ));
                                continue;
                            };
                            let mut parent = RepeatProjection {
                                depth,
                                affine,
                                instances,
                                indices: indices.into_iter(),
                                best: Projection::unavailable(CROPPED),
                            };
                            step = if let Some(next) = self.next_projection(&path, &mut parent)? {
                                parents.push(parent);
                                ProjectionStep::Visit(next)
                            } else {
                                ProjectionStep::Returned(parent.best)
                            };
                            continue;
                        }
                        _ => return Err("A leaf cannot own an inspector child".into()),
                    };
                    step = match next {
                        Some(affine) => ProjectionStep::Visit(ProjectionVisit {
                            depth: depth + 1,
                            affine,
                            instances,
                        }),
                        None => ProjectionStep::Returned(Projection::unavailable(CROPPED)),
                    };
                }
            }
        }
    }

    /// Evaluate candidates lazily. A later arithmetic failure must not replace
    /// an earlier successful picture, and clipped candidates do not replace a
    /// previous exact interval. Each suspended Repeat lives in the heap stack.
    fn next_projection(
        &self,
        path: &[NodeId],
        parent: &mut RepeatProjection,
    ) -> Result<Option<ProjectionVisit>, String> {
        let node = &path[parent.depth];
        let child = &path[parent.depth + 1];
        let NodeKind::Repeat { iterations, .. } = &self.index.document.nodes()[node].kind else {
            return Err("Projection continuation no longer owns a Repeat".into());
        };
        for index in parent.indices.by_ref() {
            let iteration = iterations.at(index).ok_or("Repeat index disappeared")?;
            let play = self.index.repeats[node]
                .play(&iteration)
                .ok_or("Repeat layout omitted a play")?;
            let Some(offset) = play.branch_offset(child) else {
                parent.best = Projection::unavailable(DORMANT_GAP);
                continue;
            };
            let Some(affine) = parent.affine.child(
                ExactRatio::integer(offset),
                ExactRatio::ONE,
                self.index.durations[child],
            )?
            else {
                continue;
            };
            let mut instances = parent.instances.clone();
            instances.push(RepeatInstance {
                node: node.clone(),
                iteration,
            });
            return Ok(Some(ProjectionVisit {
                depth: parent.depth + 1,
                affine,
                instances,
            }));
        }
        Ok(None)
    }

    fn projection_indices(
        &self,
        target: &ScopedNodeTarget,
        node: &NodeId,
        affine: &Affine,
    ) -> Result<Option<Vec<u32>>, String> {
        let NodeKind::Repeat {
            iterations,
            child: literal,
            ..
        } = &self.index.document.nodes()[node].kind
        else {
            return Err("Projection candidates require a Repeat".into());
        };
        let step = target
            .repeats
            .iter()
            .find(|step| &step.repeat == node)
            .ok_or("Missing authored Repeat choice")?;
        let layout = &self.index.repeats[node];
        let mut indices = Vec::new();
        match &step.branch {
            RepeatEditBranch::Play { iteration } => indices.push(
                iterations
                    .position(iteration)
                    .ok_or("Selected play retired")?,
            ),
            RepeatEditBranch::Default => {
                let overridden = &self.index.overridden[node];
                if usize::try_from(iterations.len()).ok() == Some(overridden.len()) {
                    return Ok(None);
                }
                if let Some(index) = self
                    .preferred
                    .as_ref()
                    .and_then(|instance| instance.repeats.iter().find(|step| &step.node == node))
                    .and_then(|step| iterations.position(&step.iteration))
                    && overridden.binary_search(&index).is_err()
                {
                    indices.push(index);
                }
                let center = ExactRatio::integer(self.cursor.0)
                    .checked_add(ExactRatio::new(1, 2).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
                if !center.compare(affine.clip.start).is_lt()
                    && center.compare(affine.clip.end).is_lt()
                    && let Ok(found) = layout.locate(affine.local(center)?, InsertionBias::Right)
                    && &found.play.child == literal
                {
                    indices.push(found.play.index);
                }
                let half = ExactRatio::new(1, 2).map_err(|error| error.to_string())?;
                let first_frame = affine
                    .clip
                    .start
                    .checked_sub(half)
                    .and_then(ExactRatio::ceil)
                    .map_err(|error| error.to_string())?
                    .max(0);
                let first_center = ExactRatio::new(first_frame, 1)
                    .and_then(|value| value.checked_add(half))
                    .map_err(|error| error.to_string())?;
                if first_center.compare(affine.clip.end).is_lt()
                    && let Ok(found) =
                        layout.locate(affine.local(first_center)?, InsertionBias::Right)
                    && &found.play.child == literal
                {
                    indices.push(found.play.index);
                }
                let local = affine.local(affine.clip.start)?;
                if let Ok(found) = layout.locate(local, InsertionBias::Right) {
                    let mut index = found.play.index;
                    for _ in 0..2 {
                        let runs = &self.index.overridden_runs[node];
                        if let Some(run) = runs.get(runs.partition_point(|run| run.end <= index))
                            && run.start <= index
                        {
                            index = run.end;
                        }
                        if index >= iterations.len() {
                            break;
                        }
                        indices.push(index);
                        let Some(next) = index.checked_add(1) else {
                            break;
                        };
                        index = next;
                    }
                }
            }
        }
        let mut unique = Vec::with_capacity(indices.len());
        for index in indices {
            if !unique.contains(&index) {
                unique.push(index);
            }
        }
        Ok(Some(unique))
    }
}

impl State {
    fn sampled(
        &self,
        target: &ScopedNodeTarget,
        exact: Range<ExactRatio>,
        repeats: Vec<RepeatInstance>,
    ) -> Result<Projection, String> {
        let half = ExactRatio::new(1, 2).map_err(|error| error.to_string())?;
        let bound = |value: ExactRatio| -> Result<u64, String> {
            let value = value
                .checked_sub(half)
                .and_then(ExactRatio::ceil)
                .map_err(|error| error.to_string())?;
            u64::try_from(
                value
                    .max(0)
                    .min(i128::from(self.index.plan.duration().frames())),
            )
            .map_err(|error| error.to_string())
        };
        let frames = bound(exact.start)?..bound(exact.end)?;
        let instance = InstancePath {
            node: target.node.clone(),
            repeats,
        };
        let matches = |frame: u64| -> Result<bool, String> {
            let picture = self
                .index
                .plan
                .picture(ProjectFrame(
                    i64::try_from(frame).map_err(|error| error.to_string())?,
                ))
                .map_err(|error| error.to_string())?;
            Ok(picture
                .framing
                .iter()
                .any(|framing| framing.instance == instance))
        };
        let presentation =
            if !frames.is_empty() && matches(frames.start)? && matches(frames.end - 1)? {
                Some(Presentation {
                    instance,
                    exact: exact.clone(),
                    frames,
                })
            } else {
                None
            };
        Ok(Projection {
            reason: presentation.is_none().then(|| UNSAMPLED.into()),
            presentation,
            exact: Some(exact),
        })
    }
}

#[cfg(test)]
mod tests;
