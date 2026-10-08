use super::*;

#[test]
fn replacement_feedback_distinguishes_queued_work_from_unavailable_and_cancelled_work() {
    assert_eq!(
        replacement_preparation_status(PreparationState::Queued, None),
        "Fresh AI preparation queued."
    );
    let unavailable = replacement_preparation_status(
        PreparationState::Unavailable,
        Some("The required boundary picture is unavailable."),
    );
    assert!(unavailable.contains("Fresh AI preparation is unavailable."));
    assert!(unavailable.contains("The required boundary picture is unavailable."));
    assert!(!unavailable.contains("queued"));
    let cancelled = replacement_preparation_status(
        PreparationState::Cancelled,
        Some("The bounded replacement queue filled."),
    );
    assert!(cancelled.contains("was cancelled"));
    assert!(cancelled.contains("The bounded replacement queue filled."));
    assert!(!cancelled.contains("queued"));
}

#[test]
fn completed_preparation_does_not_claim_generated_pictures_were_accepted() {
    let message = replacement_preparation_status(PreparationState::Fulfilled, None);
    assert!(message.contains("still need explicit acceptance"));
    assert!(!message.contains("queued"));
}
