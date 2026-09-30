use super::*;
use std::io::Write;

use deadpan_cli::encoded_render::{
    publication::{PublicationOutcome, PublicationStage, publish},
    verification::VerifiedCandidate,
};

fn verified(fixture: &Fixture) -> VerifiedCandidate {
    verify(
        &native(),
        fixture.original_candidate(),
        request("publication"),
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .unwrap()
}

fn export_directory(fixture: &Fixture) -> PathBuf {
    let directory = fixture.scratch.path().join("exports");
    fs::create_dir(&directory).unwrap();
    directory
}

fn assert_retained(candidate: &mut VerifiedCandidate, fixture: &Fixture) {
    let mut bytes = Vec::new();
    candidate
        .copy_to(&mut bytes, &NOT_CANCELLED, deadline())
        .unwrap();
    assert_eq!(bytes, fs::read(&fixture.movie_path).unwrap());
}

fn report_path(directory: &Path) -> PathBuf {
    let paths: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    assert_eq!(paths.len(), 1);
    paths[0].clone()
}

#[test]
fn publishes_identical_movie_after_durable_bound_report_without_editing_project() {
    let fixture = Fixture::new("nonzero");
    let candidate = verified(&fixture);
    let expected = candidate.report().clone();
    let directory = export_directory(&fixture);
    let destination = directory.join("result.mp4");
    let mut observed_ready = false;
    let outcome = publish(
        candidate,
        &fixture.package,
        &destination,
        &NOT_CANCELLED,
        deadline(),
        |stage| {
            if stage == PublicationStage::ReadyToPublish {
                observed_ready = true;
                assert!(!destination.exists());
                let report: serde_json::Value =
                    serde_json::from_slice(&fs::read(report_path(&directory)).unwrap()).unwrap();
                assert_eq!(
                    report["scope"],
                    "verified_candidate_prepared_for_atomic_publication"
                );
                assert_eq!(
                    report["destination_readback"]["sha256"],
                    expected.movie_sha256.as_str()
                );
            }
        },
    )
    .unwrap();
    assert!(observed_ready);
    let PublicationOutcome::Published(receipt) = outcome else {
        panic!("real local filesystem must confirm durability")
    };
    assert_eq!(
        fs::read(&receipt.movie).unwrap(),
        fs::read(&fixture.movie_path).unwrap()
    );
    assert_eq!(receipt.movie_sha256, expected.movie_sha256);
    assert_eq!(receipt.movie_bytes, expected.movie_bytes);
    assert!(!receipt.contains_generated_pictures);
    let report = fs::read(&receipt.report).unwrap();
    assert_eq!(report.len() as u64, receipt.report_bytes);
    assert_eq!(
        Sha256::digest(&report)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        receipt.report_sha256.as_str()
    );
    assert!(
        fs::read_dir(&directory)
            .unwrap()
            .all(|entry| entry.unwrap().path().extension().unwrap() != "partial")
    );
    fixture.assert_project_unchanged();
}

#[test]
fn existing_and_competing_destinations_are_preserved_with_retryable_candidate() {
    for competing in [false, true] {
        let fixture = Fixture::new("nonzero");
        let directory = export_directory(&fixture);
        let destination = directory.join("existing.mp4");
        let prior = b"unrelated user file";
        if !competing {
            fs::write(&destination, prior).unwrap();
        }
        let mut failure = publish(
            verified(&fixture),
            &fixture.package,
            &destination,
            &NOT_CANCELLED,
            deadline(),
            |stage| {
                if competing && stage == PublicationStage::ReadyToPublish {
                    File::create_new(&destination)
                        .unwrap()
                        .write_all(prior)
                        .unwrap();
                }
            },
        )
        .unwrap_err();
        assert_eq!(failure.error.code, "destination_exists");
        assert_eq!(fs::read(&destination).unwrap(), prior);
        assert_retained(&mut failure.candidate, &fixture);
        let retry = publish(
            failure.candidate,
            &fixture.package,
            &directory.join("retry.mp4"),
            &NOT_CANCELLED,
            deadline(),
            |_| {},
        )
        .unwrap();
        assert!(matches!(retry, PublicationOutcome::Published(_)));
        assert_eq!(fs::read(destination).unwrap(), prior);
        fixture.assert_project_unchanged();
    }
}

#[test]
fn cancellation_before_movie_rename_never_publishes_movie_and_retains_complete_candidate() {
    let fixture = Fixture::new("nonzero");
    let directory = export_directory(&fixture);
    let mut candidate = verified(&fixture);
    for (index, phase) in [
        PublicationStage::CapturingProvenance,
        PublicationStage::CopyingDestination,
        PublicationStage::CheckingDestination,
        PublicationStage::WritingReport,
        PublicationStage::ReadyToPublish,
    ]
    .into_iter()
    .enumerate()
    {
        let cancelled = AtomicBool::new(false);
        let destination = directory.join(format!("cancel-{index}.mp4"));
        let mut failure = publish(
            candidate,
            &fixture.package,
            &destination,
            &cancelled,
            deadline(),
            |stage| {
                if stage == phase {
                    cancelled.store(true, Ordering::Release);
                }
            },
        )
        .unwrap_err();
        assert!(!destination.exists());
        assert_eq!(
            failure.error.code, "cancelled",
            "{phase:?}: {}",
            failure.error
        );
        assert_retained(&mut failure.candidate, &fixture);
        if phase == PublicationStage::ReadyToPublish {
            assert!(
                failure
                    .retained
                    .published_report
                    .as_ref()
                    .unwrap()
                    .is_file()
            );
            assert_eq!(
                fs::read(failure.retained.partial_movie.as_ref().unwrap()).unwrap(),
                fs::read(&fixture.movie_path).unwrap()
            );
        }
        candidate = failure.candidate;
    }
    fixture.assert_project_unchanged();
}

#[test]
fn destination_or_report_mutation_cannot_publish_a_successful_movie() {
    for mutate_report in [false, true] {
        let fixture = Fixture::new("nonzero");
        let directory = export_directory(&fixture);
        let destination = directory.join("mutated.mp4");
        let phase = if mutate_report {
            PublicationStage::ReadyToPublish
        } else {
            PublicationStage::CheckingDestination
        };
        let mut failure = publish(
            verified(&fixture),
            &fixture.package,
            &destination,
            &NOT_CANCELLED,
            deadline(),
            |stage| {
                if stage == phase {
                    let path = if mutate_report {
                        report_path(&directory)
                    } else {
                        fs::read_dir(&directory)
                            .unwrap()
                            .map(|entry| entry.unwrap().path())
                            .find(|path| {
                                path.extension()
                                    .is_some_and(|extension| extension == "partial")
                            })
                            .unwrap()
                    };
                    let mut bytes = fs::read(&path).unwrap();
                    bytes[0] ^= 1;
                    fs::write(path, bytes).unwrap();
                }
            },
        )
        .unwrap_err();
        assert_eq!(failure.error.code, "destination_changed");
        assert!(!destination.exists());
        assert_retained(&mut failure.candidate, &fixture);
        fixture.assert_project_unchanged();
    }
}
