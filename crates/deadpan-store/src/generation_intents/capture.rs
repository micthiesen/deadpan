use super::*;
use std::cell::Cell;

use deadpan_plan::{DefinitionPictureSpan, Picture, PlanError, RenderPlan};

use crate::generation_inputs::{GenerationCaptureSpec, InputCaptureBudget};
use crate::generation_pictures::{
    GenerationPictureIdentity, GenerationPictureSupport, GenerationPictures,
    QualifiedGenerationPictures,
};

pub(crate) struct CapturedInputs {
    pub capture: Option<GenerationCaptureSpec>,
    pub binding: IntentInputBinding,
}

impl CapturedInputs {
    pub fn matches(&self, receipt: &IntentBirthReceipt) -> bool {
        self.capture == receipt.capture && self.binding.same_authority(&receipt.input_binding)
    }
}

/// One compiled plan, measured receipt cache and work ledger per transition.
/// An unavailable input never substitutes an assumed frame or provider.
pub(crate) fn capture_inputs(
    connection: &Connection,
    document: &ProjectDocument,
    births: &[crate::generation_preparations::Birth],
) -> Result<Vec<CapturedInputs>, StoreError> {
    if births.is_empty() {
        return Ok(Vec::new());
    }
    let plan = RenderPlan::compile(document).map_err(|error| invalid(&error.to_string()))?;
    let pictures = IntentPictures::new(connection);
    let mut budget = InputCaptureBudget::default();
    births
        .iter()
        .map(|birth| {
            capture_one(
                connection,
                document,
                &plan,
                &birth.target,
                &birth.origin,
                birth.capture,
                &pictures,
                &mut budget,
            )
        })
        .collect()
}

/// Load each immutable birth individually so a wide set of heads does not
/// retain a second unbounded collection of temporal input descriptors.
pub(crate) fn capture_heads(
    connection: &Connection,
    document: &ProjectDocument,
    heads: &[IntentHead],
) -> Result<Vec<CapturedInputs>, StoreError> {
    if heads.is_empty() {
        return Ok(Vec::new());
    }
    let plan = RenderPlan::compile(document).map_err(|error| invalid(&error.to_string()))?;
    let pictures = IntentPictures::new(connection);
    let mut budget = InputCaptureBudget::default();
    heads
        .iter()
        .map(|head| {
            let birth = read_birth(connection, &head.activation_id)?;
            capture_one(
                connection,
                document,
                &plan,
                &head.target,
                &birth.origin,
                birth.receipt.capture,
                &pictures,
                &mut budget,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn capture_one(
    connection: &Connection,
    document: &ProjectDocument,
    plan: &RenderPlan,
    target: &ScopedNodeTarget,
    origin: &PreparationOrigin,
    captured: Option<GenerationCaptureSpec>,
    pictures: &IntentPictures<'_>,
    budget: &mut InputCaptureBudget,
) -> Result<CapturedInputs, StoreError> {
    let retained = if origin.options().is_none() {
        origin
            .accepted_artifact()
            .map(|artifact| crate::generation_origins::read(connection, artifact))
            .transpose()?
            .flatten()
    } else {
        None
    };
    let Some(options) = origin
        .options()
        .or_else(|| retained.as_ref().map(|receipt| receipt.options()))
    else {
        return Ok(unavailable(
            captured,
            InputUnavailableCause::InvalidRetainedEvidence,
            "Accepted generation controls have no retained origin receipt.".into(),
        ));
    };
    let capture = if let Some(capture) = captured.or_else(|| {
        retained
            .as_ref()
            .map(|receipt| receipt.input_binding().capture_spec())
    }) {
        if options
            .mode
            .validate_resolved(capture.conditioning())
            .is_err()
        {
            return Err(invalid(
                "captured operation differs from its immutable generation controls",
            ));
        }
        capture
    } else {
        match budget.resolve_capture(plan, target, options.mode) {
            Ok(capture) => capture,
            Err(error) => return capture_error(None, error, None),
        }
    };
    pictures.cause.set(None);
    let region = options.region_target.resolve(None);
    if matches!(
        options.region_target,
        deadpan_jobs::GenerationTarget::Inherit
    ) {
        return Err(invalid(
            "immutable generation intent has unresolved region controls",
        ));
    }
    match GenerationInputBinding::capture_with_plan(
        document,
        plan,
        target,
        capture,
        region.as_ref(),
        pictures,
        budget,
    ) {
        Ok(binding) => Ok(CapturedInputs {
            capture: Some(capture),
            binding: IntentInputBinding::Measured {
                binding: Box::new(binding),
            },
        }),
        Err(error) => capture_error(Some(capture), error, pictures.cause.get()),
    }
}

fn capture_error(
    capture: Option<GenerationCaptureSpec>,
    error: StoreError,
    cause: Option<InputUnavailableCause>,
) -> Result<CapturedInputs, StoreError> {
    // Aggregate resource exhaustion depends on batch order and cannot become
    // durable input identity. Fail the transaction without changing any head;
    // a later smaller capture may succeed without contradicting stored history.
    if matches!(
        &error,
        StoreError::GenerationInputLimit(_)
            | StoreError::GenerationInputQuery(PlanError::PictureQueryLimit(_))
    ) {
        return Err(error);
    }
    Ok(unavailable(
        capture,
        cause.unwrap_or_else(|| query_cause(&error)),
        error.to_string(),
    ))
}

fn unavailable(
    capture: Option<GenerationCaptureSpec>,
    cause: InputUnavailableCause,
    detail: String,
) -> CapturedInputs {
    CapturedInputs {
        capture,
        binding: IntentInputBinding::Unavailable {
            cause,
            detail: detail.chars().take(512).collect(),
        },
    }
}

fn query_cause(error: &StoreError) -> InputUnavailableCause {
    match error {
        StoreError::GenerationInputQuery(PlanError::PictureQueryLimit(_))
        | StoreError::GenerationInputLimit(_) => InputUnavailableCause::QueryLimit,
        StoreError::GenerationInputQuery(PlanError::HoldContextUnavailable(_))
        | StoreError::GenerationInputMode(_) => InputUnavailableCause::MissingContext,
        _ => InputUnavailableCause::InvalidRetainedEvidence,
    }
}

struct IntentPictures<'a> {
    measured: QualifiedGenerationPictures<'a>,
    cause: Cell<Option<InputUnavailableCause>>,
}

impl<'a> IntentPictures<'a> {
    fn new(connection: &'a Connection) -> Self {
        Self {
            measured: QualifiedGenerationPictures::new(connection),
            cause: Cell::new(None),
        }
    }
    fn preflight(&self, document: &ProjectDocument, picture: &Picture) -> Result<(), StoreError> {
        let cause = match picture {
            Picture::Still { .. }
            | Picture::Accepted {
                generated: None, ..
            } => Some(InputUnavailableCause::UnsupportedPicture),
            Picture::Source { asset, .. } | Picture::Freeze { asset, .. }
                if document
                    .assets()
                    .get(asset)
                    .is_none_or(|asset| asset.source_qualification.is_none()) =>
            {
                Some(InputUnavailableCause::MissingQualification)
            }
            _ => None,
        };
        if let Some(cause) = cause {
            self.cause.set(Some(cause));
            return Err(invalid(
                "Generation context has an unsupported or unqualified picture.",
            ));
        }
        Ok(())
    }
}

impl GenerationPictures for IntentPictures<'_> {
    fn identity(
        &self,
        document: &ProjectDocument,
        picture: &Picture,
    ) -> Result<GenerationPictureIdentity, StoreError> {
        self.preflight(document, picture)?;
        self.measured.identity(document, picture)
    }
    fn support(
        &self,
        document: &ProjectDocument,
        span: &DefinitionPictureSpan,
    ) -> Result<GenerationPictureSupport, StoreError> {
        self.preflight(document, &span.start.picture)?;
        self.measured.support(document, span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_exhaustion_never_becomes_durable_unavailable_input_authority() {
        let scratch = tempfile::tempdir().unwrap();
        let package = scratch.path().join("capture-budget.deadpan");
        let mut store = ProjectStore::create(
            &package,
            &super::super::extension_tests::document(24).unwrap(),
        )
        .unwrap();
        let preparation = super::super::extension_tests::insert(&mut store, 12).unwrap();
        let IntentInputBinding::Measured { binding } = &preparation.intent.input_binding else {
            panic!("measured black inputs")
        };
        let bytes = serde_json::to_vec(binding).unwrap().len();
        let document = store.snapshot().unwrap();
        let plan = RenderPlan::compile(&document).unwrap();
        let pictures = IntentPictures::new(&store.connection);
        let mut budget = InputCaptureBudget::with_test_byte_limit(bytes - 1);
        let result = capture_one(
            &store.connection,
            &document,
            &plan,
            &preparation.target,
            &preparation.origin,
            preparation.intent.capture,
            &pictures,
            &mut budget,
        );
        assert!(matches!(result, Err(StoreError::GenerationInputLimit(_))));
        for error in [
            StoreError::GenerationInputLimit("receipt bytes"),
            StoreError::GenerationInputQuery(PlanError::PictureQueryLimit("nodes")),
        ] {
            assert!(capture_error(preparation.intent.capture, error, None).is_err());
        }
        let complete = capture_heads(
            &store.connection,
            &document,
            &heads(&store.connection).unwrap(),
        )
        .unwrap();
        assert_eq!(complete.len(), 1);
        assert!(complete[0].matches(&preparation.intent));
        assert_eq!(
            store
                .generation_preparation(&preparation.id)
                .unwrap()
                .unwrap(),
            preparation
        );
        store.validate_full().unwrap();
    }

    #[test]
    fn explicit_operations_stay_captured_while_their_anchors_are_absent() {
        use deadpan_jobs::GenerationModePreference;
        for preference in [
            GenerationModePreference::Bridge,
            GenerationModePreference::ExtendFromLeft,
            GenerationModePreference::ExtendFromRight,
        ] {
            let capture = GenerationCaptureSpec::for_preference(preference, false, false).unwrap();
            assert!(preference.validate_resolved(capture.conditioning()).is_ok());
        }
        assert!(matches!(
            GenerationCaptureSpec::for_preference(
                GenerationModePreference::Automatic,
                false,
                false
            ),
            Err(StoreError::GenerationInputMode(
                deadpan_jobs::GenerationModeError::NoEndpoints
            ))
        ));
    }
}
