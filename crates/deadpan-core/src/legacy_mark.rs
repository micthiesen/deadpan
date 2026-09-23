//! Frozen core-3 through core-12 mark wire grammar.
//! Even an empty or null fragments field belongs to a newer schema.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{AnchorLossPolicy, BoundaryAnchor, Mark, MarkId, MarkState, NodeId, ValueChange};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyMark {
    owner: NodeId,
    label: String,
    boundary: BoundaryAnchor,
    loss_policy: AnchorLossPolicy,
    state: MarkState,
}

impl LegacyMark {
    fn upgrade(self) -> Mark {
        Mark {
            owner: self.owner,
            label: self.label,
            boundary: self.boundary,
            loss_policy: self.loss_policy,
            state: self.state,
            fragments: Vec::new(),
        }
    }

    fn project(mark: &Mark) -> Option<Self> {
        mark.fragments.is_empty().then(|| Self {
            owner: mark.owner.clone(),
            label: mark.label.clone(),
            boundary: mark.boundary.clone(),
            loss_policy: mark.loss_policy,
            state: mark.state.clone(),
        })
    }
}

pub(crate) fn upgrade_marks(marks: BTreeMap<MarkId, LegacyMark>) -> BTreeMap<MarkId, Mark> {
    marks
        .into_iter()
        .map(|(id, mark)| (id, mark.upgrade()))
        .collect()
}

pub(crate) fn project_marks(
    marks: &BTreeMap<MarkId, Mark>,
) -> Option<BTreeMap<MarkId, LegacyMark>> {
    marks
        .iter()
        .map(|(id, mark)| Some((id.clone(), LegacyMark::project(mark)?)))
        .collect()
}

pub(crate) fn project_mark_changes(
    changes: &BTreeMap<MarkId, ValueChange<Mark>>,
) -> Option<BTreeMap<MarkId, ValueChange<LegacyMark>>> {
    changes
        .iter()
        .map(|(id, change)| {
            Some((
                id.clone(),
                ValueChange {
                    before: match &change.before {
                        Some(mark) => Some(LegacyMark::project(mark)?),
                        None => None,
                    },
                    after: match &change.after {
                        Some(mark) => Some(LegacyMark::project(mark)?),
                        None => None,
                    },
                },
            ))
        })
        .collect()
}
