//! Independent authored sound recipes in their owner's output clock.
//!
//! Root-routed recipes and owner-local recipes are persisted separately. Exact
//! timed edits can transport unchanged top-level sound-bearing subtrees; other
//! temporal and ownership changes remain explicit refusals.

use std::collections::{BTreeMap, BTreeSet};

use serde::{
    Deserialize, Serialize,
    de::{MapAccess, Visitor},
};
use std::{fmt, marker::PhantomData};

use crate::{
    AudioEdgePolicy, AudioSample, Command, DocumentError, DocumentErrorCode, EditError,
    EditErrorCode, FrameDuration, NodeId, OccurrenceEdit, ProjectDocument, SourceAudio,
    SourceAudioMapping,
};

pub const MAX_DOCUMENT_SOUNDS: usize = 64;
pub const MIN_SOUND_GAIN_MILLIDECIBELS: i32 = -96_000;
pub const MAX_SOUND_GAIN_MILLIDECIBELS: i32 = 24_000;

/// Overflow never silently trims the source or lengthens picture time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundOverflowPolicy {
    Reject,
}

/// A complete source recipe plus an exact audible selection. Source phase is
/// defined by `mapping`; `offset` independently shifts it on the 48 kHz clock.
/// A qualification identity is necessary evidence, not host media admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundEvent {
    pub owner: NodeId,
    pub label: String,
    pub source: SourceAudio,
    #[serde(deserialize_with = "crate::legacy_audio_mapping_v35::sound_mapping")]
    pub mapping: SourceAudioMapping,
    pub offset: AudioSample,
    pub gain_millidecibels: i32,
    pub start_edge: AudioEdgePolicy,
    pub end_edge: AudioEdgePolicy,
    pub overflow: SoundOverflowPolicy,
}

/// A sound recipe authored in one beat owner's local output clock. Its
/// identity is `(owner, SoundId)` in `ProjectDocument::beat_sounds`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeatSound {
    pub label: String,
    pub source: SourceAudio,
    #[serde(deserialize_with = "crate::legacy_audio_mapping_v35::sound_mapping")]
    pub mapping: SourceAudioMapping,
    pub offset: AudioSample,
    pub gain_millidecibels: i32,
    pub start_edge: AudioEdgePolicy,
    pub end_edge: AudioEdgePolicy,
    pub overflow: SoundOverflowPolicy,
}

pub(crate) fn beat_sounds_map<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<NodeId, BTreeMap<crate::SoundId, BeatSound>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Inner(BTreeMap<crate::SoundId, BeatSound>);
    impl<'de> Deserialize<'de> for Inner {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            crate::document::unique_map(d).map(Self)
        }
    }
    struct Outer(PhantomData<()>);
    impl<'de> Visitor<'de> for Outer {
        type Value = BTreeMap<NodeId, BTreeMap<crate::SoundId, BeatSound>>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a map of beat owners to unique sound maps")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            let mut count = 0usize;
            while let Some((owner, events)) = map.next_entry::<NodeId, Inner>()? {
                if events.0.is_empty() {
                    return Err(serde::de::Error::custom("beat sound owner map is empty"));
                }
                count = count.saturating_add(events.0.len());
                if count > MAX_DOCUMENT_SOUNDS {
                    return Err(serde::de::Error::custom(
                        "document exceeds 64 live sound events",
                    ));
                }
                if result.insert(owner, events.0).is_some() {
                    return Err(serde::de::Error::custom("duplicate beat sound owner"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Outer(PhantomData))
}

pub(crate) fn validate(
    document: &ProjectDocument,
    durations: &BTreeMap<NodeId, FrameDuration>,
) -> Result<(), DocumentError> {
    let rate = document.presentation_basis.frame_rate;
    let beat_count: usize = document.beat_sounds.values().map(BTreeMap::len).sum();
    if document.sounds.len().saturating_add(beat_count) > MAX_DOCUMENT_SOUNDS {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "document exceeds 64 live sound events",
        ));
    }
    for (owner, events) in &document.beat_sounds {
        if events.is_empty() {
            return Err(invalid("beat sound owner map is empty"));
        }
        let extent = durations.get(owner).ok_or_else(|| {
            DocumentError::new(DocumentErrorCode::InvalidTree, "beat sound owner is absent")
        })?;
        for event in events.values() {
            validate_recipe(document, event, *extent, rate)?;
        }
    }
    for (id, event) in &document.sounds {
        if event.owner != document.root {
            return Err(invalid(
                "sound events currently require the root Sequence owner",
            ));
        }
        crate::document::validate_label(&event.label)?;
        if !(MIN_SOUND_GAIN_MILLIDECIBELS..=MAX_SOUND_GAIN_MILLIDECIBELS)
            .contains(&event.gain_millidecibels)
        {
            return Err(invalid(
                "sound gain must be between -96000 and 24000 milli dB",
            ));
        }
        let asset = document.assets.get(&event.source.asset).ok_or_else(|| {
            DocumentError::new(
                DocumentErrorCode::MissingAsset,
                "sound source asset is absent",
            )
        })?;
        if asset.source_qualification.is_none() {
            return Err(invalid("sound events require a qualified source asset"));
        }
        if !asset
            .audio
            .is_some_and(|span| span.contains_span(event.source.span))
        {
            return Err(invalid("sound selection exceeds its qualified audio span"));
        }
        let extent = document
            .sound_routes
            .get(id)
            .map_or(durations[&event.owner], |route| route.recipe_extent);
        let natural = SourceAudioMapping::natural_rate(event.source.span, rate)?;
        if event.mapping == SourceAudioMapping::FitBeat
            || event.mapping.duration_frames(extent)? != natural.duration_frames(extent)?
        {
            return Err(invalid(
                "sound events require an explicit exact natural-rate mapping",
            ));
        }
        let interval = event
            .mapping
            .selection_frames_with_offset(extent, event.offset, rate)?;
        if interval.start.compare_integer(0).is_lt()
            || interval
                .end
                .checked_sub(interval.start)?
                .compare_integer(0)
                .is_le()
            || interval.end.compare_integer(extent.frames()).is_gt()
        {
            return Err(invalid(
                "sound selected interval with offset must fit entirely inside its owner",
            ));
        }
    }
    crate::sound_routing::validate(document, durations[&document.root])?;
    Ok(())
}

fn validate_recipe(
    document: &ProjectDocument,
    event: &BeatSound,
    extent: FrameDuration,
    rate: crate::FrameRate,
) -> Result<(), DocumentError> {
    let BeatSound {
        label,
        source,
        mapping,
        offset,
        gain_millidecibels,
        ..
    } = event;
    crate::document::validate_label(label)?;
    if !(MIN_SOUND_GAIN_MILLIDECIBELS..=MAX_SOUND_GAIN_MILLIDECIBELS).contains(gain_millidecibels) {
        return Err(invalid(
            "sound gain must be between -96000 and 24000 milli dB",
        ));
    }
    let asset = document.assets.get(&source.asset).ok_or_else(|| {
        DocumentError::new(
            DocumentErrorCode::MissingAsset,
            "sound source asset is absent",
        )
    })?;
    if asset.source_qualification.is_none() {
        return Err(invalid("sound events require a qualified source asset"));
    }
    if !asset
        .audio
        .is_some_and(|span| span.contains_span(source.span))
    {
        return Err(invalid("sound selection exceeds its qualified audio span"));
    }
    let natural = SourceAudioMapping::natural_rate(source.span, rate)?;
    if *mapping == SourceAudioMapping::FitBeat
        || mapping.duration_frames(extent)? != natural.duration_frames(extent)?
    {
        return Err(invalid(
            "sound events require an explicit exact natural-rate mapping",
        ));
    }
    let interval = mapping.selection_frames_with_offset(extent, *offset, rate)?;
    if interval.start.compare_integer(0).is_lt()
        || interval
            .end
            .checked_sub(interval.start)?
            .compare_integer(0)
            .is_le()
        || interval.end.compare_integer(extent.frames()).is_gt()
    {
        return Err(invalid(
            "sound selected interval with offset must fit entirely inside its owner",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::SourceRangeInvalid, message)
}

/// Exhaustive classification keeps future commands from silently bypassing the
/// missing interval transforms, including edits hidden by occurrence isolation.
pub(crate) fn validate_command(
    document: &ProjectDocument,
    command: &Command,
) -> Result<(), EditError> {
    validate_group_ownership(document, command)?;
    if !document.beat_sounds.is_empty()
        && crate::sound_clock::edit::timing(command).is_none()
        && !preserves_sound_clocks(command)
    {
        return Err(EditError::new(
            EditErrorCode::InvalidCommand,
            "this structural edit cannot yet preserve beat-owned sound clocks and sample phase; remove beat sounds before editing time",
        ));
    }
    if document.sounds.is_empty() && document.beat_sounds.is_empty()
        || preserves_sound_clocks(command)
        || matches!(
            command,
            Command::InsertTime { .. }
                | Command::ApplySourceTrim { .. }
                | Command::TrimSource { .. }
                | Command::RollSources { .. }
                | Command::SpliceSource { .. }
                | Command::SpliceSlice { .. }
                | Command::SpliceSliceAt { .. }
                | Command::ReplaceSlice { .. }
                | Command::ReplaceSliceChildren { .. }
                | Command::SpliceSourceAt { .. }
                | Command::ReplaceSource { .. }
                | Command::ReplaceSourceChildren { .. }
                | Command::DeleteRipple { .. }
                | Command::DeleteChildren { .. }
                | Command::DeleteRange { .. }
                | Command::MoveRange { .. }
                | Command::RepeatSelection { .. }
                | Command::SetRepeatPlays { .. }
                | Command::Delete { .. }
        )
        || matches!(command, Command::Split { node, .. } if node != document.root())
    {
        return Ok(());
    }
    Err(EditError::new(
        EditErrorCode::InvalidCommand,
        "this structural edit cannot yet preserve authored sound intervals and sample phase; remove the sound events before editing time",
    ))
}

/// Neutral wrappers preserve absolute clocks, including the placement implicit
/// in a journal's current scope. They must not change or remove that scope's
/// paired subtree, split an attachment owner, or discard a group's own sounds.
fn validate_group_ownership(
    document: &ProjectDocument,
    command: &Command,
) -> Result<(), EditError> {
    let changed = match command {
        Command::Group { parent, .. } => parent,
        Command::GroupSelection {
            parent, selection, ..
        } => {
            if !document.beat_sounds.is_empty()
                && document
                    .group_selection(parent, selection)?
                    .required_split_ids
                    != 0
            {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "group endpoint splits cannot yet preserve beat-owned sound clocks",
                ));
            }
            parent
        }
        Command::Ungroup { node } => {
            if document.beat_sounds.contains_key(node) {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "ungroup cannot yet preserve sounds owned by the removed Sequence",
                ));
            }
            node
        }
        _ => return Ok(()),
    };
    let scopes: BTreeSet<_> = document
        .audio_bindings
        .sound_clocks
        .values()
        .flat_map(|events| events.values().map(|journal| journal.scope()))
        .collect();
    if scopes.is_empty() {
        return Ok(());
    }
    // Index structural parents once, including sparse override roots. Walking
    // the one changed ancestry avoids rescanning each shared journal's subtree.
    let parents: BTreeMap<_, _> = document
        .nodes()
        .keys()
        .flat_map(|parent| document.children(parent).map(move |child| (child, parent)))
        .collect();
    let mut ancestor = changed;
    for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
        if scopes.contains(ancestor) {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "group ownership changes inside a retained beat sound scope are not yet supported",
            ));
        }
        let Some(parent) = parents.get(ancestor).copied() else {
            return Ok(());
        };
        ancestor = parent;
    }
    Err(EditError::new(
        EditErrorCode::LimitExceeded,
        "group sound-scope ancestry exceeds the document depth limit",
    ))
}

fn preserves_sound_clocks(command: &Command) -> bool {
    match command {
        Command::Compound { .. } => false,
        Command::SetSound { .. }
        | Command::SetBeatSound { .. }
        | Command::DeleteBeatSound { .. }
        | Command::ReplaceSound { .. }
        | Command::DeleteSound { .. }
        | Command::SetSoundAllowance { .. }
        | Command::SlipSource { .. }
        | Command::SetSourceVideoMapping { .. }
        | Command::SetSourceAudioMapping { .. }
        | Command::SetHoldAudio { .. }
        | Command::SetHoldProvider { .. }
        | Command::SetHoldPictureContext { .. }
        | Command::AcceptGeneratedHold { .. }
        | Command::RevertGeneratedHold { .. }
        | Command::Rename { .. }
        | Command::SetAudioEdge { .. }
        | Command::SetFraming { .. }
        | Command::SetAudioTreatments { .. }
        | Command::AddAsset { .. }
        | Command::SetCanvas { .. }
        | Command::AdoptPrimaryGeometry { .. }
        | Command::SetMark { .. }
        | Command::DeleteMark { .. }
        | Command::EditScoped { .. }
        | Command::Group { .. }
        | Command::GroupSelection { .. }
        | Command::Ungroup { .. } => true,
        Command::ImportSource {
            insertion, primary, ..
        } => insertion.is_none() && primary.is_none(),
        Command::EditOccurrence { edit, .. } => match edit {
            OccurrenceEdit::SetSourceVideoMapping { .. }
            | OccurrenceEdit::SetSourceAudioMapping { .. }
            | OccurrenceEdit::SetHoldAudio { .. }
            | OccurrenceEdit::SetHoldProvider { .. }
            | OccurrenceEdit::SetHoldPictureContext { .. }
            | OccurrenceEdit::AcceptGeneratedHold { .. }
            | OccurrenceEdit::RevertGeneratedHold
            | OccurrenceEdit::Rename { .. }
            | OccurrenceEdit::SetAudioEdge { .. }
            | OccurrenceEdit::SetAudioTreatments { .. }
            | OccurrenceEdit::SetFraming { .. } => true,
            OccurrenceEdit::Split { .. }
            | OccurrenceEdit::Insert { .. }
            | OccurrenceEdit::Delete
            | OccurrenceEdit::Group { .. }
            | OccurrenceEdit::Ungroup
            | OccurrenceEdit::WrapRepeat { .. }
            | OccurrenceEdit::SetRepeat { .. }
            | OccurrenceEdit::WrapRetime { .. }
            | OccurrenceEdit::SetRetime { .. }
            | OccurrenceEdit::InsertPlays { .. }
            | OccurrenceEdit::MovePlays { .. }
            | OccurrenceEdit::SetHoldDuration { .. }
            | OccurrenceEdit::SetPlayOverride { .. }
            | OccurrenceEdit::ClearPlayOverride { .. }
            | OccurrenceEdit::SetGapOverride { .. }
            | OccurrenceEdit::IsolateGap { .. }
            | OccurrenceEdit::ClearGapOverride { .. } => false,
        },
        Command::InsertTime { .. }
        | Command::ApplySourceTrim { .. }
        | Command::TrimSource { .. }
        | Command::RollSources { .. }
        | Command::SpliceSource { .. }
        | Command::SpliceSlice { .. }
        | Command::SpliceSliceAt { .. }
        | Command::ReplaceSlice { .. }
        | Command::ReplaceSliceChildren { .. }
        | Command::SpliceSourceAt { .. }
        | Command::ReplaceSource { .. }
        | Command::ReplaceSourceChildren { .. }
        | Command::DeleteRipple { .. }
        | Command::DeleteChildren { .. }
        | Command::DeleteRange { .. }
        | Command::MoveRange { .. }
        | Command::Split { .. }
        | Command::Insert { .. }
        | Command::Delete { .. }
        | Command::Move { .. }
        | Command::WrapRepeat { .. }
        | Command::SetRepeat { .. }
        | Command::RepeatSelection { .. }
        | Command::SetRepeatPlays { .. }
        | Command::WrapRetime { .. }
        | Command::SetRetime { .. }
        | Command::InsertPlays { .. }
        | Command::MovePlays { .. }
        | Command::SetHoldDuration { .. }
        | Command::SetPlayOverride { .. }
        | Command::ClearPlayOverride { .. }
        | Command::SetGapOverride { .. }
        | Command::IsolateGap { .. }
        | Command::ClearGapOverride { .. } => false,
    }
}

#[cfg(test)]
mod beat_sound_tests {
    use crate::*;
    use std::collections::BTreeMap;

    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }
    fn fixture() -> (ProjectDocument, BeatSound) {
        let rate = FrameRate::new(30, 1).unwrap();
        let root = id("root");
        let owner = id("owner");
        let mut doc = ProjectDocument::new(
            ProjectId::new("beat-sounds").unwrap(),
            RevisionId::new("base").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: rate,
                color_policy: ColorPolicy::SdrRec709,
            },
            root.clone(),
        )
        .unwrap();
        doc.nodes.insert(
            root.clone(),
            BeatNode::sequence("Root", vec![owner.clone()]),
        );
        doc.nodes.insert(
            owner.clone(),
            BeatNode::hold(
                "Owner",
                HoldRecipe {
                    duration: FrameDuration::new(30).unwrap(),
                    video: HoldVideo::Background,
                    picture_context: None,
                    audio: HoldAudio::Silence,
                },
            ),
        );
        let time_base = SourceTimeBase::new(1, 48_000).unwrap();
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 48_000,
                time_base,
            },
        )
        .unwrap();
        doc.assets.insert(
            AssetId::new("sound").unwrap(),
            AssetRecord {
                label: "Sound".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(span),
                still_image: false,
                frame_count: None,
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
            },
        );
        let event = BeatSound {
            label: "Beat effect".into(),
            source: SourceAudio {
                asset: AssetId::new("sound").unwrap(),
                span,
            },
            mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
            offset: AudioSample(0),
            gain_millidecibels: -1200,
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Automatic,
            overflow: SoundOverflowPolicy::Reject,
        };
        doc.validate().unwrap();
        (doc, event)
    }

    fn request(doc: &ProjectDocument, revision: &str, command: Command) -> CommandRequest {
        CommandRequest {
            project_id: doc.project_id().clone(),
            expected_revision: doc.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command,
        }
    }

    #[test]
    fn beat_sound_set_roundtrips_and_patch_inverse_restores_exact_recipe() {
        let (doc, event) = fixture();
        let owner = id("owner");
        let sound = SoundId::new("effect").unwrap();
        let tx = crate::command::apply(
            &doc,
            &request(
                &doc,
                "set",
                Command::SetBeatSound {
                    owner: owner.clone(),
                    id: sound.clone(),
                    event: event.clone(),
                },
            ),
        )
        .unwrap();
        let saved = tx.forward.apply(&doc).unwrap();
        assert_eq!(saved.beat_sounds()[&owner][&sound], event);
        assert_eq!(
            ProjectDocument::from_json(&saved.to_json().unwrap()).unwrap(),
            saved
        );
        assert_eq!(tx.inverse.apply(&saved).unwrap(), doc);
    }

    #[test]
    fn owner_copy_preserves_local_id_and_independent_source_recipe() {
        let (mut doc, event) = fixture();
        let owner = id("owner");
        let copy = id("copy");
        let sound = SoundId::new("effect").unwrap();
        doc.beat_sounds.insert(
            owner.clone(),
            BTreeMap::from([(sound.clone(), event.clone())]),
        );
        let allocation = RevisionId::new("copy-allocation").unwrap();
        crate::occurrence_edit::clone_nodes(
            &mut doc,
            &BTreeMap::from([(owner.clone(), copy.clone())]),
            &allocation,
        )
        .unwrap();
        assert_eq!(doc.beat_sounds[&owner][&sound], event);
        assert_eq!(doc.beat_sounds[&copy][&sound], event);
        assert_eq!(doc.beat_sounds.len(), 2);
    }

    #[test]
    fn malformed_source_is_rejected_and_timing_edit_is_atomic() {
        let (doc, mut event) = fixture();
        let owner = id("owner");
        event.source.asset = AssetId::new("missing").unwrap();
        let rejected = crate::command::apply(
            &doc,
            &request(
                &doc,
                "bad",
                Command::SetBeatSound {
                    owner: owner.clone(),
                    id: SoundId::new("effect").unwrap(),
                    event,
                },
            ),
        );
        assert!(rejected.is_err());
        assert!(doc.beat_sounds().is_empty());

        let (doc, mut overflowing) = fixture();
        overflowing.offset = AudioSample(i64::MAX);
        let rejected = crate::command::apply(
            &doc,
            &request(
                &doc,
                "overflow",
                Command::SetBeatSound {
                    owner: owner.clone(),
                    id: SoundId::new("overflow").unwrap(),
                    event: overflowing,
                },
            ),
        );
        assert!(rejected.is_err());

        let (doc, event) = fixture();
        let set = crate::command::apply(
            &doc,
            &request(
                &doc,
                "set",
                Command::SetBeatSound {
                    owner: owner.clone(),
                    id: SoundId::new("effect").unwrap(),
                    event,
                },
            ),
        )
        .unwrap();
        let with_sound = set.forward.apply(&doc).unwrap();
        let attempted = crate::command::apply(
            &with_sound,
            &request(&with_sound, "delete-owner", Command::Delete { node: owner }),
        );
        assert!(attempted.is_err());
        assert_eq!(with_sound.beat_sounds().len(), 1);
    }

    #[test]
    fn beat_sound_guard_does_not_change_root_only_ripple_admission() {
        let (mut doc, event) = fixture();
        let root = id("root");
        doc.sounds.insert(
            SoundId::new("root-effect").unwrap(),
            SoundEvent {
                owner: root.clone(),
                label: event.label,
                source: event.source,
                mapping: event.mapping,
                offset: event.offset,
                gain_millidecibels: event.gain_millidecibels,
                start_edge: event.start_edge,
                end_edge: event.end_edge,
                overflow: event.overflow,
            },
        );
        let command = Command::DeleteRipple {
            node: id("owner"),
            timing: AudioTimingId {
                allocation: RevisionId::new("delete").unwrap(),
                ordinal: 0,
            },
        };
        assert!(doc.beat_sounds().is_empty());
        assert!(super::validate_command(&doc, &command).is_ok());
    }

    #[test]
    fn empty_owner_and_duplicate_sound_maps_reject_before_authoring() {
        let (mut doc, event) = fixture();
        doc.beat_sounds.insert(id("root"), BTreeMap::new());
        assert!(doc.validate().is_err());
        assert!(ProjectDocument::from_json(&serde_json::to_string(&doc).unwrap()).is_err());
        let event = serde_json::to_string(&event).unwrap();
        for map in [
            format!("{{\"owner\":{{\"effect\":{event},\"effect\":{event}}}}}"),
            format!("{{\"owner\":{{\"effect\":{event}}},\"owner\":{{\"other\":{event}}}}}"),
        ] {
            let mut de = serde_json::Deserializer::from_str(&map);
            assert!(super::beat_sounds_map(&mut de).is_err());
        }
    }
}
