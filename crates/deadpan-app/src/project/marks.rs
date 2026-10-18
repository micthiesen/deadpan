//! Revision-bound native letter marks. The reserved ID selects a keyboard mark;
//! its exact single-letter label confirms that the native namespace is intended.

use deadpan_core::{
    AssetId, ExactRatio, MarkId, NodeId, ProjectFrame, ProjectId, RevisionId, SourceQualificationId,
};

use super::{SequenceScope, Workspace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Id {
    pub ticket: u64,
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        /// A measured presentation ordinal, including the exact terminal boundary.
        ordinal: u64,
    },
    Edit {
        scope: SequenceScope,
        at: ProjectFrame,
        selected: Option<NodeId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Set { letter: char, location: Location },
    Jump { letter: char },
    Delete { letter: char },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: Id,
    pub operation: Operation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedLocation {
    Original {
        asset: AssetId,
        qualification: SourceQualificationId,
        ordinal: u64,
    },
    Edit {
        scope: SequenceScope,
        selected: Option<NodeId>,
        frame: ProjectFrame,
        exact_frame: ExactRatio,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub id: Id,
    pub letter: char,
    pub revision: RevisionId,
    pub refresh_error: Option<String>,
}

impl Saved {
    /// A later Undo has a fresh revision and cannot revive this warning.
    pub fn needs_refresh(&self, workspace: Option<&Workspace>) -> bool {
        workspace.is_some_and(|workspace| {
            workspace.session == self.id.session
                && workspace.document.project_id() == &self.id.project
                && workspace.document.revision_id() == &self.id.revision
                && workspace.document.revision_id() != &self.revision
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Saved(Saved),
    Jumped(ResolvedLocation),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub id: Id,
    pub result: Result<Outcome, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Update {
    pub reply: Option<Reply>,
    /// Independent of later queries and failures, including refresh failure.
    pub saved: Option<Saved>,
    /// A successful mark-only revision, including a Macro or headless write.
    /// It grants metadata rebasing, never a cursor continuation.
    pub metadata: Option<MetadataRevision>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataRevision {
    pub session: u64,
    pub project: ProjectId,
    pub before: RevisionId,
    pub after: RevisionId,
    /// First revision in this contiguous run of mark-only commits.
    pub base: RevisionId,
}

impl Update {
    pub fn record_metadata(
        &mut self,
        session: u64,
        project: &ProjectId,
        before: &RevisionId,
        after: &RevisionId,
    ) {
        let base = self
            .metadata
            .as_ref()
            .filter(|previous| {
                previous.session == session
                    && &previous.project == project
                    && &previous.after == before
            })
            .map_or_else(|| before.clone(), |previous| previous.base.clone());
        self.metadata = Some(MetadataRevision {
            session,
            project: project.clone(),
            before: before.clone(),
            after: after.clone(),
            base,
        });
    }
}

impl MetadataRevision {
    /// The service keeps one contiguous mark-run base. A consumer retains just
    /// its last admitted proof to recognize an intermediate visible revision.
    pub fn admits_previous(&self, before: &RevisionId, seen: Option<&Self>) -> bool {
        before != &self.after
            && (before == &self.before
                || before == &self.base
                || seen.is_some_and(|seen| {
                    seen.session == self.session
                        && seen.project == self.project
                        && seen.base == self.base
                        && &seen.after == before
                }))
    }

    pub fn matches(&self, workspace: &Workspace) -> bool {
        workspace.session == self.session
            && workspace.document.project_id() == &self.project
            && workspace.document.revision_id() == &self.after
    }
}

pub fn mark_id(letter: char) -> Result<MarkId, String> {
    if !letter.is_ascii_alphabetic() {
        return Err("A mark uses one letter: a–z or A–Z".into());
    }
    MarkId::new(format!("native-mark-{letter}")).map_err(|error| error.to_string())
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    fn revision(name: &str) -> RevisionId {
        RevisionId::new(name).unwrap()
    }

    #[test]
    fn mark_metadata_coalescing_retains_only_proven_contiguous_ancestry() {
        let project = ProjectId::new("metadata").unwrap();
        let mut update = Update::default();
        update.record_metadata(1, &project, &revision("a"), &revision("b"));
        update.record_metadata(1, &project, &revision("b"), &revision("c"));
        let seen = update.metadata.clone().unwrap();
        assert_eq!(seen.base, revision("a"));
        assert_eq!(seen.before, revision("b"));
        assert!(seen.admits_previous(&revision("a"), None));
        assert!(!seen.admits_previous(&revision("c"), Some(&seen)));
        update.record_metadata(1, &project, &revision("c"), &revision("d"));
        update.record_metadata(1, &project, &revision("d"), &revision("e"));
        let next = update.metadata.as_ref().unwrap();
        assert!(!next.admits_previous(&revision("c"), None));
        assert!(next.admits_previous(&revision("c"), Some(&seen)));
        let mut foreign = seen.clone();
        foreign.session = 2;
        assert!(!next.admits_previous(&revision("c"), Some(&foreign)));
        // An intervening ordinary edit breaks the run even within this session.
        update.record_metadata(1, &project, &revision("structural"), &revision("later"));
        let next = update.metadata.as_ref().unwrap();
        assert_eq!(next.base, revision("structural"));
        assert!(!next.admits_previous(&revision("c"), Some(&seen)));
        assert!(next.admits_previous(&revision("structural"), None));
        // A different project or session cannot inherit the prior base.
        update.record_metadata(2, &project, &revision("later"), &revision("new-session"));
        assert_eq!(update.metadata.as_ref().unwrap().base, revision("later"));
        let other = ProjectId::new("other").unwrap();
        update.record_metadata(
            2,
            &other,
            &revision("new-session"),
            &revision("new-project"),
        );
        assert_eq!(
            update.metadata.as_ref().unwrap().base,
            revision("new-session")
        );
    }
}
