//! Opt-in real APFS-volume qualification for an offline linked Original and
//! explicit identical-byte relink on another volume. No device is selected
//! outside the two private disk images created under this run's /tmp root.
#![cfg(target_os = "macos")]

#[path = "relink_cross_volume/evidence.rs"]
mod evidence;
#[path = "relink_cross_volume/fixture.rs"]
mod fixture;
#[path = "relink_cross_volume/support.rs"]
mod support;
#[path = "relink_cross_volume/volumes.rs"]
mod volumes;

use serde_json::json;
use std::{fs, os::unix::fs::MetadataExt, path::Path, time::Instant};
use support::{Result, Run, hash, read_json, require, text};
use volumes::Volume;

#[test]
fn an_unmounted_original_relinks_across_volumes_without_changing_authored_history() -> Result {
    let run = Run::new()?;
    eprintln!("cross-volume relink evidence: {}", run.root.display());
    let started = Instant::now();
    let mut report = json!({"schema_version":1,"status":"running","cli":run.cli_hash,"test_binary":run.test_hash,
        "fixture_sha256":fixture::FIXTURE_SHA256,"private_apfs_images":2,"image_size_mib":128,
        "scope":"linked-only single Original; actual unmount; ordinary CLI bookmark recovery and explicit cross-volume relink; decoded frames 0/59/119",
        "limitations":["private APFS disk images on this Mac, not physical removable hardware",
            "store/media/ordinary CLI paths; no native UI, screenshots, physical input or VoiceOver claims",
            "logical authored/history/profile/qualification cells compared; ordinary writer recovery bookkeeping may change"]});
    run.save("report.json", &report)?;
    let result = qualify(&run);
    report["seconds"] = json!(started.elapsed().as_secs_f64());
    match &result {
        Ok(()) => report["status"] = json!("passed"),
        Err(error) => {
            report["status"] = json!("failed");
            report["error"] = json!(error.to_string());
        }
    }
    run.save("report.json", &report)?;
    result
}

fn qualify(run: &Run) -> Result {
    let mut a = Volume::create(run, "A")?;
    let mut b = Volume::create(run, "B")?;
    let result = qualify_relink(run, &a, &b);
    // Both cleanups are attempted even when the measured workflow fails. A
    // passed result requires confirmed detachment of every owned image.
    let cleanup_b = b.finish();
    let cleanup_a = a.finish();
    run.save(
        "cleanup.json",
        &json!({"A":cleanup_a.as_ref().map(|()|"detached").map_err(|error|error.to_string()),
        "B":cleanup_b.as_ref().map(|()|"detached").map_err(|error|error.to_string())}),
    )?;
    let mut errors = Vec::new();
    for outcome in [result, cleanup_a, cleanup_b] {
        if let Err(error) = outcome {
            errors.push(error.to_string());
        }
    }
    require(errors.is_empty(), &errors.join("; "))
}

fn qualify_relink(run: &Run, a: &Volume<'_>, b: &Volume<'_>) -> Result {
    require(
        a.device != b.device,
        "the two test images have the same filesystem device",
    )?;
    let fixture = fixture::fixture_path()?;
    let fixture_hash = hash(&fixture)?;
    require(
        fixture_hash["sha256"] == fixture::FIXTURE_SHA256 && fixture_hash["bytes"] == 39157,
        "cfr-bframes fixture differs from its committed pin",
    )?;
    run.save("fixture.json", &fixture_hash)?;
    fs::create_dir(a.mount.join("Originals"))?;
    let original = a.mount.join("Originals/original-on-A.mp4");
    fs::copy(&fixture, &original)?;
    let original_hash = hash(&original)?;
    require(
        original_hash["sha256"] == fixture_hash["sha256"],
        "volume A copy differs",
    )?;
    let package = run.root.join("linked.deadpan");
    let record = fixture::seed(&package, &original)?;
    let before = evidence::capture(&package)?;
    run.save("before-rows.json", &before)?;
    run.save("original-before.json", &json!(record))?;
    let pictures = fixture::pictures(&package, &record)?;
    run.save("pictures-before.json", &pictures.evidence)?;

    // Every store, decoder, verified input and admission guard created above
    // has been dropped. Only owned pixel vectors and value receipts remain.
    a.detach()?;
    require(
        !original.exists(),
        "Original remains readable after volume A detach",
    )?;
    let offline = fixture::offline(&package, &record)?;
    run.save("offline.json", &offline)?;
    let content = record.object().content().digest();
    let offline_cli = run.cli(
        "verify-offline",
        &["project", "verify-original", text(&package)?, content],
    )?;
    require(
        !offline_cli.status.success(),
        "offline Original passed ordinary CLI verification",
    )?;
    let offline_cli = read_json(&offline_cli.stderr)?;
    require(
        offline_cli["error"]["code"] == "OriginalOffline",
        "CLI lost the readable offline diagnostic",
    )?;
    run.save("offline-cli-result.json", &offline_cli)?;
    evidence::unchanged(run, "offline", &package, &before, true)?;
    a.assert_detached("after-offline-inspection")?;

    let moved = run.cli(
        "relink-moved",
        &["project", "relink-moved", text(&package)?],
    )?;
    require(moved.status.success(), "ordinary relink-moved failed")?;
    let moved = read_json(&moved.stdout)?;
    require(
        moved["relinked_originals"] == json!([]) && moved["refused_candidates"] == json!([]),
        "unmounted bookmark unexpectedly produced a relink candidate",
    )?;
    run.save("relink-moved-result.json", &moved)?;
    a.assert_detached("after-bookmark-resolution")?;
    evidence::unchanged(run, "after-relink-moved", &package, &before, true)?;

    fs::create_dir(b.mount.join("Recovered"))?;
    let destination = b.mount.join("Recovered/renamed-original-on-B.mp4");
    fs::copy(&fixture, &destination)?;
    let destination_hash = hash(&destination)?;
    require(
        destination_hash["sha256"] == fixture_hash["sha256"]
            && fs::metadata(&destination)?.dev() == b.device
            && b.device != a.device,
        "relink destination does not contain the exact source on volume B",
    )?;
    let wrong = b.mount.join("Recovered/wrong-original.mp4");
    let mut wrong_bytes = fs::read(&fixture)?;
    let last = wrong_bytes.last_mut().ok_or("empty fixture")?;
    *last ^= 1;
    fs::write(&wrong, &wrong_bytes)?;
    let wrong_result = run.cli(
        "wrong-bytes",
        &[
            "project",
            "relink-original",
            text(&package)?,
            content,
            text(&wrong)?,
            "--expected-version",
            "1",
        ],
    )?;
    require(
        !wrong_result.status.success(),
        "different source bytes were accepted",
    )?;
    let wrong_result = read_json(&wrong_result.stderr)?;
    require(
        wrong_result["error"]["code"] == "OriginalContentMismatch",
        "wrong-byte candidate has an unexpected refusal",
    )?;
    run.save("wrong-bytes-result.json", &wrong_result)?;
    evidence::unchanged(run, "wrong-bytes", &package, &before, true)?;
    run.save(
        "offline-after-refusal.json",
        &fixture::offline(&package, &record)?,
    )?;
    a.assert_detached("after-wrong-byte-refusal")?;

    let relink = run.cli(
        "relink-original",
        &[
            "project",
            "relink-original",
            text(&package)?,
            content,
            text(&destination)?,
            "--expected-version",
            "1",
        ],
    )?;
    require(
        relink.status.success(),
        "ordinary cross-volume relink failed",
    )?;
    let relink = read_json(&relink.stdout)?;
    let current = fixture::relinked(&package, &record, &destination)?;
    require(
        relink["relinked_original"] == json!(current),
        "CLI receipt differs from reopened Original record",
    )?;
    run.save("relink-result.json", &relink)?;
    evidence::unchanged(run, "after-relink", &package, &before, false)?;
    let restored = fixture::pictures(&package, &current)?;
    run.save("pictures-after.json", &restored.evidence)?;
    require(
        pictures.equals(&restored),
        "relinked decoded frame identities, PTS or pixels differ",
    )?;
    a.assert_detached("after-restored-decodes")?;
    require(
        !original.exists(),
        "relink or restored decoding remounted volume A",
    )?;
    run.save(
        "qualified.json",
        &json!({"package":package,"original_before":record,"original_after":current,
        "source_A":original_hash,"source_B":destination_hash,"wrong_candidate":hash(&wrong)?,
        "original_A_device":a.device,"original_B_device":b.device,"A_still_detached":true,
        "authored_history_profile_qualification_preserved":true,"decoded_frame_bytes_equal":true,
        "frames":[0,59,119],"can_undo":false,"can_redo":true}),
    )?;
    require(
        hash(Path::new(env!("CARGO_BIN_EXE_deadpan-cli")))? == run.cli_hash,
        "CLI binary changed during qualification",
    )?;
    require(
        hash(&std::env::current_exe()?)? == run.test_hash,
        "test binary changed during qualification",
    )
}
