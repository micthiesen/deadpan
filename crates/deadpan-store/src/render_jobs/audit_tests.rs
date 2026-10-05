//! Count the actual expensive reads in the production audit, without clocks or
//! SQL tracing. Thread-local counters isolate concurrently running unit tests.
use super::*;
use deadpan_core::{
    BeatNode, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId,
    PresentationBasis, ProjectDocument, ProjectId, RevisionId, Subtree,
};
use deadpan_jobs::render::{
    RenderAutomaticAlgorithm, RenderAutomaticPolicy, RenderAutomaticSelection, RenderPolicy,
    admission::{
        RenderAdmissionFailure, RenderAdmissionFailureKind, RenderDecisionOutcome,
        RenderEncodingDecision,
    },
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

thread_local! {
    pub(super) static HEAD_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn audit_reads(connection: &Connection) -> Result<(usize, usize)> {
    HEAD_READS.with(|count| count.set(0));
    validation::REVISION_READS.with(|count| count.set(0));
    validate_store(connection)?;
    Ok((
        HEAD_READS.with(|count| count.get()),
        validation::REVISION_READS.with(|count| count.get()),
    ))
}

#[test]
fn render_audit_reuses_engineering_jobs_across_historical_checkpoint_retries() -> Result {
    let (_root, mut store) = test_fixture::store()?;
    // Exercise real checkpoint ownership and retry allocation through the
    // current store API. The opaque bytes and observations are synthetic.
    // Two jobs share the baseline; the third captures a later stored revision.
    for (name, attempt_count) in [
        ("engineering-a", 8),
        ("engineering-b", 9),
        ("engineering-c", 9),
    ] {
        if name == "engineering-c" {
            let before = store.snapshot()?;
            store.commit(&CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: RevisionId::new("audit-later")?,
                command: Command::Rename {
                    node: before.root().clone(),
                    label: "later synthetic render revision".into(),
                },
            })?;
        }
        let owner = test_fixture::verified(&mut store, name)?;
        for ordinal in 2..=attempt_count {
            let retry =
                test_fixture::verify_again(&mut store, &owner, &format!("{name}-retry-{ordinal}"))?;
            assert_eq!(
                retry.checkpoint_attempt_id.as_ref(),
                Some(&owner.attempt_id)
            );
        }
    }
    store.validate()?;
    let connection = &store.connection;
    let jobs: i64 =
        connection.query_row("SELECT COUNT(*) FROM render_jobs", [], |row| row.get(0))?;
    let revisions: i64 = connection.query_row(
        "SELECT COUNT(DISTINCT revision_id) FROM render_jobs",
        [],
        |row| row.get(0),
    )?;
    let attempts: i64 =
        connection.query_row("SELECT COUNT(*) FROM render_attempts", [], |row| row.get(0))?;
    let retries: i64 = connection.query_row("SELECT COUNT(*) FROM render_attempts WHERE json_extract(body,'$.checkpoint_attempt_id')!=attempt_id", [], |row| row.get(0))?;
    assert_eq!(jobs, 3);
    assert_eq!(revisions, 2);
    assert_eq!(attempts, 26);
    assert_eq!(retries, attempts - jobs);
    assert_eq!(
        audit_reads(connection)?,
        (usize::try_from(jobs)?, usize::try_from(revisions)?)
    );
    Ok(())
}

fn document(basis: PresentationBasis, frames: i64, revision: &str) -> Result<ProjectDocument> {
    let before = ProjectDocument::new(
        ProjectId::new("audit-project")?,
        RevisionId::new("before")?,
        basis,
        NodeId::new("root")?,
    )?;
    let edit = deadpan_core::apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new(revision)?,
            command: Command::Insert {
                parent: before.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: NodeId::new("hold")?,
                    nodes: BTreeMap::from([(
                        NodeId::new("hold")?,
                        BeatNode::hold(
                            "pause",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(frames)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        },
    )?;
    Ok(edit.forward.apply(&before)?)
}

fn automatic_history(attempts_per_job: u64) -> Result<Connection> {
    let mut connection = Connection::open_in_memory()?;
    crate::schema::configure(&connection)?;
    crate::schema::create(&mut connection)?;
    let measured = RenderEncodingDecision::from_json(include_bytes!(
        "../../../deadpan-jobs/src/render/admission/tests/measured-decision-v1.json"
    ))?;
    let basis = PresentationBasis {
        width: measured.output.canvas[0],
        height: measured.output.canvas[1],
        frame_rate: measured.output.frame_rate,
        color_policy: measured.output.color_policy,
    };
    let documents = [
        document(
            basis.clone(),
            i64::try_from(measured.output.frame_count)?,
            "revision-a",
        )?,
        document(
            basis,
            i64::try_from(measured.output.frame_count)?,
            "revision-b",
        )?,
    ];
    for document in &documents {
        connection.execute(
            "INSERT INTO revisions(id,kind,document,depth,json_bound) VALUES(?1,'initial',?2,0,length(?2))",
            params![document.revision_id().as_str(), document.to_json()?],
        )?;
    }
    // Two jobs share one revision; a third uses a separate immutable revision.
    for (name, revision) in [("automatic-a", 0), ("automatic-b", 0), ("automatic-c", 1)] {
        let document = &documents[revision];
        let intent = RenderIntent {
            schema_version: 2,
            job_id: RequestId::new(name)?,
            project_id: document.project_id().clone(),
            revision_id: document.revision_id().clone(),
            document_sha256: render::document_sha256_for_validation(document)?,
            range: measured.output.range,
            policy: RenderPolicy::Automatic(RenderAutomaticPolicy {
                schema_version: 1,
                selection: RenderAutomaticSelection::Automatic,
                algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
            }),
        };
        intent.validate()?;
        connection.execute(
            "INSERT INTO render_jobs(job_id,revision_id,intent) VALUES(?1,?2,?3)",
            params![
                name,
                intent.revision_id.as_str(),
                serde_json::to_string(&intent)?
            ],
        )?;
        for ordinal in 1..=attempts_per_job {
            let attempt = StoredRenderAttempt {
                job_id: intent.job_id.clone(),
                attempt_id: AttemptId::new(format!("{name}-{ordinal}"))?,
                cancellation_token: CancellationToken::new(format!("token-{name}-{ordinal}"))?,
                ordinal,
                transition_sequence: 2,
                state: RenderAttemptState::Failed,
                checkpoint_attempt_id: None,
                cancellation_requested: false,
                verification: None,
                diagnostic: Some(RenderDiagnostic {
                    code: "Deadline".into(),
                    detail: "Controlled admission deadline".into(),
                }),
            };
            // Only the measured output shape is reused. These are explicit
            // interrupted-admission declarations with no fake probe success.
            let mut decision = measured.clone();
            decision.job_id = intent.job_id.clone();
            decision.encoding_attempt_id = attempt.attempt_id.clone();
            decision.document_sha256 = intent.document_sha256.clone();
            decision.output.project_id = intent.project_id.clone();
            decision.output.revision_id = intent.revision_id.clone();
            decision.runtime = None;
            decision.probes.clear();
            decision.outcome = RenderDecisionOutcome::Aborted {
                failure: RenderAdmissionFailure {
                    kind: RenderAdmissionFailureKind::Deadline,
                    diagnostic: deadpan_jobs::Diagnostic::new("Controlled admission deadline")?,
                },
            };
            decision.validate_for(&intent, &attempt.attempt_id)?;
            connection.execute("INSERT INTO render_attempts(job_id,attempt_id,ordinal,cancellation_token,state,transition_sequence,body) VALUES(?1,?2,?3,?4,'failed',2,?5)", params![name, attempt.attempt_id.as_str(), i64::try_from(ordinal)?, attempt.cancellation_token.as_str(), serde_json::to_string(&attempt)?])?;
            connection.execute(
                "INSERT INTO render_encoding_decisions(job_id,attempt_id,body) VALUES(?1,?2,?3)",
                params![
                    name,
                    attempt.attempt_id.as_str(),
                    serde_json::to_string(&decision)?
                ],
            )?;
        }
        connection.execute(
            "INSERT INTO render_job_heads(job_id,high_water,latest_attempt_id) VALUES(?1,?2,?3)",
            params![
                name,
                i64::try_from(attempts_per_job)?,
                format!("{name}-{attempts_per_job}")
            ],
        )?;
    }
    Ok(connection)
}

#[test]
fn render_audit_reads_each_job_and_revision_once_as_decision_history_grows() -> Result {
    for attempts in [2, 32] {
        let connection = automatic_history(attempts)?;
        assert_eq!(audit_reads(&connection)?, (3, 2));

        // Targeted reads still inspect the selected job's allocation head.
        connection.execute(
            "UPDATE render_job_heads SET high_water=high_water+1 WHERE job_id='automatic-a'",
            [],
        )?;
        let error = read_attempt(
            &connection,
            &RequestId::new("automatic-a")?,
            &AttemptId::new("automatic-a-1")?,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("allocation head is inconsistent")
        );
        connection.execute(
            "UPDATE render_job_heads SET high_water=high_water-1 WHERE job_id='automatic-a'",
            [],
        )?;

        // A well-formed decision with another legal canvas must still fail the
        // immutable document check after the cached basis has been reused.
        let body: String = connection.query_row(
            "SELECT body FROM render_encoding_decisions WHERE attempt_id='automatic-c-1'",
            [],
            |row| row.get(0),
        )?;
        let mut decision = RenderEncodingDecision::from_json(body.as_bytes())?;
        decision.output.canvas[0] += 2;
        decision.output.raster[0] += 2;
        decision.validate()?;
        connection.execute(
            "UPDATE render_encoding_decisions SET body=?1 WHERE attempt_id='automatic-c-1'",
            [serde_json::to_string(&decision)?],
        )?;
        let error = validate_store(&connection).unwrap_err();
        assert!(error.to_string().contains("immutable document basis"));
    }
    Ok(())
}
