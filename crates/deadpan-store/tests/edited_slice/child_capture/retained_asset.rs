//! A zero-time Source mark still requires real historical registration evidence.
use super::*;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::SourceRegistration;
use std::{path::PathBuf, time::Duration};

// Same bounded verified-input/session path as tests/source_registration.rs.
fn register_camera(store: &mut ProjectStore) -> Result {
    let active = AtomicBool::new(false);
    let limits = OriginalMediaLimits::new(2_000_000, Duration::from_secs(10))?;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()?;
    let original = store
        .retain_original(&path, OriginalOwnership::Managed, limits, &active)?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits, &active)?;
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &active,
    )?;
    let video = SourceSession::open_input(
        input.clone(),
        AssetId::new("decode-alias")?,
        SourceSessionLimits::default(),
        &active,
    )?;
    let audio = AudioSession::open_input(input, 1, AudioSessionLimits::default(), &active)?;
    let qualified = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio))?;
    store.register_source(
        &SourceRegistration {
            expected_revision: store.snapshot()?.revision_id().clone(),
            new_revision: revision("registered-camera"),
            original: original.object().content().clone(),
            new_asset_id: AssetId::new("camera")?,
            label: "Measured source".into(),
            insertion: None,
        },
        &qualified,
        None,
        limits,
        &active,
    )?;
    Ok(())
}

#[test]
fn zero_paste_preserves_routed_root_bus_and_restores_only_mark_owned_historical_media() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("empty-with-media.deadpan");
    let mut wire = serde_json::to_value(structural_document(true)?)?;
    wire["marks"]["source-address"] = serde_json::to_value(Mark {
        owner: node("empty-mid"),
        label: "Dormant source address".into(),
        boundary: BoundaryAnchor {
            coordinate: Anchor::Source {
                asset: AssetId::new("camera")?,
                moment: SourceMoment::Timestamp {
                    stream: SourceStream::Video,
                    timestamp: SourceTimestamp {
                        ticks: 0,
                        time_base: SourceTimeBase::new(1, 30_000)?,
                    },
                },
            },
            bias: InsertionBias::Right,
        },
        loss_policy: AnchorLossPolicy::KeepUnresolved,
        state: MarkState::Unresolved {
            reason: MarkLossReason::SourceUnavailable,
        },
        fragments: vec![],
    })?;
    let baseline = ProjectDocument::from_json(&wire.to_string())?;
    let mut store = ProjectStore::create(&path, &baseline)?;
    register_camera(&mut store)?;
    let registered = store.snapshot()?;
    let asset = AssetId::new("camera")?;
    let record = registered.assets()[&asset].clone();
    let span = record.audio.unwrap();
    let natural =
        SourceAudioMapping::natural_rate(span, registered.presentation_basis().frame_rate)?;
    store.commit(&request(
        &registered,
        "root-sound",
        Command::SetSound {
            id: SoundId::new("effect")?,
            event: SoundEvent {
                owner: node("root"),
                label: "Root sound".into(),
                source: SourceAudio {
                    asset: asset.clone(),
                    span,
                },
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::ZERO,
                    frames: natural.duration_frames(FrameDuration::ZERO)?,
                    selection: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(20))?,
                },
                offset: AudioSample(0),
                gain_millidecibels: -3000,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    ))?;
    let sounded = store.snapshot()?;
    store.commit(&request(
        &sounded,
        "allow-lead",
        Command::SetSoundAllowance {
            sound: SoundId::new("effect")?,
            issuer: SoundHoldIssuer::Node {
                instance: InstancePath {
                    node: node("lead"),
                    repeats: vec![],
                },
            },
            allowed: true,
        },
    ))?;
    let allowed = store.snapshot()?;
    let range = FrameRange::new(ProjectFrame(0), ProjectFrame(1))?;
    let count = allowed.range_deletion(allowed.root(), range)?.required_ids;
    store.commit(&request(
        &allowed,
        "route-existing-sound",
        Command::DeleteRange {
            parent: allowed.root().clone(),
            range,
            identities: SplitIdentities {
                nodes: (0..count).map(|i| node(&format!("cut-{i}"))).collect(),
            },
            timing: timing("route-existing-sound"),
        },
    ))?;
    let before = store.snapshot()?;
    assert!(!before.sounds().is_empty());
    assert!(!before.sound_routes().is_empty());
    assert!(!before.sound_allowances().is_empty());
    let slice = capture_child(&before, "root", "empty-mid")?;
    assert_eq!(slice.identity_requirements()?.marks, 5);
    assert_eq!(slice.identity_requirements()?.timings, 0);
    assert_eq!(
        serde_json::to_value(&slice)?["assets"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    let command = seam(&before, &slice, "empty-keeps-bus", 3)?;
    let cells = authored(&path)?;
    let view = store.preview_edit_slice(&command)?;
    assert_eq!(authored(&path)?, cells);
    store.commit(&command)?;
    let after = store.snapshot()?;
    assert_eq!(after, **view.document());
    assert_eq!(after.audio_bindings(), before.audio_bindings());
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sound_routes(), before.sound_routes());
    assert_eq!(after.sound_allowances(), before.sound_allowances());
    for (id, mark) in before.marks() {
        assert_eq!(&after.marks()[id], mark);
    }
    for (id, lineage) in before.audio_lineage() {
        assert_eq!(&after.audio_lineage()[id], lineage);
    }
    // Undo the zero paste, prior deletion, allowance, sound and registration.
    for next in [
        "undo-zero",
        "undo-route",
        "undo-allowance",
        "undo-sound",
        "undo-registration",
    ] {
        let current = store.snapshot()?;
        store.undo(current.revision_id(), revision(next))?;
    }
    let without_media = store.snapshot()?;
    assert_authored(&without_media, &baseline)?;
    assert!(without_media.assets().is_empty());
    drop(store);
    assert!(view.check_live(&AtomicBool::new(false)).is_err());
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let cells = authored(&path)?;
    let source = store.view_edit_slice(&slice, view_ids(&slice, "historical-empty-source")?)?;
    assert_eq!(source.document().duration()?, FrameDuration::ZERO);
    assert_eq!(source.sources().len(), 1);
    assert_eq!(
        source.sources()[&asset]
            .receipt
            .asset_record(record.label.clone())?,
        record
    );
    assert_eq!(source.document().assets()[&asset], record);
    assert!(source.document().sounds().is_empty());
    assert!(source.document().audio_bindings().is_empty());
    assert_eq!(authored(&path)?, cells);
    let mut forged = serde_json::to_value(&slice)?;
    forged["assets"]["camera"]["source_qualification"] = serde_json::json!("f".repeat(64));
    let forged = CapturedEditSlice::from_json(&forged.to_string())?;
    let command = seam(&without_media, &forged, "forged-media", 3)?;
    apply(&without_media, &command)?;
    assert!(store.preview(&command).is_err());
    assert!(store.preview_edit_slice(&command).is_err());
    assert!(
        store
            .view_edit_slice(&forged, view_ids(&forged, "forged-source")?)
            .is_err()
    );
    assert!(store.commit(&command).is_err());
    assert_eq!(authored(&path)?, cells);
    let restored = seam(&without_media, &slice, "restored-media", 3)?;
    store.commit(&restored)?;
    let final_document = store.snapshot()?;
    assert_eq!(final_document.duration()?, baseline.duration()?);
    assert_eq!(final_document.assets()[&asset], record);
    assert_eq!(final_document.assets().len(), 1);
    assert!(final_document.sounds().is_empty());
    store.validate()?;
    drop(store);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(
        reader
            .registered_source(final_document.revision_id(), &asset)?
            .id(),
        record.source_qualification.as_ref().unwrap()
    );
    reader.validate()?;
    Ok(())
}
