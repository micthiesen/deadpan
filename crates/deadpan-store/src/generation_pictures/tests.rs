use super::*;
use deadpan_core::{
    AssetId, EndpointPolicy, ExactRatio, GeneratedObjectRef, IndexedSourceFrame, SourceFrameId,
    SourceFrameIndex, SourcePoint, SourceSpan, SourceTimeBase, SourceTimestamp, TerminalProvenance,
};
use rusqlite::params;
use serde_json::json;

#[path = "tests/support.rs"]
mod support;

fn clock() -> SourceTimeBase {
    SourceTimeBase::new(1, 100).unwrap()
}
fn point(ticks: i64) -> SourcePoint {
    SourcePoint {
        ticks: ExactRatio::integer(ticks),
        time_base: clock(),
    }
}
fn span(start: i64, end: i64) -> SourceSpan {
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: clock(),
        },
        SourceTimestamp {
            ticks: end,
            time_base: clock(),
        },
    )
    .unwrap()
}
fn index() -> SourceFrameIndex {
    SourceFrameIndex::new(
        AssetId::new("receipt-alias").unwrap(),
        clock(),
        [0, 3, 11, 20]
            .into_iter()
            .enumerate()
            .map(|(ordinal, pts)| IndexedSourceFrame {
                identity: SourceFrameId(ordinal as u64),
                pts,
                reported_duration: None,
                keyframe: true,
                seek_from: None,
                decode_timestamp: None,
            })
            .collect(),
        24,
        TerminalProvenance::Explicit,
    )
    .unwrap()
}
fn source(ticks: i64, end: i64) -> Picture {
    Picture::Source {
        asset: AssetId::new("authored-alias").unwrap(),
        point: point(ticks),
        span: span(0, end),
        selection: span(0, end).into(),
        endpoints: EndpointPolicy::HoldAdjacent,
    }
}
fn identity(picture: &Picture) -> GenerationPictureIdentity {
    original_identity(
        &SourceQualificationId::new("a".repeat(64)).unwrap(),
        &index(),
        picture,
    )
    .unwrap()
}

#[test]
fn measured_vfr_identity_ignores_remote_span_changes_and_source_aliases() {
    assert_eq!(identity(&source(9, 20)), identity(&source(9, 24)));
    assert_eq!(
        identity(&source(9, 24)),
        identity(&Picture::Freeze {
            asset: AssetId::new("another-alias").unwrap(),
            point: point(4),
        })
    );
    assert!(matches!(
        identity(&source(9, 24)),
        GenerationPictureIdentity::Original {
            frame: SourceFrameId(1),
            ..
        }
    ));
    assert_ne!(identity(&source(9, 24)), identity(&source(11, 24)));
}

#[test]
fn measured_selection_policy_is_applied_before_identity_comparison() {
    let mut held = source(22, 24);
    if let Picture::Source { selection, .. } = &mut held {
        *selection = span(0, 11).into();
    }
    assert_eq!(identity(&held), identity(&source(9, 24)));
    if let Picture::Source { endpoints, .. } = &mut held {
        *endpoints = EndpointPolicy::Reject;
    }
    assert!(
        original_identity(
            &SourceQualificationId::new("a".repeat(64)).unwrap(),
            &index(),
            &held
        )
        .is_err()
    );
}

#[test]
fn source_identity_refuses_missing_terminal_support_or_wrong_clock() {
    let qualification = SourceQualificationId::new("a".repeat(64)).unwrap();
    assert!(original_identity(&qualification, &index(), &source(9, 25)).is_err());
    let mut wrong = source(9, 24);
    if let Picture::Source { point, .. } = &mut wrong {
        point.time_base = SourceTimeBase::new(1, 30).unwrap();
    }
    assert!(original_identity(&qualification, &index(), &wrong).is_err());
}

fn connection() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    crate::original_media::create_tables(&connection).unwrap();
    crate::source_registration::create_tables(&connection).unwrap();
    connection
}

fn empty_document() -> ProjectDocument {
    use deadpan_core::{ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectId, RevisionId};
    ProjectDocument::new(
        ProjectId::new("picture-cache").unwrap(),
        RevisionId::new("before").unwrap(),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap()
}

// Valid persisted metadata exercises the production receipt reader without
// opening media or granting a decoded-source admission token. The audio index
// is deliberately much larger than the picture index.
fn retain_receipt(connection: &Connection) -> (SourceQualificationId, Vec<u8>) {
    use crate::original_media::{OriginalContentId, OriginalObjectRef};
    use deadpan_media::audio_index::{
        AudioChannelLayout, AudioFrameObservation, AudioIndexSnapshot, AudioStreamDescriptor,
    };
    use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
    use deadpan_media::source_qualification::{
        QUALIFIED_SOURCE_ASSET_ID, SOURCE_IMPORT_TIMING_POLICY_VERSION,
        SOURCE_QUALIFICATION_DECODER_CONTRACT, SOURCE_QUALIFICATION_VERSION,
        SourceQualificationSnapshot,
    };
    let content = SourceContentIdentity::new([7; 32], 4096).unwrap();
    let mut measured_frames = index().frames().to_vec();
    measured_frames.last_mut().unwrap().reported_duration = Some(4);
    let video = SourceIndexSnapshot::new(
        content,
        0,
        SourceFrameIndex::new(
            AssetId::new(QUALIFIED_SOURCE_ASSET_ID).unwrap(),
            clock(),
            measured_frames,
            24,
            TerminalProvenance::DecodedFrameDuration,
        )
        .unwrap(),
    )
    .unwrap();
    let count = 4096_i64;
    let audio = AudioIndexSnapshot::new(
        content,
        AudioStreamDescriptor {
            matroska_opus: None,
            stream_index: 1,
            codec: "aac".into(),
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            sample_rate: 48_000,
            channel_layout: AudioChannelLayout::Native {
                channels: 2,
                mask: 3,
            },
            stream_start: Some(0),
            stream_duration: Some(count * 1024),
            initial_padding: 0,
            trailing_padding: 0,
            seek_preroll: 0,
        },
        (0..count)
            .map(|ordinal| AudioFrameObservation {
                pts: ordinal * 1024,
                discard: false,
                decode_timestamp: None,
                reported_duration: Some(1024),
                sample_count: 1024,
                sample_format: "fltp".into(),
                skip_samples: None,
            })
            .collect(),
    )
    .unwrap();
    let snapshot = SourceQualificationSnapshot::from_json(&serde_json::to_vec(&json!({
        "schema_version": SOURCE_QUALIFICATION_VERSION,
        "decoder_contract": SOURCE_QUALIFICATION_DECODER_CONTRACT,
        "timing_policy_version": SOURCE_IMPORT_TIMING_POLICY_VERSION,
        "content": content, "origin_seconds": ExactRatio::ZERO,
        "video": {"index": video, "interpretation": {
            "width": 640, "height": 480, "stream_index": 0, "time_base_num": 1, "time_base_den": 100,
            "sample_aspect_num": 1, "sample_aspect_den": 1, "rotation_quarter_turns": 0,
            "color": {"range": "full", "matrix": "rgb", "transfer": "srgb", "primaries": "bt709"},
            "codec": "h264", "pixel_format": "gbrp", "stream_start": 0, "stream_duration": 24,
            "container_start": null, "container_duration": null,
            "audio_streams": [{"stream_index": 1, "codec": "aac", "time_base_num": 1, "time_base_den": 48000,
                "stream_start": 0, "stream_duration": count * 1024, "sample_rate": 48000, "channel_count": 2}]
        }}, "audio": audio,
    })).unwrap()).unwrap();
    let bytes = snapshot.to_json().unwrap();
    let original =
        OriginalObjectRef::new(OriginalContentId::new("a".repeat(64)).unwrap(), 4096).unwrap();
    let mut hash = blake3::Hasher::new();
    hash.update(b"deadpan-source-qualification-v1\0");
    hash.update(original.content().digest().as_bytes());
    hash.update(&original.byte_length().to_be_bytes());
    hash.update(&(bytes.len() as u64).to_be_bytes());
    hash.update(&bytes);
    let id = SourceQualificationId::new(hash.finalize().to_hex().to_string()).unwrap();
    let record = json!({"object": original, "sha256": content.sha256(), "label": "A/V receipt",
        "version": 1, "managed": true, "linked": null});
    connection
        .execute(
            "INSERT INTO original_media(content_id,version,record) VALUES(?1,1,?2)",
            params![original.content().to_string(), record.to_string()],
        )
        .unwrap();
    connection.execute("INSERT INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
        params![id.as_str(), original.content().to_string(), serde_json::to_string(&original).unwrap(), bytes]).unwrap();
    (id, bytes)
}

#[test]
fn repeated_aliases_share_one_admitted_receipt_and_one_asset_contract() {
    let connection = connection();
    let (id, bytes) = retain_receipt(&connection);
    let receipt = crate::source_registration::read_receipt(&connection, &id)
        .unwrap()
        .unwrap();
    let mut wire = serde_json::to_value(empty_document()).unwrap();
    wire["assets"] = json!({
        "authored-alias": receipt.asset_record("First label".into()).unwrap(),
        "other-alias": receipt.asset_record("Different editorial label".into()).unwrap(),
    });
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let pictures = QualifiedGenerationPictures::new(&connection);
    let expected = GenerationPictureIdentity::Original {
        qualification: id.clone(),
        frame: SourceFrameId(1),
    };
    for _ in 0..128 {
        assert_eq!(
            pictures.identity(&document, &source(9, 24)).unwrap(),
            expected
        );
        assert_eq!(
            pictures
                .identity(
                    &document,
                    &Picture::Freeze {
                        asset: AssetId::new("other-alias").unwrap(),
                        point: point(4),
                    }
                )
                .unwrap(),
            expected
        );
    }
    let cache = pictures.receipts.borrow();
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(
        cache.contract_builds, 1,
        "audio timing must not be rescanned per endpoint"
    );
    assert_eq!(cache.bytes, bytes.len());
    assert_eq!(cache.frames, 4);
    drop(cache);
    // An alias with changed measured metadata still refuses after a cache hit.
    wire["assets"]["other-alias"]["content_hash"] = json!("b".repeat(64));
    let changed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(
        pictures
            .identity(
                &changed,
                &Picture::Freeze {
                    asset: AssetId::new("other-alias").unwrap(),
                    point: point(4),
                }
            )
            .unwrap_err()
            .to_string()
            .contains("disagrees with its source qualification")
    );
}

#[test]
fn failed_receipt_is_not_loaded_again_until_a_new_batch() {
    let connection = connection();
    let (id, bytes) = retain_receipt(&connection);
    connection
        .execute(
            "UPDATE source_qualifications SET snapshot=?1 WHERE id=?2",
            params![b"invalid".as_slice(), id.as_str()],
        )
        .unwrap();
    let mut cache = ReceiptCache::default();
    let first = cache.get(&connection, &id).err().unwrap().to_string();
    assert!(first.contains("qualification identity disagrees"));
    assert_eq!(cache.bytes, 7);
    connection
        .execute(
            "UPDATE source_qualifications SET snapshot=?1 WHERE id=?2",
            params![bytes, id.as_str()],
        )
        .unwrap();
    assert_eq!(
        cache.get(&connection, &id).err().unwrap().to_string(),
        first
    );
    assert_eq!(cache.bytes, 7);
    assert_eq!(cache.contract_builds, 0);
    assert!(ReceiptCache::default().get(&connection, &id).is_ok());
}

#[test]
fn byte_budget_refuses_before_decoding_and_stops_later_cold_loads() {
    let connection = connection();
    let (id, _) = retain_receipt(&connection);
    connection
        .execute(
            "UPDATE source_qualifications SET snapshot=?1 WHERE id=?2",
            params![b"invalid".as_slice(), id.as_str()],
        )
        .unwrap();
    let mut cache = ReceiptCache {
        bytes: MAX_RECEIPT_BYTES - 6,
        ..ReceiptCache::default()
    };
    let error = cache.get(&connection, &id).err().unwrap().to_string();
    assert!(
        error.contains("aggregate byte bound"),
        "refuse before the bad receipt is decoded: {error}"
    );
    assert_eq!(cache.bytes, MAX_RECEIPT_BYTES - 6);
    assert_eq!(cache.contract_builds, 0);
    connection
        .execute("DROP TABLE source_qualifications", [])
        .unwrap();
    let fresh = SourceQualificationId::new("b".repeat(64)).unwrap();
    assert!(
        cache
            .get(&connection, &fresh)
            .err()
            .unwrap()
            .to_string()
            .contains("aggregate byte bound")
    );
    assert_eq!(
        cache.entries.len(),
        0,
        "aggregate exhaustion is separate from per-receipt evidence failures"
    );
}

#[test]
fn missing_receipts_count_toward_attempt_bound_and_never_repeat_queries() {
    let connection = connection();
    let mut cache = ReceiptCache::default();
    let missing = SourceQualificationId::new("a".repeat(64)).unwrap();
    let first = cache.get(&connection, &missing).err().unwrap().to_string();
    assert!(first.contains("qualification is missing"));
    for i in 1..MAX_RECEIPTS {
        let id = SourceQualificationId::new(format!("{i:064x}")).unwrap();
        assert!(cache.get(&connection, &id).is_err());
    }
    assert_eq!(cache.entries.len(), MAX_RECEIPTS);
    connection
        .execute("DROP TABLE source_qualifications", [])
        .unwrap();
    assert_eq!(
        cache.get(&connection, &missing).err().unwrap().to_string(),
        first
    );
    let fresh = SourceQualificationId::new("b".repeat(64)).unwrap();
    assert!(
        cache
            .get(&connection, &fresh)
            .err()
            .unwrap()
            .to_string()
            .contains("receipt count")
    );
    assert_eq!(cache.entries.len(), MAX_RECEIPTS);
    assert_eq!(cache.bytes, 0);
}

#[test]
fn frame_budget_halts_new_loads_but_preserves_already_cached_receipts() {
    let connection = connection();
    let (id, _) = retain_receipt(&connection);
    let mut too_many = ReceiptCache {
        frames: deadpan_core::MAX_SOURCE_INDEX_FRAMES - 3,
        ..ReceiptCache::default()
    };
    assert!(
        too_many
            .get(&connection, &id)
            .err()
            .unwrap()
            .to_string()
            .contains("aggregate frame bound")
    );
    assert_eq!(too_many.contract_builds, 0);
    let mut cache = ReceiptCache::default();
    assert!(cache.get(&connection, &id).is_ok());
    cache.bytes = MAX_RECEIPT_BYTES;
    connection
        .execute("DROP TABLE source_qualifications", [])
        .unwrap();
    let fresh = SourceQualificationId::new("b".repeat(64)).unwrap();
    assert!(
        cache
            .get(&connection, &fresh)
            .err()
            .unwrap()
            .to_string()
            .contains("aggregate byte bound")
    );
    assert!(cache.get(&connection, &id).is_ok());
    assert_eq!(cache.contract_builds, 1);
}

fn generated(aspect: Option<[u32; 2]>, frame: u64) -> Picture {
    use deadpan_core::{
        BridgeInterpolation, BridgeSamplingMap, FrameDuration, FrameRate, GeneratedArtifact,
        GeneratedContentId,
    };
    let object = |digit: char| {
        GeneratedObjectRef::new(
            GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
            100,
        )
        .unwrap()
    };
    let artifact = GeneratedArtifact {
        sampled_asset: AssetId::new("sampled").unwrap(),
        sampled_object: object('a'),
        native_asset: AssetId::new("native").unwrap(),
        native_object: object('b'),
        provenance: object('c'),
        sampling: BridgeSamplingMap::new(
            FrameRate::new(30, 1).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            FrameDuration::new(5).unwrap(),
            FrameDuration::new(6).unwrap(),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap()
        .into(),
        content_aspect: aspect,
    };
    Picture::Accepted {
        asset: artifact.sampled_asset.clone(),
        generated: Some(std::sync::Arc::new(artifact)),
        time_base: SourceTimeBase::new(1, 30).unwrap(),
        position: ExactRatio::integer(frame as i64),
        frame: SourceFrameId(frame),
    }
}

#[test]
fn generated_identity_uses_sampled_ordinal_and_normalized_saved_crop() {
    let connection = connection();
    let pictures = QualifiedGenerationPictures::new(&connection);
    let document = empty_document();
    let identify = |aspect, frame| {
        pictures
            .identity(&document, &generated(aspect, frame))
            .unwrap()
    };
    assert_eq!(identify(Some([640, 480]), 2), identify(Some([4, 3]), 2));
    assert_ne!(identify(Some([4, 3]), 2), identify(Some([16, 9]), 2));
    assert_ne!(identify(Some([4, 3]), 2), identify(None, 2));
    assert_ne!(identify(Some([4, 3]), 2), identify(Some([4, 3]), 3));
    // Six sampled pictures and five native pictures: sampled ordinal 5 is legal.
    assert!(matches!(
        identify(Some([4, 3]), 5),
        GenerationPictureIdentity::Generated {
            frame: SourceFrameId(5),
            content_aspect: Some([4, 3]),
            ..
        }
    ));
    assert!(
        pictures
            .identity(&document, &generated(Some([4, 3]), 6))
            .is_err()
    );
    assert!(
        pictures
            .identity(&document, &generated(Some([0, 3]), 2))
            .is_err()
    );
}
