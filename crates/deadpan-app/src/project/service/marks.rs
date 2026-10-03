//! Mark-only transactions and identity-preserving native navigation.

use super::*;
use crate::project::marks::{self as contract, Location, Operation, Outcome, ResolvedLocation};
use deadpan_core::{
    Anchor, AnchorIndex, AnchorLossPolicy, BoundaryAnchor, BoundarySelector, ExactRatio,
    InsertionBias, InstancePath, MarkState, MediaRole, NamedMarkTarget, ProjectFrame,
    ResolvedSelectionKind, SelectionRequest, SourceMoment, SourceStream, SourceTimestamp,
};

#[derive(Default)]
pub(super) struct State {
    ticket: u64,
    successful: Option<(contract::Request, Outcome)>,
    saved_request: Option<contract::Request>,
}

impl Service {
    pub(super) fn clear_marks(&mut self) {
        self.marks = contract::Update::default();
        self.marks_state = State::default();
    }

    pub(super) fn marks_command(&mut self, request: contract::Request) {
        // Invalidate an older selection-changing receipt before publishing any
        // new mark intent, including jumps and rejected requests.
        self.committed = None;
        let result = self.execute_mark(&request);
        if let Ok(outcome) = &result {
            self.marks_state.successful = Some((request.clone(), outcome.clone()));
        }
        self.marks.reply = Some(contract::Reply {
            id: request.id,
            result,
        });
    }

    fn execute_mark(&mut self, request: &contract::Request) -> Result<Outcome> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if workspace.session != request.id.session
            || workspace.document.project_id() != &request.id.project
        {
            return Err("The mark belongs to a different project session".into());
        }
        if let Some(previous) = &self.marks_state.saved_request
            && previous.id == request.id
        {
            if previous != request {
                return Err(
                    "The mark request identity was already used for a different operation".into(),
                );
            }
            return self
                .marks
                .saved
                .clone()
                .map(Outcome::Saved)
                .ok_or_else(|| "The saved mark receipt is unavailable".into());
        }
        if let Some((previous, result)) = &self.marks_state.successful
            && previous.id == request.id
        {
            return if previous == request {
                Ok(result.clone())
            } else {
                Err("The mark request identity was already used for a different operation".into())
            };
        }
        if request.id.ticket == 0 || request.id.ticket <= self.marks_state.ticket {
            return Err("The mark request ticket is no longer available".into());
        }
        self.check_context(request.id.session, &request.id.revision)?;
        // A failed workspace refresh must not permit queries against stale
        // authored state, even when another command caused that failure.
        if self.writer()?.snapshot().map_err(display)?.revision_id() != &request.id.revision {
            return Err("The saved project changed; reopen it before using marks".into());
        }
        self.marks_state.ticket = request.id.ticket;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let (letter, command) = match &request.operation {
            Operation::Jump { letter } => {
                return resolve(workspace, *letter).map(Outcome::Jumped);
            }
            Operation::Set { letter, location } => {
                let id = native_mark_id(workspace, *letter)?;
                let (owner, boundary) = capture(workspace, location)?;
                (
                    *letter,
                    Command::SetMark {
                        id,
                        owner,
                        label: letter.to_string(),
                        boundary,
                        loss_policy: AnchorLossPolicy::KeepUnresolved,
                    },
                )
            }
            Operation::Delete { letter } => {
                let id = native_mark_id(workspace, *letter)?;
                if !workspace.document.marks().contains_key(&id) {
                    return Err(format!("Mark {letter} has not been set"));
                }
                (*letter, Command::DeleteMark { id })
            }
        };
        let committed = self
            .writer()?
            .commit(&CommandRequest {
                project_id: request.id.project.clone(),
                expected_revision: request.id.revision.clone(),
                new_revision: revision(),
                command,
            })
            .map_err(display)?;
        self.preserve_semantic(&request.id.revision, &committed.revision_id);
        let mut saved = contract::Saved {
            id: request.id.clone(),
            letter,
            revision: committed.revision_id,
            refresh_error: None,
        };
        self.marks.saved = Some(saved.clone());
        self.marks_state.saved_request = Some(request.clone());
        #[cfg(test)]
        {
            self.render_preview_refresh_failure = self
                .shared
                .render_commit_refresh_failure
                .swap(false, Ordering::AcqRel);
        }
        let action = if matches!(request.operation, Operation::Delete { .. }) {
            "deleted"
        } else {
            "saved"
        };
        match self.refresh() {
            Ok(()) => self.message = Some(format!("Mark {letter} {action}. Undo with u.")),
            Err(error) => {
                let message = format!(
                    "Mark {letter} {action}, but the preview could not refresh: {error}. Reopen this project before editing or undoing."
                );
                saved.refresh_error = Some(message.clone());
                self.message = Some(message);
            }
        }
        self.error = None;
        self.marks.saved = Some(saved.clone());
        Ok(Outcome::Saved(saved))
    }
}

/// The native key address shares the authored mark API. Preserve explicit CLI
/// marks with that address unless their label declares the same native letter.
fn native_mark_id(workspace: &Workspace, letter: char) -> Result<deadpan_core::MarkId> {
    let id = contract::mark_id(letter)?;
    if workspace
        .document
        .marks()
        .get(&id)
        .is_some_and(|mark| mark.label != letter.to_string())
    {
        return Err(format!(
            "Mark {letter} conflicts with a named mark using its native key address"
        ));
    }
    Ok(id)
}

fn capture(workspace: &Workspace, location: &Location) -> Result<(NodeId, BoundaryAnchor)> {
    match location {
        Location::Original {
            asset,
            qualification,
            ordinal,
        } => {
            let source = source(workspace, asset)?;
            if source.receipt.id() != qualification {
                return Err("Original mark qualification changed before capture".into());
            }
            let index = source
                .video_index
                .as_ref()
                .ok_or("Original has no picture index")?;
            let ordinal = usize::try_from(*ordinal).map_err(display)?;
            let ticks = if ordinal == index.frames().len() {
                index.terminal_end()
            } else {
                index
                    .frames()
                    .get(ordinal)
                    .ok_or("Original mark ordinal is outside its measured picture index")?
                    .pts
            };
            Ok((
                workspace.document.root().clone(),
                BoundaryAnchor {
                    coordinate: Anchor::Source {
                        asset: asset.clone(),
                        moment: SourceMoment::Timestamp {
                            stream: SourceStream::Video,
                            timestamp: SourceTimestamp {
                                ticks,
                                time_base: index.time_base(),
                            },
                        },
                    },
                    bias: if ordinal == index.frames().len() {
                        InsertionBias::Left
                    } else {
                        InsertionBias::Right
                    },
                },
            ))
        }
        Location::Edit {
            scope,
            at,
            selected,
        } => capture_edit(workspace, scope, *at, selected.as_ref()),
    }
}

fn source<'a>(workspace: &'a Workspace, asset: &AssetId) -> Result<&'a RegisteredSource> {
    let source = workspace
        .sources
        .get(asset)
        .ok_or("Original mark media is unavailable")?;
    let metadata = workspace
        .document
        .assets()
        .get(asset)
        .ok_or("Original mark asset is missing")?;
    let index = source
        .video_index
        .as_ref()
        .ok_or("Original has no picture index")?;
    if source.asset != *asset
        || index.asset() != asset
        || metadata.source_qualification.as_ref() != Some(source.receipt.id())
    {
        return Err("Original mark differs from its admitted source qualification".into());
    }
    Ok(source)
}

fn capture_edit(
    workspace: &Workspace,
    scope: &SequenceScope,
    at: ProjectFrame,
    selected: Option<&NodeId>,
) -> Result<(NodeId, BoundaryAnchor)> {
    let view = scope.resolve(workspace)?;
    let at = u64::try_from(at.0).map_err(|_| "Mark is outside the active Sequence")?;
    if at < view.start || at > view.end {
        return Err("Mark is outside the active Sequence".into());
    }
    if selected.is_some_and(|node| !view.children.contains(node)) {
        return Err("The selected mark host is outside the active Sequence".into());
    }
    let mut start = view.start;
    let mut chosen = None;
    let mut explicit_empty = None;
    for child in view.children {
        let duration = workspace
            .plan
            .node_duration(child)
            .ok_or("Mark host is missing")?;
        let duration = u64::try_from(duration.frames()).map_err(display)?;
        let end = start
            .checked_add(duration)
            .ok_or("Mark host position overflowed")?;
        if duration == 0 && at == start && selected == Some(child) {
            explicit_empty = Some((child, start, end));
        }
        if (start <= at && at < end) || (at == view.end && Some(child) == view.children.last()) {
            chosen = Some((child, start, end));
        }
        start = end;
    }
    let (host, start, end) = explicit_empty
        .or(chosen)
        .unwrap_or((view.owner, view.start, view.end));
    let position = at.checked_sub(start).ok_or("Mark precedes its host")?;
    let position = i64::try_from(position).map_err(display)?;
    Ok((
        host.clone(),
        BoundaryAnchor {
            coordinate: Anchor::Occurrence {
                instance: InstancePath {
                    node: host.clone(),
                    repeats: Vec::new(),
                },
                position: ExactRatio::integer(position),
            },
            bias: if at == end {
                InsertionBias::Left
            } else {
                InsertionBias::Right
            },
        },
    ))
}

fn resolve(workspace: &Workspace, letter: char) -> Result<ResolvedLocation> {
    let id = native_mark_id(workspace, letter)?;
    let mark = workspace
        .document
        .marks()
        .get(&id)
        .ok_or_else(|| format!("Mark {letter} has not been set"))?;
    let bindings: Vec<_> = mark
        .bindings()
        .filter(|binding| binding.state == MarkState::Bound)
        .collect();
    if bindings.is_empty() {
        return Err(format!(
            "Mark {letter} is unresolved: its content is no longer available"
        ));
    }
    if bindings
        .iter()
        .any(|binding| matches!(binding.coordinate, Anchor::Source { .. }))
    {
        let mut original = None;
        for binding in bindings {
            let Anchor::Source {
                asset,
                moment:
                    SourceMoment::Timestamp {
                        stream: SourceStream::Video,
                        timestamp,
                    },
            } = binding.coordinate
            else {
                return Err(format!(
                    "Mark {letter} has ambiguous Original and edit bindings"
                ));
            };
            let source = source(workspace, &asset)?;
            let index = source
                .video_index
                .as_ref()
                .ok_or("Original has no picture index")?;
            if timestamp.time_base != index.time_base() {
                return Err("Original mark timestamp clock differs from the measured index".into());
            }
            let ordinal = if timestamp.ticks == index.terminal_end() {
                index.frames().len()
            } else {
                index
                    .frames()
                    .binary_search_by_key(&timestamp.ticks, |frame| frame.pts)
                    .map_err(|_| "Original mark timestamp is absent from the measured index")?
            };
            let candidate = ResolvedLocation::Original {
                asset,
                qualification: source.receipt.id().clone(),
                ordinal: u64::try_from(ordinal).map_err(display)?,
            };
            if original
                .as_ref()
                .is_some_and(|previous| previous != &candidate)
            {
                return Err(format!(
                    "Mark {letter} resolves to distinct Original boundaries"
                ));
            }
            original = Some(candidate);
        }
        return original.ok_or_else(|| "Original mark has no bound location".into());
    }
    let resolved = AnchorIndex::new(&workspace.document)
        .map_err(display)?
        .resolve(&SelectionRequest {
            project_id: workspace.document.project_id().clone(),
            expected_revision: workspace.document.revision_id().clone(),
            role: MediaRole::Linked,
            selector: BoundarySelector::Mark {
                target: NamedMarkTarget {
                    id,
                    occurrence: None,
                },
            },
        })
        .map_err(display)?;
    let ResolvedSelectionKind::Point { point } = resolved.selection else {
        return Err("A mark must resolve to one boundary".into());
    };
    let resolved_mark = point
        .mark
        .as_deref()
        .ok_or("Mark resolution did not retain its binding identities")?;
    let bindings: Vec<_> = mark.bindings().collect();
    let mut context = None;
    for ordinal in &resolved_mark.bindings {
        let binding = bindings
            .get(*ordinal)
            .ok_or("Mark resolution returned an invalid binding identity")?;
        let host = match &binding.coordinate {
            Anchor::Local { node, .. } => node,
            Anchor::Occurrence { instance, .. } => &instance.node,
            Anchor::Sequence { .. } => workspace.document.root(),
            Anchor::Source { .. } => {
                return Err("Original mark requires its measured source index".into());
            }
        };
        let candidate = SequenceScope::for_target(workspace, host)?;
        if context
            .as_ref()
            .is_some_and(|previous| previous != &candidate)
        {
            return Err(format!(
                "Mark {letter} resolves to distinct accessible edit targets"
            ));
        }
        context = Some(candidate);
    }
    let (scope, selected) =
        context.ok_or_else(|| format!("Mark {letter} has no accessible edit target"))?;
    Ok(ResolvedLocation::Edit {
        scope,
        selected,
        frame: point.frame,
        exact_frame: point.exact_frame,
    })
}
