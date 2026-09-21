#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::fs;
use std::io::Read;
use std::os::unix::net::UnixListener;

use deadpan_jobs::artifact::{
    ArtifactError, ArtifactLimits, ArtifactWorkspace, SnapshotInterruption,
};
use deadpan_jobs::{Sha256, WorkspaceArtifact, WorkspaceRef};
use sha2::{Digest, Sha256 as Sha256Hasher};

fn hash(bytes: &[u8]) -> Sha256 {
    let digest = Sha256Hasher::digest(bytes);
    let mut encoded = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Sha256::new(encoded).unwrap()
}

fn reference(value: &str) -> WorkspaceRef {
    WorkspaceRef::new(value).unwrap()
}

fn declaration(path: &str, bytes: &[u8]) -> WorkspaceArtifact {
    WorkspaceArtifact::new(reference(path), hash(bytes), bytes.len() as u64).unwrap()
}

fn limits(maximum: u64) -> ArtifactLimits {
    ArtifactLimits::new(maximum).unwrap()
}

fn create_fifo(path: &std::path::Path) {
    #[cfg(target_os = "linux")]
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    #[cfg(target_os = "macos")]
    assert!(
        std::process::Command::new("/usr/bin/mkfifo")
            .arg(path)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn valid_artifact_is_frozen_and_later_source_changes_do_not_reach_snapshot() {
    let scratch = tempfile::tempdir().unwrap();
    let output = scratch.path().join("work/job-1");
    fs::create_dir_all(&output).unwrap();
    let path = output.join("candidate.bin");
    let original = b"validated candidate bytes";
    fs::write(&path, original).unwrap();

    let workspace = ArtifactWorkspace::open(scratch.path()).unwrap();
    let declared = declaration("work/job-1/candidate.bin", original);
    let mut snapshot = workspace
        .snapshot(&reference("work/job-1"), &declared, limits(1_024))
        .unwrap();
    assert_eq!(snapshot.declaration(), &declared);

    fs::write(path, b"replacement after validation").unwrap();
    let mut bytes = Vec::new();
    snapshot.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, original);
}

#[test]
fn sha256_matches_the_standard_abc_known_answer() {
    let scratch = tempfile::tempdir().unwrap();
    let output = scratch.path().join("output");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("abc.bin"), b"abc").unwrap();
    let declared = WorkspaceArtifact::new(
        reference("output/abc.bin"),
        Sha256::new("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad").unwrap(),
        3,
    )
    .unwrap();
    ArtifactWorkspace::open(scratch.path())
        .unwrap()
        .snapshot(&reference("output"), &declared, limits(3))
        .unwrap();
}

#[test]
fn output_scope_is_component_aware_and_strict() {
    let scratch = tempfile::tempdir().unwrap();
    let workspace = ArtifactWorkspace::open(scratch.path()).unwrap();
    let scope = reference("work/job-1");
    for path in [
        "work/job-1",
        "work/job-10/candidate.bin",
        "inputs/context.bin",
    ] {
        let declared = declaration(path, b"x");
        assert!(matches!(
            workspace.snapshot(&scope, &declared, limits(8)),
            Err(ArtifactError::OutsideOutputScope { .. })
        ));
    }
}

#[test]
fn controlled_snapshot_reports_typed_interruption_before_filesystem_access() {
    let scratch = tempfile::tempdir().unwrap();
    let workspace = ArtifactWorkspace::open(scratch.path()).unwrap();
    let declared = declaration("output/missing.bin", b"never read");
    for interruption in [
        SnapshotInterruption::Cancelled,
        SnapshotInterruption::Deadline,
    ] {
        assert!(matches!(
            workspace.snapshot_with_control(
                &reference("output"),
                &declared,
                limits(64),
                || Err(interruption),
            ),
            Err(ArtifactError::Interrupted(reason)) if reason == interruption
        ));
    }
}

#[test]
fn final_and_intermediate_symlinks_are_rejected() {
    let scratch = tempfile::tempdir().unwrap();
    let outside = scratch.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("candidate.bin"), b"outside").unwrap();
    let output = scratch.path().join("output");
    fs::create_dir(&output).unwrap();
    std::os::unix::fs::symlink(outside.join("candidate.bin"), output.join("final-link.bin"))
        .unwrap();
    std::os::unix::fs::symlink(&outside, scratch.path().join("directory-link")).unwrap();
    let workspace = ArtifactWorkspace::open(scratch.path()).unwrap();

    let final_link = declaration("output/final-link.bin", b"outside");
    assert!(matches!(
        workspace.snapshot(&reference("output"), &final_link, limits(64)),
        Err(ArtifactError::UnsafeComponent(_))
    ));
    let intermediate = declaration("directory-link/candidate.bin", b"outside");
    assert!(matches!(
        workspace.snapshot(&reference("directory-link"), &intermediate, limits(64)),
        Err(ArtifactError::UnsafeComponent(_))
    ));

    let root_link = scratch.path().join("root-link");
    std::os::unix::fs::symlink(scratch.path(), &root_link).unwrap();
    assert!(matches!(
        ArtifactWorkspace::open(&root_link),
        Err(ArtifactError::UnsafeWorkspace)
    ));
}

#[test]
fn hardlinks_inside_or_outside_the_workspace_are_rejected() {
    let scratch = tempfile::tempdir().unwrap();
    let workspace_root = scratch.path().join("workspace");
    let output = workspace_root.join("output");
    fs::create_dir_all(&output).unwrap();
    let outside = scratch.path().join("outside.bin");
    fs::write(&outside, b"linked").unwrap();
    fs::hard_link(&outside, output.join("external-link.bin")).unwrap();
    fs::write(output.join("internal.bin"), b"internal").unwrap();
    fs::hard_link(
        output.join("internal.bin"),
        output.join("internal-alias.bin"),
    )
    .unwrap();
    let workspace = ArtifactWorkspace::open(&workspace_root).unwrap();

    for declared in [
        declaration("output/external-link.bin", b"linked"),
        declaration("output/internal.bin", b"internal"),
    ] {
        assert!(matches!(
            workspace.snapshot(&reference("output"), &declared, limits(64)),
            Err(ArtifactError::MultipleLinks(_))
        ));
    }
}

#[test]
fn directories_fifos_and_sockets_are_rejected_without_blocking() {
    let scratch = tempfile::tempdir().unwrap();
    let output = scratch.path().join("output");
    fs::create_dir(&output).unwrap();
    fs::create_dir(output.join("directory")).unwrap();
    create_fifo(&output.join("fifo"));
    let _socket = UnixListener::bind(output.join("socket")).unwrap();
    let workspace = ArtifactWorkspace::open(scratch.path()).unwrap();

    for name in ["directory", "fifo", "socket"] {
        let declared = declaration(&format!("output/{name}"), b"x");
        assert!(matches!(
            workspace.snapshot(&reference("output"), &declared, limits(64)),
            Err(ArtifactError::NotRegularFile(_))
        ));
    }
}

#[test]
fn hash_length_and_budget_failures_are_distinct() {
    let scratch = tempfile::tempdir().unwrap();
    let output = scratch.path().join("output");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("candidate.bin"), b"actual").unwrap();
    let workspace = ArtifactWorkspace::open(scratch.path()).unwrap();
    let scope = reference("output");

    let wrong_hash =
        WorkspaceArtifact::new(reference("output/candidate.bin"), hash(b"other!"), 6).unwrap();
    assert!(matches!(
        workspace.snapshot(&scope, &wrong_hash, limits(64)),
        Err(ArtifactError::HashMismatch { .. })
    ));

    let wrong_length =
        WorkspaceArtifact::new(reference("output/candidate.bin"), hash(b"actual"), 5).unwrap();
    assert!(matches!(
        workspace.snapshot(&scope, &wrong_length, limits(64)),
        Err(ArtifactError::LengthMismatch {
            declared: 5,
            actual: 6
        })
    ));

    let valid = declaration("output/candidate.bin", b"actual");
    assert!(matches!(
        workspace.snapshot(&scope, &valid, limits(5)),
        Err(ArtifactError::TooLarge {
            size: 6,
            maximum: 5
        })
    ));
    assert!(matches!(
        ArtifactLimits::new(0),
        Err(ArtifactError::InvalidBudget)
    ));
}

#[test]
fn retained_workspace_descriptor_ignores_later_root_path_replacement() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("workspace");
    let original_output = root.join("output");
    fs::create_dir_all(&original_output).unwrap();
    fs::write(original_output.join("candidate.bin"), b"original").unwrap();
    let workspace = ArtifactWorkspace::open(&root).unwrap();

    let parked = scratch.path().join("parked");
    fs::rename(&root, &parked).unwrap();
    fs::create_dir_all(root.join("output")).unwrap();
    fs::write(root.join("output/candidate.bin"), b"replacement").unwrap();

    let declared = declaration("output/candidate.bin", b"original");
    let mut snapshot = workspace
        .snapshot(&reference("output"), &declared, limits(64))
        .unwrap();
    let mut bytes = Vec::new();
    snapshot.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"original");
}
