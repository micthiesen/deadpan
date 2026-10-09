use super::*;
use deadpan_core::{
    AudioBindingState, AudioEdgePolicy, AudioSample, AudioTimingRecord, BeatNode, BeatSound,
    ColorPolicy, ExactRatio, FrozenAudioLayout, HoldAudio, HoldRecipe, HoldVideo,
    PresentationBasis, ProjectId, SoundClockJournal, SoundClockReference, SoundId,
    SoundOverflowPolicy, SourceAudio, SourceAudioMapping, SourceTimeBase,
};
use deadpan_media::audio_index::{
    AudioChannelLayout, AudioFrameObservation, AudioIndexSnapshot, AudioStreamDescriptor,
};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_qualification::{
    SOURCE_IMPORT_TIMING_POLICY_VERSION, SOURCE_QUALIFICATION_DECODER_CONTRACT,
    SOURCE_QUALIFICATION_VERSION,
};

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn sound() -> SoundId {
    SoundId::new("overlay").unwrap()
}

fn timing(ordinal: u32) -> AudioTimingId {
    AudioTimingId {
        allocation: RevisionId::new("capture").unwrap(),
        ordinal,
    }
}

// This boundary reads retained evidence. Synthetic, validated metadata is enough
// to exercise it; these fixtures never grant decoded source registration.
fn retain_receipt(connection: &Connection, seed: u8) -> SourceQualificationReceipt {
    let content = SourceContentIdentity::new([seed; 32], 4096).unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let audio = AudioIndexSnapshot::new(
        content,
        AudioStreamDescriptor {
            matroska_opus: None,
            mp3: None,
            stream_index: 0,
            codec: "pcm_s16le".into(),
            time_base,
            sample_rate: 48_000,
            channel_layout: AudioChannelLayout::Native {
                channels: 2,
                mask: 3,
            },
            stream_start: Some(0),
            stream_duration: Some(1024),
            initial_padding: 0,
            trailing_padding: 0,
            seek_preroll: 0,
        },
        vec![AudioFrameObservation {
            pts: 0,
            discard: false,
            decode_timestamp: Some(0),
            reported_duration: Some(1024),
            sample_count: 1024,
            sample_format: "s16".into(),
            skip_samples: None,
        }],
    )
    .unwrap();
    let snapshot = SourceQualificationSnapshot::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": SOURCE_QUALIFICATION_VERSION,
            "decoder_contract": SOURCE_QUALIFICATION_DECODER_CONTRACT,
            "timing_policy_version": SOURCE_IMPORT_TIMING_POLICY_VERSION,
            "content": content,
            "origin_seconds": ExactRatio::ZERO,
            "video": null,
            "audio": audio,
        }))
        .unwrap(),
    )
    .unwrap();
    let original = OriginalObjectRef::new(
        OriginalContentId::new(format!("{seed:02x}").repeat(32)).unwrap(),
        content.byte_length(),
    )
    .unwrap();
    let record = serde_json::json!({
        "object": original,
        "sha256": content.sha256(),
        "label": "Retained sound",
        "version": 1,
        "managed": true,
        "linked": null,
    });
    connection
        .execute(
            "INSERT INTO original_media(content_id,version,record) VALUES(?1,1,?2)",
            params![original.content().to_string(), record.to_string()],
        )
        .unwrap();
    let bytes = snapshot.to_json().unwrap();
    let receipt = SourceQualificationReceipt {
        id: receipt_id(&original, &bytes).unwrap(),
        original,
        snapshot,
    };
    write_receipt(connection, &receipt, &bytes).unwrap();
    receipt
}

fn fixture() -> (Connection, ProjectDocument, SourceQualificationReceipt) {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .pragma_update(None, "foreign_keys", false)
        .unwrap();
    crate::original_media::create_tables(&connection).unwrap();
    create_tables(&connection).unwrap();
    let receipts = [
        retain_receipt(&connection, 1),
        retain_receipt(&connection, 2),
    ];
    let document = ProjectDocument::new(
        ProjectId::new("sound-clock-admission").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let hold = || {
        BeatNode::hold(
            "Owner",
            HoldRecipe {
                duration: FrameDuration::new(4).unwrap(),
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            },
        )
    };
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("left"), node("right")]),
        ),
        (node("left"), hold()),
        (node("right"), hold()),
    ]))
    .unwrap();
    let mut assets = BTreeMap::new();
    let mut sounds = BTreeMap::new();
    for (owner, receipt) in ["left", "right"].into_iter().zip(&receipts) {
        let asset = AssetId::new(owner).unwrap();
        let record = receipt.asset_record(owner.into()).unwrap();
        let span = record.audio.unwrap();
        sounds.insert(
            node(owner),
            BTreeMap::from([(
                sound(),
                BeatSound {
                    label: owner.into(),
                    source: SourceAudio {
                        asset: asset.clone(),
                        span,
                    },
                    mapping: SourceAudioMapping::natural_rate(
                        span,
                        document.presentation_basis().frame_rate,
                    )
                    .unwrap(),
                    offset: AudioSample(7),
                    gain_millidecibels: -3000,
                    start_edge: AudioEdgePolicy::Automatic,
                    end_edge: AudioEdgePolicy::Hard,
                    overflow: SoundOverflowPolicy::Reject,
                },
            )]),
        );
        assets.insert(asset, record);
    }
    wire["assets"] = serde_json::to_value(assets).unwrap();
    wire["beat_sounds"] = serde_json::to_value(sounds).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    (connection, document, receipts[0].clone())
}

fn with_clocks(document: &ProjectDocument, owner: &str, ordinals: &[u32]) -> ProjectDocument {
    let clocks: Vec<_> = ordinals.iter().map(|ordinal| timing(*ordinal)).collect();
    let records = clocks
        .iter()
        .map(|id| AudioTimingRecord {
            id: id.clone(),
            layout: FrozenAudioLayout::capture(document).unwrap(),
        })
        .collect();
    let scope = node(owner);
    let journals = if clocks.is_empty() {
        BTreeMap::new()
    } else {
        let references = clocks
            .iter()
            .map(|id| SoundClockReference::new(id.clone(), scope.clone(), node(owner)))
            .collect();
        BTreeMap::from([(
            scope.clone(),
            BTreeMap::from([(sound(), SoundClockJournal::new(scope, references).unwrap())]),
        )])
    };
    let bindings = AudioBindingState::new_with_sound_clocks(
        records,
        BTreeMap::new(),
        BTreeMap::new(),
        journals,
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn unchanged_beat_sound_clock_transitions_recheck_exact_receipt_and_original() {
    for corruption in [
        "missing-receipt",
        "changed-receipt",
        "missing-original",
        "changed-original",
    ] {
        let (connection, before, receipt) = fixture();
        let captured = with_clocks(&before, "left", &[0]);
        let appended = with_clocks(&before, "left", &[0, 1]);
        let transitions = [
            (&before, &captured),
            (&captured, &appended),
            (&captured, &before),
        ];
        for (current, next) in transitions {
            assert_eq!(current.beat_sounds(), next.beat_sounds());
            assert_eq!(current.assets(), next.assets());
            validate_sound_sources(&connection, current, next, None).unwrap();
        }
        match corruption {
            "missing-receipt" => {
                connection
                    .execute(
                        "DELETE FROM source_qualifications WHERE id=?1",
                        [receipt.id.as_str()],
                    )
                    .unwrap();
            }
            "changed-receipt" => {
                connection
                    .execute(
                        "UPDATE source_qualifications SET snapshot=X'00' WHERE id=?1",
                        [receipt.id.as_str()],
                    )
                    .unwrap();
            }
            "missing-original" => {
                connection
                    .execute(
                        "DELETE FROM original_media WHERE content_id=?1",
                        [receipt.original.content().to_string()],
                    )
                    .unwrap();
            }
            "changed-original" => {
                let record =
                    crate::original_media::read_record(&connection, receipt.original.content())
                        .unwrap()
                        .unwrap();
                let mut wire = serde_json::to_value(record).unwrap();
                wire["sha256"] = serde_json::to_value([9_u8; 32]).unwrap();
                connection
                    .execute(
                        "UPDATE original_media SET record=?1 WHERE content_id=?2",
                        params![wire.to_string(), receipt.original.content().to_string()],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        // An unchanged address does not become a fresh admission request merely
        // because another sound with the same local ID acquires a clock.
        let other = with_clocks(&before, "right", &[0]);
        validate_sound_sources(&connection, &before, &other, None).unwrap();
        for (current, next) in transitions {
            let error = validate_sound_sources(&connection, current, next, None).unwrap_err();
            assert!(
                matches!(error, StoreError::SourceRegistration(_)),
                "{corruption}: {error}"
            );
        }
    }
}
