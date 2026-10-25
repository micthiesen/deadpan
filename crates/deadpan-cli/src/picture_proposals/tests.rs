use super::*;
use deadpan_jobs::picture_proposals::{FACE_ENGINE, SALIENCY_ENGINE};
use deadpan_store::{AccessMode, ProjectStore};
use std::cell::Cell;
use std::process::ExitCode;

thread_local! { static COPIES: Cell<Option<usize>> = const { Cell::new(None) }; }
pub(super) fn record_copy() {
    COPIES.with(|count| {
        if let Some(old) = count.get() {
            count.set(Some(old + 1));
        }
    });
}
fn with_copy_count<T>(f: impl FnOnce() -> T) -> (T, usize) {
    struct Restore(Option<usize>);
    impl Drop for Restore {
        fn drop(&mut self) {
            COPIES.with(|count| count.set(self.0));
        }
    }
    let restore = Restore(COPIES.with(|count| count.replace(Some(0))));
    let result = f();
    let copies = COPIES.with(|count| count.get().unwrap());
    drop(restore);
    (result, copies)
}
fn face(x: f64) -> DetectedFace {
    DetectedFace {
        region: NormalizedRect::new(x, 0.2, 0.1, 0.3).unwrap(),
        confidence: 0.9,
    }
}
fn region(x: f64) -> DetectedSalientRegion {
    DetectedSalientRegion {
        region: NormalizedRect::new(x, 0.2, 0.1, 0.3).unwrap(),
    }
}
fn fixture(scratch: &Path) -> std::path::PathBuf {
    fn cli(arguments: &[&str]) {
        assert_eq!(
            crate::entry(arguments.iter().map(|s| s.to_string())),
            ExitCode::SUCCESS
        );
    }
    let package = scratch.join("proposals.deadpan");
    let project = package.to_str().unwrap();
    cli(&[
        "project", "create", project, "--fps", "24/1", "--size", "480x270",
    ]);
    let source = scratch.join("source.mkv");
    std::fs::write(
        &source,
        include_bytes!("../../../../native/deadpan-track/tests/fixtures/two-drawn-faces.mkv"),
    )
    .unwrap();
    cli(&[
        "project",
        "retain-original",
        project,
        source.to_str().unwrap(),
    ]);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let records = store.original_records(None, 8).unwrap();
    let original = serde_json::to_value(records[0].object().content()).unwrap();
    let snapshot = store.snapshot().unwrap();
    drop(store);
    let request = serde_json::json!({
        "protocol":1,
        "registration":{"expected_revision":snapshot.revision_id(),"new_revision":"registered",
            "original":original,"new_asset_id":"clip","label":"Clip",
            "insertion":{"parent":snapshot.root(),"index":0,"node":"source","label":"Clip"}},
        "streams":{"type":"video_only"}
    });
    let file = scratch.join("register.json");
    std::fs::write(&file, serde_json::to_vec(&request).unwrap()).unwrap();
    cli(&[
        "project",
        "register-source",
        project,
        "--request-json",
        file.to_str().unwrap(),
    ]);
    package
}
fn request(selection: ProposalSelection, at_pts: i64) -> PictureProposalRequest {
    PictureProposalRequest {
        asset: Some(AssetId::new("clip").unwrap()),
        at_pts,
        selection,
    }
}
fn runtime(detector: DetectorKind) -> DetectorRuntime {
    // Simulated protocol payload only; these fixtures do not execute Vision.
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
fn detection(prepared: &PreparedPictureProposals) -> PictureProposalDetection {
    PictureProposalDetection {
        selection: prepared.selection(),
        pts: prepared.pts,
        proposals: PictureProposals {
            faces: prepared
                .selection()
                .includes_faces()
                .then(|| FaceProposals {
                    faces: vec![face(0.1), face(0.5)],
                    runtime: runtime(DetectorKind::Faces),
                }),
            salient_regions: prepared.selection().includes_salient_regions().then(|| {
                SalientRegionProposals {
                    regions: vec![region(0.2), region(0.6)],
                    runtime: runtime(DetectorKind::SalientRegions),
                }
            }),
        },
        decode_elapsed: Duration::ZERO,
        vision_elapsed: Duration::ZERO,
        worker_elapsed: Duration::ZERO,
    }
}

#[test]
fn one_verified_copy_prepares_both_collections_without_changing_history() {
    use sha2::{Digest, Sha256};
    let scratch = tempfile::tempdir().unwrap();
    let package = fixture(scratch.path());
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let before = store.snapshot().unwrap();
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let (prepared, copies) = with_copy_count(|| {
        prepare_picture_proposals(
            &store,
            &request(ProposalSelection::Both, 0),
            &cancelled,
            deadline,
        )
        .unwrap()
    });
    assert_eq!(copies, 1);
    assert_eq!(prepared.selection(), ProposalSelection::Both);
    assert_eq!(prepared.asset, AssetId::new("clip").unwrap());
    assert_eq!(prepared.ordinal, 0);
    assert_eq!(prepared.pts, 0);
    assert_eq!(
        prepared.display_aspect,
        Some(deadpan_core::ExactRatio::new(16, 9).unwrap())
    );
    assert_eq!(prepared.head, before);
    assert_eq!(store.snapshot().unwrap(), before);
    drop(store);
    // The one input remains independently owned after the store and linked
    // original disappear. Both detector lists use this prepared value.
    std::fs::remove_file(scratch.path().join("source.mkv")).unwrap();
    let input = prepared.workspace.path().join("input");
    let entries = std::fs::read_dir(&input)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 1);
    let copied = std::fs::read(entries[0].path()).unwrap();
    assert_eq!(
        u64::try_from(copied.len()).unwrap(),
        prepared.source.byte_length()
    );
    assert_eq!(
        Sha256::digest(&copied)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        prepared.source.sha256().as_str()
    );
    let admitted = admit_picture_proposals(&prepared, detection(&prepared)).unwrap();
    assert_eq!(
        admitted.proposals.faces.unwrap().faces,
        [face(0.1), face(0.5)]
    );
    assert_eq!(
        admitted.proposals.salient_regions.unwrap().regions,
        [region(0.2), region(0.6)]
    );
}

#[test]
fn exact_index_selection_and_all_admission_paths_keep_mode_pts_and_bounds() {
    let scratch = tempfile::tempdir().unwrap();
    let package = fixture(scratch.path());
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let prepared = prepare_picture_proposals(
        &store,
        &request(ProposalSelection::Both, 1),
        &cancelled,
        deadline,
    )
    .unwrap();
    // PTS1 resolves the picture at PTS0; Camera's stricter installed-picture
    // gate checks that this differs from the requested PTS.
    assert_eq!((prepared.ordinal, prepared.pts), (0, 0));
    for selection in [ProposalSelection::Faces, ProposalSelection::SalientRegions] {
        let mut bad = detection(&prepared);
        bad.selection = selection;
        assert!(matches!(
            admit_picture_proposals(&prepared, bad),
            Err(ProposalError::Protocol(_))
        ));
    }
    let mut bad = detection(&prepared);
    bad.pts = 1;
    assert!(admit_picture_proposals(&prepared, bad).is_err());
    let mut bad = detection(&prepared);
    bad.proposals.salient_regions = None;
    assert!(admit_picture_proposals(&prepared, bad).is_err());
    for kind in [DetectorKind::Faces, DetectorKind::SalientRegions] {
        for label in [
            "fixture detector",
            if kind == DetectorKind::Faces {
                SALIENCY_ENGINE
            } else {
                FACE_ENGINE
            },
        ] {
            let mut bad = detection(&prepared);
            let runtime = match kind {
                DetectorKind::Faces => &mut bad.proposals.faces.as_mut().unwrap().runtime,
                DetectorKind::SalientRegions => {
                    &mut bad.proposals.salient_regions.as_mut().unwrap().runtime
                }
            };
            runtime.engine = label.into();
            assert!(matches!(
                admit_picture_proposals(&prepared, bad),
                Err(ProposalError::Protocol(_))
            ));
        }
    }
    let mut bad = detection(&prepared);
    bad.proposals
        .salient_regions
        .as_mut()
        .unwrap()
        .runtime
        .request_revision = 1;
    assert!(admit_picture_proposals(&prepared, bad).is_err());
    let mut bad = detection(&prepared);
    bad.proposals.faces.as_mut().unwrap().faces[0].confidence = f32::NAN;
    assert!(admit_picture_proposals(&prepared, bad).is_err());
    let mut bad = detection(&prepared);
    bad.proposals
        .salient_regions
        .as_mut()
        .unwrap()
        .regions
        .reverse();
    assert!(admit_picture_proposals(&prepared, bad).is_err());
    let mut bad = detection(&prepared);
    bad.proposals.salient_regions.as_mut().unwrap().regions =
        (0..65).map(|i| region(f64::from(i) * 0.001)).collect();
    assert!(admit_picture_proposals(&prepared, bad).is_err());
    // Explicitly scripted geometry does not manufacture an Apple revision.
    assert!(
        admit_proposal_geometry(
            &prepared,
            ProposalSelection::Both,
            0,
            Some(&[face(0.1)]),
            Some(&[region(0.2)])
        )
        .is_ok()
    );
    assert!(
        admit_proposal_geometry(&prepared, ProposalSelection::Both, 0, Some(&[]), None).is_err()
    );
    assert!(crate::faces::admit_faces(&prepared, 0, vec![face(0.1)]).is_err());
    assert!(
        prepare_picture_proposals(
            &store,
            &request(ProposalSelection::Faces, -1),
            &cancelled,
            deadline
        )
        .is_err()
    );
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        prepare_picture_proposals(
            &store,
            &request(ProposalSelection::Faces, 0),
            &cancelled,
            deadline
        ),
        Err(ProposalError::Cancelled)
    ));
    assert!(matches!(
        prepare_picture_proposals(
            &store,
            &request(ProposalSelection::Faces, 0),
            &AtomicBool::new(false),
            Instant::now() - Duration::from_secs(1)
        ),
        Err(ProposalError::Deadline)
    ));
}

#[test]
fn face_facade_still_admits_only_face_mode_and_preserves_target_semantics() {
    let scratch = tempfile::tempdir().unwrap();
    let package = fixture(scratch.path());
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    let (prepared, copies) = with_copy_count(|| {
        crate::faces::prepare_faces(
            &store,
            &crate::faces::FaceRequest {
                asset: Some(AssetId::new("clip").unwrap()),
                at_pts: 0,
            },
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap()
    });
    assert_eq!(copies, 1);
    assert_eq!(prepared.selection(), ProposalSelection::Faces);
    assert_eq!(
        crate::faces::admit_faces(&prepared, 0, vec![face(0.1)]).unwrap(),
        [face(0.1)]
    );
    assert!(crate::faces::admit_faces(&prepared, 1, vec![face(0.1)]).is_err());
    let target = region_target(
        &region(0.2).region,
        "Region 1".into(),
        prepared.asset.clone(),
        prepared.time_base,
        prepared.display_aspect,
        prepared.pts,
        prepared.video_end,
    )
    .unwrap();
    assert_eq!(target.region.center(), [250_000, 350_000]);
    assert_eq!(target.region.as_region().unwrap().size, [100_000, 300_000]);
    assert_eq!(target.display_aspect, prepared.display_aspect);
    assert!(
        target.samples.is_empty() && target.corrections.is_empty() && target.provenance.is_none()
    );
    assert_eq!(target.span.start().ticks, 0);
    assert_eq!(target.span.end().ticks, prepared.video_end);
    assert!(
        region_target(
            &region(0.2).region,
            "Region".into(),
            prepared.asset.clone(),
            prepared.time_base,
            prepared.display_aspect,
            1,
            1
        )
        .is_err()
    );
    let other = crate::faces::face_target(
        &face(0.2),
        "Region 1".into(),
        prepared.asset.clone(),
        prepared.time_base,
        prepared.display_aspect,
        prepared.pts,
        prepared.video_end,
    )
    .unwrap();
    assert_eq!(target, other);
}

#[test]
fn salient_numbering_and_command_refusals_are_explicit_without_writes() {
    let regions = [region(0.1), region(0.6)];
    assert_eq!(choose_salient_region(&regions, 2).unwrap(), &regions[1]);
    assert!(
        choose_salient_region(&[], 1)
            .unwrap_err()
            .contains("No salient regions")
    );
    assert!(
        choose_salient_region(&regions, 0)
            .unwrap_err()
            .contains("from 1")
    );
    assert!(
        choose_salient_region(&regions, 3)
            .unwrap_err()
            .contains("region:3")
    );
    for arguments in [
        vec![],
        vec!["missing.deadpan"],
        vec!["missing.deadpan", "--at", "oops"],
        vec!["missing.deadpan", "--at", "0", "--at", "1"],
        vec!["missing.deadpan", "--at", "0", "--unknown", "1"],
        vec![
            "missing.deadpan",
            "--at",
            "0",
            "--include-faces",
            "--include-faces",
        ],
    ] {
        assert!(matches!(
            run_detect_regions(&arguments),
            Err(crate::CliError::Usage(_))
        ));
    }
    assert_eq!(
        crate::CliError::Faces(crate::faces::FaceError::Unavailable("x".into())).code(),
        "FaceDetectionUnavailable"
    );
    assert_eq!(
        crate::CliError::PictureProposals(ProposalError::Unavailable("x".into())).code(),
        "PictureProposalsUnavailable"
    );
}

#[test]
fn copy_witness_restores_nested_and_unwinding_scopes() {
    assert_eq!(
        with_copy_count(|| {
            record_copy();
            assert_eq!(
                with_copy_count(|| {
                    record_copy();
                    record_copy();
                })
                .1,
                2
            );
            record_copy();
        })
        .1,
        2
    );
    assert!(
        std::panic::catch_unwind(|| with_copy_count(|| {
            record_copy();
            panic!("scoped witness unwind");
        }))
        .is_err()
    );
    assert_eq!(with_copy_count(|| {}).1, 0);
}
