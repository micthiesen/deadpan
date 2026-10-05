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

/// A private filled APFS image, detached on drop.
#[cfg(target_os = "macos")]
struct FullVolume {
    mount: PathBuf,
    filler: PathBuf,
    _scratch: tempfile::TempDir,
}

#[cfg(target_os = "macos")]
impl FullVolume {
    fn new() -> Self {
        use std::process::Command;
        let scratch = tempfile::tempdir().unwrap();
        let image = scratch.path().join("volume.dmg");
        let mount = scratch.path().join("mount");
        fs::create_dir(&mount).unwrap();
        let created = Command::new("hdiutil")
            .args([
                "create", "-quiet", "-size", "16m", "-fs", "APFS", "-layout", "NONE",
            ])
            .args(["-volname", "deadpan-export"])
            .arg(&image)
            .output()
            .unwrap();
        assert!(created.status.success(), "{created:?}");
        let attached = Command::new("hdiutil")
            .args([
                "attach",
                "-quiet",
                "-nobrowse",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(&mount)
            .arg(&image)
            .output()
            .unwrap();
        assert!(attached.status.success(), "{attached:?}");
        let mount = mount.canonicalize().unwrap();
        let filler = mount.join("filler");
        let mut file = File::create(&filler).unwrap();
        // APFS can release space shortly after a write fails; keep filling
        // until a new 4 KiB file is refused.
        let mut full = false;
        // APFS can keep releasing space under load; give it time to settle.
        for round in 0..64 {
            if round > 0 {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            for chunk in [1 << 20, 64 << 10, 4 << 10, 512] {
                let bytes = vec![0x5a_u8; chunk];
                while file
                    .write_all(&bytes)
                    .and_then(|()| file.sync_data())
                    .is_ok()
                {}
            }
            let probe = mount.join("probe");
            let refused = File::create(&probe)
                .and_then(|mut probe| probe.write_all(&[0; 4096]).and_then(|()| probe.sync_all()));
            let _ = fs::remove_file(&probe);
            if matches!(&refused, Err(error) if error.kind() == std::io::ErrorKind::StorageFull) {
                full = true;
                break;
            }
        }
        assert!(full, "the volume never stayed full");
        Self {
            mount,
            filler,
            _scratch: scratch,
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for FullVolume {
    fn drop(&mut self) {
        let _ = std::process::Command::new("hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&self.mount)
            .output();
    }
}

/// Real ENOSPC at the destination: no movie appears, nothing reads as
/// published, and the verified candidate publishes once space returns.
#[cfg(target_os = "macos")]
#[test]
fn a_full_destination_volume_publishes_nothing_and_keeps_the_candidate() {
    let fixture = Fixture::new("nonzero");
    let volume = FullVolume::new();
    let destination = volume.mount.join("result.mp4");
    let mut failure = publish(
        verified(&fixture),
        &fixture.package,
        &destination,
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .unwrap_err();
    assert!(!destination.exists(), "{}", failure.error);
    assert_eq!(failure.error.code, "destination_full", "{}", failure.error);
    assert_retained(&mut failure.candidate, &fixture);
    fixture.assert_project_unchanged();
    fs::remove_file(&volume.filler).unwrap();
    let retry = publish(
        failure.candidate,
        &fixture.package,
        &destination,
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .unwrap();
    assert!(matches!(retry, PublicationOutcome::Published(_)));
    assert_eq!(
        fs::read(&destination).unwrap(),
        fs::read(&fixture.movie_path).unwrap()
    );
}
