//! Explicit exemptions from one current silent Hold for one root contribution.
//! These identities never describe sample ranges, intrinsic definition views,
//! source exhaustion, or a permission to fill a retained routing gap.

use std::collections::BTreeMap;
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::{
    Command, DocumentError, DocumentErrorCode, EditError, EditErrorCode, HoldAudio, InstancePath,
    IterationId, NodeId, NodeKind, ProjectDocument, RepeatInstance, SoundId,
};

pub const MAX_SOUND_ALLOWANCES_PER_EVENT: usize = 1024;
pub const MAX_DOCUMENT_SOUND_ALLOWANCES: usize = 8192;
pub const MAX_DOCUMENT_SOUND_ALLOWANCE_BYTES: usize = 1024 * 1024;

/// A concrete issuer in the current root output. Every repeated ancestor must
/// be present. A Repeat gap names its stable preceding play, never an ordinal
/// position or an unplayed gap definition.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SoundHoldIssuer {
    Node {
        instance: InstancePath,
    },
    RepeatGap {
        instance: InstancePath,
        gap_after: IterationId,
    },
}

impl SoundHoldIssuer {
    pub fn instance(&self) -> &InstancePath {
        match self {
            Self::Node { instance } | Self::RepeatGap { instance, .. } => instance,
        }
    }

    fn instance_mut(&mut self) -> &mut InstancePath {
        match self {
            Self::Node { instance } | Self::RepeatGap { instance, .. } => instance,
        }
    }

    /// Validate an address without enumerating any Repeat's plays.
    pub fn validate(&self, document: &ProjectDocument) -> Result<(), DocumentError> {
        self.validate_with_parents(document, &parents(document))
    }

    fn validate_with_parents(
        &self,
        document: &ProjectDocument,
        parents: &BTreeMap<&NodeId, &NodeId>,
    ) -> Result<(), DocumentError> {
        let instance = self.instance();
        instance.validate_with_parents(document, parents)?;
        match (self, &document.nodes[&instance.node].kind) {
            (Self::Node { .. }, NodeKind::Hold { recipe })
                if matches!(recipe.audio, HoldAudio::Silence) =>
            {
                Ok(())
            }
            (
                Self::RepeatGap { gap_after, .. },
                NodeKind::Repeat {
                    iterations,
                    gap: Some(gap),
                    ..
                },
            ) if matches!(gap.audio, HoldAudio::Silence)
                && iterations.position(gap_after).is_some_and(|position| {
                    position
                        .checked_add(1)
                        .is_some_and(|next| next < iterations.len())
                })
                && !document
                    .gap_overrides
                    .get(&instance.node)
                    .is_some_and(|entries| entries.get(gap_after).is_some()) =>
            {
                Ok(())
            }
            _ => Err(invalid(
                "sound allowance must name one current silent Hold or default Repeat gap",
            )),
        }
    }
}

/// Canonically sorted, duplicate-free issuer identities for one sound.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SoundHoldAllowances(Vec<SoundHoldIssuer>);

impl SoundHoldAllowances {
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &SoundHoldIssuer> {
        self.0.iter()
    }
    pub fn contains(&self, issuer: &SoundHoldIssuer) -> bool {
        self.0.binary_search(issuer).is_ok()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl TryFrom<Vec<SoundHoldIssuer>> for SoundHoldAllowances {
    type Error = DocumentError;
    fn try_from(mut values: Vec<SoundHoldIssuer>) -> Result<Self, Self::Error> {
        if values.len() > MAX_SOUND_ALLOWANCES_PER_EVENT {
            return Err(limit("sound exceeds 1024 silent-Hold allowances"));
        }
        for issuer in &values {
            issuer.instance().validate_depth()?;
        }
        values.sort();
        if values.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid("duplicate sound Hold allowance"));
        }
        let result = Self(values);
        validate_bytes(&result)?;
        Ok(result)
    }
}

impl<'de> Deserialize<'de> for SoundHoldAllowances {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = SoundHoldAllowances;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("at most 1024 distinct silent-Hold issuers")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element()? {
                    if values.len() == MAX_SOUND_ALLOWANCES_PER_EVENT {
                        return Err(serde::de::Error::custom("sound Hold allowance limit"));
                    }
                    values.push(value);
                }
                SoundHoldAllowances::try_from(values).map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_seq(Visitor)
    }
}

pub(crate) fn validate(document: &ProjectDocument) -> Result<(), DocumentError> {
    if document.sound_allowances.is_empty() {
        return Ok(());
    }
    validate_limits(&document.sound_allowances)?;
    let parents = parents(document);
    for (sound, allowances) in &document.sound_allowances {
        if !document.sounds.contains_key(sound) || allowances.is_empty() {
            return Err(invalid(
                "nonempty sound allowances must belong to an existing sound",
            ));
        }
        for issuer in allowances.iter() {
            issuer.validate_with_parents(document, &parents)?;
        }
    }
    Ok(())
}

pub(crate) fn set(
    document: &mut ProjectDocument,
    sound: &SoundId,
    issuer: &SoundHoldIssuer,
    allowed: bool,
) -> Result<(), EditError> {
    if !document.sounds.contains_key(sound) {
        return Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "sound event is absent",
        ));
    }
    issuer.validate(document)?;
    let mut values = document
        .sound_allowances
        .get(sound)
        .cloned()
        .unwrap_or_default()
        .0;
    match (values.binary_search(issuer), allowed) {
        (Err(at), true) => values.insert(at, issuer.clone()),
        (Ok(at), false) => {
            values.remove(at);
        }
        _ => {}
    }
    if values.is_empty() {
        document.sound_allowances.remove(sound);
    } else {
        document
            .sound_allowances
            .insert(sound.clone(), SoundHoldAllowances::try_from(values)?);
    }
    validate_limits(&document.sound_allowances)?;
    Ok(())
}

/// The outer transaction detaches this relation before structural helpers
/// capture soundless contexts. Identity transforms update it independently;
/// the relation is restored only after the sound bus and final tree are ready.
pub(crate) struct SoundAllowanceEdit {
    values: BTreeMap<SoundId, SoundHoldAllowances>,
}

impl SoundAllowanceEdit {
    pub(crate) fn capture(document: &ProjectDocument, command: &Command) -> Option<Self> {
        (!document.sound_allowances.is_empty()
            && !matches!(command, Command::SetSoundAllowance { .. }))
        .then(|| Self {
            values: document.sound_allowances.clone(),
        })
    }

    /// A transparent Split retains both complete contexts, including hidden
    /// definitions. It does not grant anything to newly inserted Hold nodes.
    pub(crate) fn split(&mut self, mapping: &BTreeMap<NodeId, NodeId>) -> Result<(), EditError> {
        for allowances in self.values.values_mut() {
            let mut values = allowances.0.clone();
            for issuer in allowances.iter() {
                if mapping.contains_key(&issuer.instance().node) {
                    let mut copy = issuer.clone();
                    crate::occurrence_edit::remap_instance(copy.instance_mut(), mapping);
                    values.push(copy);
                }
            }
            *allowances = SoundHoldAllowances::try_from(values)?;
        }
        validate_limits(&self.values)?;
        Ok(())
    }

    /// Isolation moves only addresses in the chosen occurrence. Other plays
    /// keep their original definition IDs and never inherit this exemption.
    pub(crate) fn isolate(
        &mut self,
        prefix: &[RepeatInstance],
        mapping: &BTreeMap<NodeId, NodeId>,
    ) -> Result<(), EditError> {
        for allowances in self.values.values_mut() {
            let mut values = allowances.0.clone();
            for issuer in &mut values {
                if issuer.instance().repeats.starts_with(prefix)
                    && mapping.contains_key(&issuer.instance().node)
                {
                    crate::occurrence_edit::remap_instance(issuer.instance_mut(), mapping);
                }
            }
            *allowances = SoundHoldAllowances::try_from(values)?;
        }
        validate_limits(&self.values)?;
        Ok(())
    }

    pub(crate) fn restore(mut self, document: &mut ProjectDocument) -> Result<(), EditError> {
        self.values.retain(|sound, allowances| {
            if !document.sounds.contains_key(sound) {
                return false;
            }
            // An authored deletion removes the relation with its issuer. A
            // surviving complete context can remain temporarily inaudible.
            allowances
                .0
                .retain(|issuer| document.nodes.contains_key(&issuer.instance().node));
            !allowances.is_empty()
        });
        document.sound_allowances = self.values;
        validate(document)?;
        Ok(())
    }
}

fn parents(document: &ProjectDocument) -> BTreeMap<&NodeId, &NodeId> {
    document
        .nodes
        .keys()
        .flat_map(|id| document.children(id).map(move |child| (child, id)))
        .collect()
}

fn validate_limits(values: &BTreeMap<SoundId, SoundHoldAllowances>) -> Result<(), DocumentError> {
    if values.len() > crate::MAX_DOCUMENT_SOUNDS
        || values.values().map(SoundHoldAllowances::len).sum::<usize>()
            > MAX_DOCUMENT_SOUND_ALLOWANCES
    {
        return Err(limit("document exceeds 8192 silent-Hold allowances"));
    }
    validate_bytes(values)
}

fn validate_bytes(value: &impl Serialize) -> Result<(), DocumentError> {
    struct Count(usize);
    impl Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_DOCUMENT_SOUND_ALLOWANCE_BYTES - self.0 {
                return Err(std::io::Error::other("sound Hold allowance byte limit"));
            }
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Count(0), value).map_err(|error| {
        if error.is_io() {
            limit("sound Hold allowances exceed 1 MiB")
        } else {
            DocumentError::json(error)
        }
    })
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidIdentity, message)
}
fn limit(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}
