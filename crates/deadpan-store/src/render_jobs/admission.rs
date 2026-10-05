//! Immutable observations of automatic encoder admission. Stored declarations
//! never prove worker cleanup or recreate a live encoder capability.
use super::*;
use deadpan_jobs::render::admission::{MAX_RENDER_DECISION_BYTES, RenderEncodingDecision};

pub(super) const MAX_DECISION_BYTES: i64 = MAX_RENDER_DECISION_BYTES as i64;

pub(crate) fn create_decision_table(connection: &Connection) -> Result<(), StoreError> {
    // No IF NOT EXISTS: a legacy database cannot introduce this vocabulary.
    connection.execute_batch(
        "CREATE TABLE render_encoding_decisions (
        job_id TEXT NOT NULL, attempt_id TEXT PRIMARY KEY,
        body TEXT NOT NULL CHECK(json_valid(body)),
        FOREIGN KEY(job_id,attempt_id) REFERENCES render_attempts(job_id,attempt_id)
    ) STRICT;",
    )?;
    Ok(())
}

impl ProjectStore {
    /// Resolve one original encoding owner's immutable observation. Verification
    /// retries have no decision of their own; use their checkpoint owner ID.
    pub fn render_encoding_decision(
        &self,
        job: &RequestId,
        encoding_attempt: &AttemptId,
    ) -> Result<Option<RenderEncodingDecision>, StoreError> {
        validate_runtime(&self.connection)?;
        let intent = read_job(&self.connection, job)?;
        read_attempt(&self.connection, job, encoding_attempt)?;
        read_decision(&self.connection, &intent, encoding_attempt)
    }

    /// Commit the selected observation and Queued -> Encoding together. The
    /// caller retains the separate fresh execution capability until this commits.
    pub fn begin_render_encoding(
        &mut self,
        identity: &RenderAttemptIdentity,
        decision: &RenderEncodingDecision,
    ) -> Result<StoredRenderAttempt, StoreError> {
        self.require_writer()?;
        decision.validate().map_err(pure)?;
        let body = encode(decision, MAX_DECISION_BYTES)?;
        if !decision.is_selected() {
            return Err(invalid("encoding requires a selected admission decision"));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_runtime(&transaction)?;
        let mut attempt = bound_attempt(&transaction, identity)?;
        if attempt.state != RenderAttemptState::Queued || attempt.checkpoint_attempt_id.is_some() {
            return Err(invalid("only a fresh queued attempt can select an encoder"));
        }
        insert_decision(&transaction, &attempt, decision, &body)?;
        advance(&mut attempt, RenderAttemptState::Encoding)?;
        write_attempt(&transaction, &attempt)?;
        transaction.commit()?;
        Ok(attempt)
    }

    /// Retain a returned failed qualification and its terminal transition
    /// atomically. The caller must first stop/reap/drain all owned work, exactly
    /// as for ordinary terminal transitions. No serialized cleanup flag grants
    /// that authority; unresolved cleanup remains nonterminal without a record.
    pub fn finish_render_admission(
        &mut self,
        identity: &RenderAttemptIdentity,
        decision: &RenderEncodingDecision,
        terminal: RenderAttemptTransition,
    ) -> Result<StoredRenderAttempt, StoreError> {
        self.require_writer()?;
        decision.validate().map_err(pure)?;
        let body = encode(decision, MAX_DECISION_BYTES)?;
        if decision.is_selected() || decision.unresolved_cleanup() {
            return Err(invalid(
                "terminal admission requires a resolved failed decision",
            ));
        }
        let (next, diagnostic) = match terminal {
            RenderAttemptTransition::FinishCancelled if decision.cancelled() => {
                (RenderAttemptState::Cancelled, None)
            }
            RenderAttemptTransition::Failed(diagnostic) if !decision.cancelled() => {
                diagnostic.validate().map_err(pure)?;
                (RenderAttemptState::Failed, Some(diagnostic))
            }
            _ => {
                return Err(invalid(
                    "admission outcome differs from terminal transition",
                ));
            }
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_runtime(&transaction)?;
        let mut attempt = bound_attempt(&transaction, identity)?;
        if !matches!(
            attempt.state,
            RenderAttemptState::Queued | RenderAttemptState::Cancelling
        ) || attempt.checkpoint_attempt_id.is_some()
        {
            return Err(invalid(
                "only unfinished qualification can retain a failed decision",
            ));
        }
        insert_decision(&transaction, &attempt, decision, &body)?;
        advance(&mut attempt, next)?;
        attempt.diagnostic = diagnostic;
        write_attempt(&transaction, &attempt)?;
        transaction.commit()?;
        Ok(attempt)
    }
}

fn insert_decision(
    connection: &Connection,
    attempt: &StoredRenderAttempt,
    decision: &RenderEncodingDecision,
    body: &str,
) -> Result<(), StoreError> {
    let intent = read_job(connection, &attempt.job_id)?;
    decision
        .validate_for(&intent, &attempt.attempt_id)
        .map_err(pure)?;
    validate_output_document(connection, &intent, decision)?;
    if read_decision(connection, &intent, &attempt.attempt_id)?.is_some() {
        return Err(invalid("render encoding decision is immutable"));
    }
    connection.execute(
        "INSERT INTO render_encoding_decisions(job_id,attempt_id,body) VALUES(?1,?2,?3)",
        params![attempt.job_id.as_str(), attempt.attempt_id.as_str(), body],
    )?;
    Ok(())
}

fn read_decision(
    connection: &Connection,
    intent: &RenderIntent,
    attempt: &AttemptId,
) -> Result<Option<RenderEncodingDecision>, StoreError> {
    let row: Option<Option<String>> = connection.query_row(
        "SELECT CASE WHEN typeof(body)='text' AND length(CAST(body AS BLOB)) BETWEEN 1 AND ?3 THEN body END FROM render_encoding_decisions WHERE job_id=?1 AND attempt_id=?2",
        params![intent.job_id.as_str(), attempt.as_str(), MAX_DECISION_BYTES],
        |row| row.get(0),
    ).optional()?;
    row.map(|body| {
        let body = required_text(body)?;
        let decision: RenderEncodingDecision = serde_json::from_str(&body)?;
        decision.validate_for(intent, attempt).map_err(pure)?;
        Ok(decision)
    })
    .transpose()
}

pub(super) fn validate_output_document(
    connection: &Connection,
    intent: &RenderIntent,
    decision: &RenderEncodingDecision,
) -> Result<(), StoreError> {
    let document = validation::read_revision(connection, intent.revision_id.as_str())?.document;
    validate_output_basis(document.presentation_basis(), decision)?;
    // Re-derive the branch from the revision's own stored receipts; a stored
    // decision cannot choose a different color policy or mastering volume.
    let color = crate::output_color::committed_output_color(connection, &document)?;
    validate_output_color(
        &color,
        decision.output.color_policy,
        decision.output.mastering_display,
    )
}

fn validate_output_color(
    color: &deadpan_core::OutputColorDecision,
    policy: deadpan_core::ColorPolicy,
    mastering: Option<deadpan_core::MasteringDisplay>,
) -> Result<(), StoreError> {
    if policy != color.output || mastering != color.mastering {
        return Err(invalid(
            "automatic output color or mastering volume differs from the revision's receipts",
        ));
    }
    Ok(())
}

fn validate_output_basis(
    basis: &deadpan_core::PresentationBasis,
    decision: &RenderEncodingDecision,
) -> Result<(), StoreError> {
    if decision.output.canvas != [basis.width, basis.height]
        || decision.output.frame_rate != basis.frame_rate
        || !output_color_admitted(basis.color_policy, decision.output.color_policy)
    {
        return Err(invalid(
            "automatic output differs from its immutable document basis",
        ));
    }
    // validate_for already checks range, exact derived clocks, even raster,
    // aspect, project/revision/document identities and the pinned algorithm.
    Ok(())
}

/// The automatic branch keeps the basis transfer or falls back to tone-mapped
/// SDR from an HDR basis. An SDR basis never yields HDR output, and HDR never
/// changes transfer. Receipt-level branch facts are the host's responsibility.
fn output_color_admitted(
    basis: deadpan_core::ColorPolicy,
    output: deadpan_core::ColorPolicy,
) -> bool {
    output == basis || output == deadpan_core::ColorPolicy::SdrRec709
}

pub(super) fn validate_attempt_decision(
    connection: &Connection,
    attempt: &StoredRenderAttempt,
    intent: &RenderIntent,
) -> Result<(), StoreError> {
    use RenderAttemptState::*;
    let decision = read_decision(connection, intent, &attempt.attempt_id)?;
    if !intent.policy.is_automatic() {
        if decision.is_some() {
            return Err(invalid(
                "engineering attempt cannot own an automatic decision",
            ));
        }
        return Ok(());
    }
    let owns_checkpoint = attempt.checkpoint_attempt_id.as_ref() == Some(&attempt.attempt_id);
    if attempt.checkpoint_attempt_id.is_some() && !owns_checkpoint && decision.is_some() {
        return Err(invalid(
            "verification retry cannot own an encoding decision",
        ));
    }
    match decision {
        Some(decision) if decision.is_selected() => {
            if attempt.state == Queued {
                return Err(invalid("selected decision was not committed with Encoding"));
            }
        }
        Some(decision) => {
            if attempt.checkpoint_attempt_id.is_some()
                || decision.unresolved_cleanup()
                || !matches!(attempt.state, Failed | Cancelled)
                || (attempt.state == Cancelled) != decision.cancelled()
            {
                return Err(invalid("failed decision and terminal attempt disagree"));
            }
        }
        None if attempt.state == Encoding || owns_checkpoint => {
            return Err(invalid("automatic encoding owner has no selected decision"));
        }
        None => {}
    }
    Ok(())
}

pub(super) fn validate_all_outputs(
    connection: &Connection,
    intents: &BTreeMap<RequestId, RenderIntent>,
    documents: &BTreeMap<deadpan_core::RevisionId, AuditRevision>,
) -> Result<(), StoreError> {
    let mut statement =
        connection.prepare("SELECT job_id,attempt_id FROM render_encoding_decisions")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let job =
            RequestId::new(row.get::<_, String>(0)?).map_err(|error| invalid(error.to_string()))?;
        let attempt =
            AttemptId::new(row.get::<_, String>(1)?).map_err(|error| invalid(error.to_string()))?;
        let intent = intents
            .get(&job)
            .ok_or_else(|| invalid("render decision job is missing"))?;
        let revision = documents
            .get(&intent.revision_id)
            .ok_or_else(|| invalid("render decision revision is missing"))?;
        let decision = read_decision(connection, intent, &attempt)?
            .ok_or_else(|| invalid("render decision disappeared during validation"))?;
        validate_output_basis(&revision.basis, &decision)?;
    }
    Ok(())
}

#[cfg(test)]
mod color_tests {
    use super::*;
    use deadpan_core::ColorPolicy::{HdrRec2020Hlg, HdrRec2020Pq, SdrRec709};

    #[test]
    fn output_keeps_the_basis_transfer_or_falls_back_to_sdr_never_upgrades() {
        for (basis, output, admitted) in [
            (SdrRec709, SdrRec709, true),
            (HdrRec2020Pq, HdrRec2020Pq, true),
            (HdrRec2020Hlg, HdrRec2020Hlg, true),
            (HdrRec2020Pq, SdrRec709, true),
            (HdrRec2020Hlg, SdrRec709, true),
            (SdrRec709, HdrRec2020Pq, false),
            (SdrRec709, HdrRec2020Hlg, false),
            (HdrRec2020Pq, HdrRec2020Hlg, false),
            (HdrRec2020Hlg, HdrRec2020Pq, false),
        ] {
            assert_eq!(output_color_admitted(basis, output), admitted);
        }
    }

    #[test]
    fn stored_decisions_must_match_the_receipt_derived_branch() {
        let volume = deadpan_core::MasteringDisplay {
            primaries: [[35_400, 14_600], [8_500, 39_850], [6_550, 2_300]],
            white_point: [15_635, 16_450],
            max_luminance: 10_000_000,
            min_luminance: 50,
        };
        let pq = deadpan_core::OutputColorDecision {
            output: HdrRec2020Pq,
            reason: deadpan_core::OutputColorReason::HdrSources,
            hdr_sources: true,
            mastering: Some(volume),
            ..deadpan_core::OutputColorDecision::sdr()
        };
        assert!(validate_output_color(&pq, HdrRec2020Pq, Some(volume)).is_ok());
        // A forged fallback, a dropped or altered volume all refuse.
        assert!(validate_output_color(&pq, SdrRec709, None).is_err());
        assert!(validate_output_color(&pq, HdrRec2020Pq, None).is_err());
        let altered = deadpan_core::MasteringDisplay {
            max_luminance: 40_000_000,
            ..volume
        };
        assert!(validate_output_color(&pq, HdrRec2020Pq, Some(altered)).is_err());
        // A receipt-derived SDR fallback cannot be stored as HDR.
        let fallback = deadpan_core::OutputColorDecision {
            hdr_sources: true,
            reason: deadpan_core::OutputColorReason::GeneratedPictures,
            ..deadpan_core::OutputColorDecision::sdr()
        };
        assert!(validate_output_color(&fallback, HdrRec2020Pq, Some(volume)).is_err());
        assert!(validate_output_color(&fallback, SdrRec709, None).is_ok());
        let mut decision = RenderEncodingDecision::from_json(include_bytes!(
            "../../../deadpan-jobs/src/render/admission/tests/measured-decision-v1.json"
        ))
        .unwrap();
        let basis = deadpan_core::PresentationBasis {
            width: decision.output.canvas[0],
            height: decision.output.canvas[1],
            frame_rate: decision.output.frame_rate,
            color_policy: HdrRec2020Pq,
        };
        // Tone-mapped SDR fallback from an HDR basis is admitted.
        validate_output_basis(&basis, &decision).unwrap();
        // HDR output from an SDR basis is refused.
        decision.output.color_policy = HdrRec2020Pq;
        let sdr_basis = deadpan_core::PresentationBasis {
            color_policy: SdrRec709,
            ..basis.clone()
        };
        assert!(validate_output_basis(&sdr_basis, &decision).is_err());
        validate_output_basis(&basis, &decision).unwrap();
    }
}
