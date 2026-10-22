//! Host-supplied transcript intent with checked, revision-bound Edit geometry.

use std::sync::Arc;

use crate::{
    EditError, EditErrorCode, FrameRange, NodeId, ProjectDocument, ProjectId, RevisionId,
    SemanticContext, SpeechTimeline,
};

pub const MAX_SEMANTIC_TRANSCRIPT_QUERY_BYTES: usize = 4096;

/// Preserve exactly the query the user supplied. Normalization and matching
/// belong to the analysis host; Core only admits bounded, nonempty intent.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct SemanticTranscriptQuery(String);

impl SemanticTranscriptQuery {
    pub fn new(query: impl Into<String>) -> Result<Self, EditError> {
        let query = query.into();
        if query.len() > MAX_SEMANTIC_TRANSCRIPT_QUERY_BYTES {
            return Err(EditError::new(
                EditErrorCode::LimitExceeded,
                "transcript search query exceeds 4096 bytes",
            ));
        }
        if query.trim().is_empty() {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "transcript search requires a nonempty query",
            ));
        }
        Ok(Self(query))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for SemanticTranscriptQuery {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Query;
        impl serde::de::Visitor<'_> for Query {
            type Value = SemanticTranscriptQuery;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a nonempty transcript query of at most 4096 bytes")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                SemanticTranscriptQuery::new(value).map_err(E::custom)
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                SemanticTranscriptQuery::new(value).map_err(E::custom)
            }
        }
        deserializer.deserialize_string(Query)
    }
}

/// A host result grants geometry only for this staged document, scope and raw
/// query. Its constructor checks the ordinary linked range editing contract.
/// Analysis freshness remains the host's transactional responsibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticTranscriptMatches {
    project: ProjectId,
    revision: RevisionId,
    parent: NodeId,
    query: SemanticTranscriptQuery,
    ranges: Vec<FrameRange>,
}

impl SemanticTranscriptMatches {
    pub fn new(
        document: &ProjectDocument,
        context: &SemanticContext,
        query: &SemanticTranscriptQuery,
        ranges: Vec<FrameRange>,
    ) -> Result<Self, EditError> {
        super::range_set::validate_geometry(document, context, &ranges)?;
        Ok(Self {
            project: document.project_id().clone(),
            revision: document.revision_id().clone(),
            parent: context.parent.clone(),
            query: query.clone(),
            ranges,
        })
    }

    pub fn ranges(&self) -> &[FrameRange] {
        &self.ranges
    }

    pub(super) fn resolve(
        self,
        document: &ProjectDocument,
        context: &SemanticContext,
        query: &SemanticTranscriptQuery,
    ) -> Result<Vec<FrameRange>, EditError> {
        if self.project != *document.project_id()
            || self.revision != *document.revision_id()
            || self.parent != context.parent
            || self.query != *query
        {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "transcript search result belongs to another staged document, scope or query",
            ));
        }
        super::range_set::validate_geometry(document, context, &self.ranges)?;
        Ok(self.ranges)
    }
}

/// One invocation's host owns transcript loading and projects its frozen
/// analysis through each staged document. Both operations must share that
/// analysis snapshot when a program combines word motions and search.
pub trait SemanticSpeechProvider {
    /// Record that this invocation consumes transcript words, even when a
    /// pause or shot instruction already populated the timeline cache. Hosts
    /// with transactional analysis authority retain its guard here. Legacy
    /// timeline closures have no additional authority to retain.
    fn require_words(&mut self) -> Result<(), EditError> {
        Ok(())
    }

    fn timeline(&mut self, document: &ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>;

    fn transcript_search(
        &mut self,
        document: &ProjectDocument,
        context: &SemanticContext,
        query: &SemanticTranscriptQuery,
    ) -> Result<SemanticTranscriptMatches, EditError>;
}

impl<T: SemanticSpeechProvider + ?Sized> SemanticSpeechProvider for &mut T {
    fn require_words(&mut self) -> Result<(), EditError> {
        (**self).require_words()
    }

    fn timeline(&mut self, document: &ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError> {
        (**self).timeline(document)
    }

    fn transcript_search(
        &mut self,
        document: &ProjectDocument,
        context: &SemanticContext,
        query: &SemanticTranscriptQuery,
    ) -> Result<SemanticTranscriptMatches, EditError> {
        (**self).transcript_search(document, context, query)
    }
}

pub(super) struct TimelineProvider<F>(pub F);

impl<F> SemanticSpeechProvider for TimelineProvider<F>
where
    F: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
{
    fn timeline(&mut self, document: &ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError> {
        (self.0)(document)
    }

    fn transcript_search(
        &mut self,
        _document: &ProjectDocument,
        _context: &SemanticContext,
        _query: &SemanticTranscriptQuery,
    ) -> Result<SemanticTranscriptMatches, EditError> {
        Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "transcript search requires the host's qualified search provider",
        ))
    }
}
