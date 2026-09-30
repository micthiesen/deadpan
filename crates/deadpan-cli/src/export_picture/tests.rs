use std::collections::BTreeMap;
use std::path::Path;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectFrame, ProjectId,
    RevisionId, Subtree,
};
use deadpan_store::ProjectStore;

use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn captured(path: &Path) -> Result<ProjectPictureSession> {
    let initial = ProjectDocument::new(
        ProjectId::new("encoder-pictures")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 319,
            height: 179,
            frame_rate: FrameRate::new(30000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(path, &initial)?;
    let beat = NodeId::new("black")?;
    store.commit(&CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("inserted")?,
        command: Command::Insert {
            parent: initial.root().clone(),
            index: 0,
            subtree: Subtree {
                root: beat.clone(),
                nodes: BTreeMap::from([(
                    beat,
                    BeatNode::hold(
                        "Black",
                        HoldRecipe {
                            duration: FrameDuration::new(30)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                            picture_context: None,
                        },
                    ),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    })?;
    Ok(ProjectPictureSession::open_revision(
        path,
        &RevisionId::new("inserted")?,
        None,
        &AtomicBool::new(false),
    )?)
}

#[test]
fn captured_contract_rejects_other_revision_clock_canvas_and_position() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut pictures = captured(&scratch.path().join("output.deadpan"))?;
    let contract = ExportPictureContract::capture(&pictures)?;
    let timing = contract.timing(OutputFrameOrdinal(7))?;
    let mut prepared = pictures.prepare(ProjectFrame(7), &AtomicBool::new(false))?;
    validate_prepared(&contract, timing, &prepared)?;
    prepared.revision_id = RevisionId::new("newer")?;
    assert!(matches!(
        validate_prepared(&contract, timing, &prepared),
        Err(ExportPictureError::InvalidContract(_))
    ));
    prepared.revision_id = contract.revision_id().clone();
    prepared.project_id = ProjectId::new("unrelated")?;
    assert!(validate_prepared(&contract, timing, &prepared).is_err());
    prepared.project_id = contract.project_id().clone();
    prepared.project_frame = ProjectFrame(8);
    assert!(validate_prepared(&contract, timing, &prepared).is_err());
    prepared.project_frame = timing.project_frame();
    // Raster normalization must happen after authored framing; replacing the
    // prepared canvas with codec dimensions would distort captured geometry.
    prepared.canvas = contract.raster();
    assert!(validate_prepared(&contract, timing, &prepared).is_err());
    prepared.canvas = contract.canvas();
    prepared.frame_rate = FrameRate::new(30, 1)?;
    assert!(validate_prepared(&contract, timing, &prepared).is_err());
    prepared.frame_rate = contract.frame_rate();
    validate_prepared(&contract, timing, &prepared)?;
    Ok(())
}

#[test]
fn completed_result_budget_is_released_on_worker_drop_and_failed_work() {
    let outstanding = Arc::new(AtomicBool::new(false));
    let retained = CompletedPermit::acquire(&outstanding).unwrap();
    assert!(matches!(
        CompletedPermit::acquire(&outstanding),
        Err(ExportPictureError::OutstandingFrame)
    ));
    std::thread::spawn(move || drop(retained)).join().unwrap();
    let failed_work = || -> std::result::Result<(), ExportPictureError> {
        let _permit = CompletedPermit::acquire(&outstanding)?;
        Err(ExportPictureError::Cancelled)
    };
    assert!(matches!(failed_work(), Err(ExportPictureError::Cancelled)));
    let next = CompletedPermit::acquire(&outstanding).unwrap();
    drop(next);
    assert!(!outstanding.load(Ordering::Acquire));
}

#[test]
fn cancellation_and_expired_deadline_stop_before_next_poll() {
    assert!(matches!(
        wait_for_progress(
            &AtomicBool::new(true),
            Instant::now() + Duration::from_secs(1)
        ),
        Err(ExportPictureError::Cancelled)
    ));
    assert!(matches!(
        wait_for_progress(&AtomicBool::new(false), Instant::now()),
        Err(ExportPictureError::Deadline)
    ));
}
