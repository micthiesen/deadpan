use super::*;
use crate::protocol::{Sha256, WorkspaceRef};

fn request(selection: ProposalSelection) -> HostMessage {
    HostMessage::DetectPictureProposals {
        protocol: VERSION,
        request: RequestId::new("proposals-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        cancellation_token: CancellationToken::new("cancel-1").unwrap(),
        source: WorkspaceArtifact::new(
            WorkspaceRef::new("input/source").unwrap(),
            Sha256::new("1".repeat(64)).unwrap(),
            49_000,
        )
        .unwrap(),
        stream: Box::new(ExpectedStream {
            interpretation: None,
            stream_index: 0,
            width: 320,
            height: 180,
            clean_aperture: None,
            time_base_num: 1,
            time_base_den: 1000,
            rotation_quarter_turns: 0,
        }),
        selection,
        pts: 1000,
        timeout_millis: 60_000,
    }
}
fn face(x: f64, y: f64) -> DetectedFace {
    DetectedFace {
        region: NormalizedRect::new(x, y, 0.1, 0.2).unwrap(),
        confidence: 0.8,
    }
}
fn region(x: f64, y: f64) -> DetectedSalientRegion {
    DetectedSalientRegion {
        region: NormalizedRect::new(x, y, 0.1, 0.2).unwrap(),
    }
}
fn runtime(detector: DetectorKind) -> DetectorRuntime {
    DetectorRuntime {
        detector,
        engine: match detector {
            DetectorKind::Faces => FACE_ENGINE,
            DetectorKind::SalientRegions => SALIENCY_ENGINE,
        }
        .into(),
        request_revision: if detector == DetectorKind::Faces {
            3
        } else {
            SALIENCY_REQUEST_REVISION
        },
    }
}
fn proposals(selection: ProposalSelection) -> PictureProposals {
    PictureProposals {
        faces: selection.includes_faces().then(|| FaceProposals {
            faces: vec![face(0.1, 0.2), face(0.5, 0.3)],
            runtime: runtime(DetectorKind::Faces),
        }),
        salient_regions: selection
            .includes_salient_regions()
            .then(|| SalientRegionProposals {
                regions: vec![region(0.2, 0.1), region(0.6, 0.2)],
                runtime: runtime(DetectorKind::SalientRegions),
            }),
    }
}
fn completed(selection: ProposalSelection) -> WorkerMessage {
    WorkerMessage::Completed {
        protocol: VERSION,
        request: RequestId::new("proposals-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        selection,
        pts: 1000,
        proposals: proposals(selection),
        decode_millis: 1,
        vision_millis: 2,
        elapsed_millis: 3,
    }
}

#[test]
fn requests_preserve_source_stream_timeout_and_closed_mode_bounds() {
    for selection in [
        ProposalSelection::Faces,
        ProposalSelection::SalientRegions,
        ProposalSelection::Both,
    ] {
        assert!(request(selection).validate().is_ok());
    }
    let mut nested = request(ProposalSelection::Both);
    if let HostMessage::DetectPictureProposals { source, .. } = &mut nested {
        *source = WorkspaceArtifact::new(
            WorkspaceRef::new("input/nested/source").unwrap(),
            Sha256::new("1".repeat(64)).unwrap(),
            1,
        )
        .unwrap();
    }
    assert!(nested.validate().is_err());
    for size in [
        1,
        crate::tracking::MAX_SOURCE_BYTES,
        crate::tracking::MAX_SOURCE_BYTES + 1,
    ] {
        let mut r = request(ProposalSelection::Faces);
        if let HostMessage::DetectPictureProposals { source, .. } = &mut r {
            *source = WorkspaceArtifact::new(
                WorkspaceRef::new("input/source").unwrap(),
                Sha256::new("1".repeat(64)).unwrap(),
                size,
            )
            .unwrap();
        }
        assert_eq!(
            r.validate().is_ok(),
            size <= crate::tracking::MAX_SOURCE_BYTES
        );
    }
    for field in ["selection", "extra", "protocol", "timeout_millis", "stream"] {
        let mut value = serde_json::to_value(request(ProposalSelection::Both)).unwrap();
        match field {
            "selection" => value[field] = "all".into(),
            "extra" => value[field] = true.into(),
            "protocol" => value[field] = 1.into(),
            "timeout_millis" => value[field] = 0.into(),
            "stream" => value["stream"]["rotation_quarter_turns"] = 4.into(),
            _ => unreachable!(),
        }
        let parsed = serde_json::from_value::<HostMessage>(value);
        assert!(
            parsed.is_err() || parsed.unwrap().validate().is_err(),
            "{field}"
        );
    }
}

#[test]
fn all_three_modes_bind_attempt_exact_pts_and_collection_presence() {
    for selection in [
        ProposalSelection::Faces,
        ProposalSelection::SalientRegions,
        ProposalSelection::Both,
    ] {
        let protocol = PictureProposalProtocol::from_request(&request(selection)).unwrap();
        assert_eq!(
            protocol.classify(&completed(selection)).unwrap(),
            ResponseKind::Completed
        );
        for other in [
            ProposalSelection::Faces,
            ProposalSelection::SalientRegions,
            ProposalSelection::Both,
        ] {
            assert_eq!(
                protocol.classify(&completed(other)).is_ok(),
                selection == other
            );
        }
        for mutate in 0..4 {
            let mut message = completed(selection);
            if let WorkerMessage::Completed {
                pts,
                request,
                attempt,
                proposals,
                ..
            } = &mut message
            {
                match mutate {
                    0 => *pts = 999,
                    1 => *request = RequestId::new("foreign").unwrap(),
                    2 => *attempt = AttemptId::new("foreign").unwrap(),
                    3 => {
                        if selection.includes_faces() {
                            proposals.faces = None
                        } else {
                            proposals.faces = Some(FaceProposals {
                                faces: vec![],
                                runtime: runtime(DetectorKind::Faces),
                            })
                        }
                    }
                    _ => unreachable!(),
                }
            }
            assert!(protocol.classify(&message).is_err());
        }
        let cancellation = protocol.cancellation();
        match &cancellation {
            HostMessage::Cancel {
                request,
                attempt,
                cancellation_token,
                ..
            } => {
                assert_eq!(request.as_str(), "proposals-1");
                assert_eq!(attempt.as_str(), "attempt-1");
                assert_eq!(cancellation_token.as_str(), "cancel-1");
            }
            _ => panic!("not cancellation"),
        }
        assert!(PictureProposalProtocol::from_request(&cancellation).is_err());
        let mut empty = proposals(selection);
        if let Some(found) = &mut empty.faces {
            found.faces.clear()
        }
        if let Some(found) = &mut empty.salient_regions {
            found.regions.clear()
        }
        assert!(empty.validate(selection).is_ok());
    }
}

#[test]
fn detector_provenance_is_kind_bound_and_only_saliency_revision_two_is_accepted() {
    for kind in [DetectorKind::Faces, DetectorKind::SalientRegions] {
        let valid = runtime(kind);
        assert!(valid.validate(kind).is_ok());
        for revision in [0, 1, 2, 3, 99] {
            let mut reported = valid.clone();
            reported.request_revision = revision;
            assert_eq!(
                reported.validate(kind).is_ok(),
                if kind == DetectorKind::Faces {
                    revision > 0
                } else {
                    revision == 2
                }
            );
        }
        let other = if kind == DetectorKind::Faces {
            DetectorKind::SalientRegions
        } else {
            DetectorKind::Faces
        };
        assert!(valid.validate(other).is_err());
        for label in [
            "".to_string(),
            "x".repeat(97),
            "Vision\ninjected".to_string(),
        ] {
            let mut bad = valid.clone();
            bad.engine = label;
            assert!(bad.validate(kind).is_err());
        }
    }
    let mut both = proposals(ProposalSelection::Both);
    both.salient_regions.as_mut().unwrap().runtime = runtime(DetectorKind::Faces);
    assert!(both.validate(ProposalSelection::Both).is_err());
}

#[test]
fn wire_completion_refuses_unknown_and_cross_kind_engine_labels() {
    for kind in [DetectorKind::Faces, DetectorKind::SalientRegions] {
        let protocol =
            PictureProposalProtocol::from_request(&request(ProposalSelection::Both)).unwrap();
        for label in [
            "fixture detector".to_string(),
            FACE_ENGINE.to_lowercase(),
            format!("{SALIENCY_ENGINE} "),
            if kind == DetectorKind::Faces {
                SALIENCY_ENGINE.to_string()
            } else {
                FACE_ENGINE.to_string()
            },
        ] {
            let mut message = completed(ProposalSelection::Both);
            if let WorkerMessage::Completed { proposals, .. } = &mut message {
                let reported = match kind {
                    DetectorKind::Faces => &mut proposals.faces.as_mut().unwrap().runtime,
                    DetectorKind::SalientRegions => {
                        &mut proposals.salient_regions.as_mut().unwrap().runtime
                    }
                };
                // Keep the detector kind and revision correct: the engine
                // label itself must reject the entire Both completion.
                reported.engine = label;
                assert!(reported.validate(kind).is_err());
            }
            let mut wire = Vec::new();
            assert!(write_worker(&mut wire, &message).is_err());
            assert!(wire.is_empty());
            // Simulate an untrusted worker that bypasses its validated writer.
            write_frame(&mut wire, &message).unwrap();
            assert!(PictureProposalProtocol::read_response(&mut wire.as_slice()).is_err());
            assert!(protocol.classify(&message).is_err());
        }
    }
}

#[test]
fn per_kind_order_and_confidence_are_independent_and_strict() {
    let mut faces = vec![face(0.6, 0.1), face(0.2, 0.7), face(0.2, 0.3)];
    faces.sort_by(DetectedFace::order);
    assert_eq!(
        faces
            .iter()
            .map(|f| (f.region.x(), f.region.y()))
            .collect::<Vec<_>>(),
        [(0.2, 0.3), (0.2, 0.7), (0.6, 0.1)]
    );
    assert!(validate_faces(&faces).is_ok());
    for confidence in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        faces[0].confidence = confidence;
        assert!(validate_faces(&faces).is_err());
    }
    let mut regions = vec![region(0.6, 0.1), region(0.2, 0.7), region(0.2, 0.3)];
    assert!(validate_salient_regions(&regions).is_err());
    regions.sort_by(DetectedSalientRegion::order);
    assert_eq!(
        regions
            .iter()
            .map(|f| (f.region.x(), f.region.y()))
            .collect::<Vec<_>>(),
        [(0.2, 0.3), (0.2, 0.7), (0.6, 0.1)]
    );
    assert!(validate_salient_regions(&regions).is_ok());
    regions.push(regions[2]);
    assert!(validate_salient_regions(&regions).is_err());
    let serialized = serde_json::to_value(region(0.1, 0.2)).unwrap();
    assert!(serialized.get("confidence").is_none());
    let mut invented = serialized;
    invented["confidence"] = 0.9.into();
    assert!(serde_json::from_value::<DetectedSalientRegion>(invented).is_err());
}

#[test]
fn raw_collection_caps_refuse_before_decoding_item_sixty_five() {
    for selection in [ProposalSelection::Faces, ProposalSelection::SalientRegions] {
        let mut data = proposals(selection);
        if let Some(f) = &mut data.faces {
            f.faces = (0..64).map(|i| face(f64::from(i) * 0.001, 0.2)).collect()
        }
        if let Some(r) = &mut data.salient_regions {
            r.regions = (0..64).map(|i| region(f64::from(i) * 0.001, 0.2)).collect()
        }
        assert!(data.validate(selection).is_ok());
        let mut value = serde_json::to_value(&data).unwrap();
        let decoded: PictureProposals = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(data, decoded);
        let (collection, items) = if selection == ProposalSelection::Faces {
            ("faces", "faces")
        } else {
            ("salient_regions", "regions")
        };
        // Null is deliberately not a valid proposal. The limit, not item65's
        // shape, must reject it before deserializing another typed item.
        value[collection][items]
            .as_array_mut()
            .unwrap()
            .push(serde_json::Value::Null);
        assert!(
            serde_json::from_value::<PictureProposals>(value)
                .unwrap_err()
                .to_string()
                .contains("bound")
        );
    }
    let mut regions = (0..65)
        .map(|i| region(f64::from(i) * 0.001, 0.2))
        .collect::<Vec<_>>();
    assert!(validate_salient_regions(&regions).is_err());
    regions.truncate(64);
    assert!(validate_salient_regions(&regions).is_ok());
    let faces = (0..65)
        .map(|i| face(f64::from(i) * 0.001, 0.2))
        .collect::<Vec<_>>();
    assert!(validate_faces(&faces).is_err());
}

#[test]
fn raw_geometry_unknown_duplicate_fields_and_frame_bytes_refuse() {
    let baseline = serde_json::to_string(&proposals(ProposalSelection::Both)).unwrap();
    let duplicate = baseline.replacen("\"faces\":[", "\"faces\":[],\"faces\":[", 1);
    assert!(serde_json::from_str::<PictureProposals>(&duplicate).is_err());
    for bad in [
        r#"{"region":{"x":0.95,"y":0.2,"width":0.1,"height":0.2}}"#,
        r#"{"region":{"x":0.1,"y":0.2,"width":0,"height":0.2}}"#,
        r#"{"region":{"x":0.1,"y":0.2,"width":0.1,"height":0.2,"extra":1}}"#,
        r#"{"region":{"x":1e999,"y":0.2,"width":0.1,"height":0.2}}"#,
    ] {
        assert!(serde_json::from_str::<DetectedSalientRegion>(bad).is_err());
    }
    let length = u32::try_from(crate::protocol::MAX_FRAME_BYTES + 1)
        .unwrap()
        .to_be_bytes();
    assert!(PictureProposalProtocol::read_response(&mut length.as_slice()).is_err());
    let mut encoded = Vec::new();
    write_worker(&mut encoded, &completed(ProposalSelection::Both)).unwrap();
    assert_eq!(
        PictureProposalProtocol::read_response(&mut encoded.as_slice()).unwrap(),
        Some(completed(ProposalSelection::Both))
    );
}

#[test]
fn adversarial_face_frames() {
    let detect = request(ProposalSelection::Both);
    let cancel = PictureProposalProtocol::from_request(&detect)
        .unwrap()
        .cancellation();
    crate::adversarial::protocol::<PictureProposalProtocol>(
        "jobs-faces-protocol",
        vec![detect, cancel],
        vec![
            completed(ProposalSelection::Both),
            completed(ProposalSelection::SalientRegions),
        ],
        |reader| read_host(reader),
    );
}
