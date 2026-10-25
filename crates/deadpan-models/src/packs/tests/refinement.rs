use super::*;

fn refinement() -> PackManifest {
    approved_pack("phoneme-ctc-en").unwrap()
}

fn role_names(pack: &PackManifest) -> [Option<&str>; 4] {
    [
        pack.refinement_checkpoint_file()
            .map(|file| file.name.as_str()),
        pack.refinement_statistics_file()
            .map(|file| file.name.as_str()),
        pack.refinement_dictionary_file()
            .map(|file| file.name.as_str()),
        pack.refinement_phones_file().map(|file| file.name.as_str()),
    ]
}

#[test]
fn original_refinement_pack_pins_source_data_and_bounded_audio_contract() {
    let pack = refinement();
    pack.validate().unwrap();
    assert_eq!(pack.schema, 4);
    assert_eq!(pack.pack_version, "1");
    assert_eq!(pack.runtime_id, "deadpan-align");
    assert_eq!(pack.runtime_versions, ["1"]);
    assert_eq!(pack.operations, [Operation::RefineWords]);
    assert_eq!(pack.languages, ["en"]);
    assert_eq!(
        pack.constraints.weight_precisions,
        [constraints::Precision::Float32]
    );
    assert_eq!(
        pack.constraints.conditioning,
        [constraints::Conditioning::MonoAudio]
    );
    assert_eq!(
        pack.constraints.audio,
        Some(constraints::AudioConstraints {
            sample_rate: 16_000,
            channels: 1,
            maximum_samples: 480_000,
        })
    );
    assert!(pack.constraints.bridge.is_none());
    assert!(pack.constraints.extension.is_none());
    assert_eq!(pack.files.len(), 6);
    assert_eq!(pack.total_bytes(), 61_188_767);
    for (name, bytes, sha256) in [
        (
            "model-0.6.ckpt",
            57_562_900,
            "4070c3b89fb27129e2999f550ddb55c747ec77da843fa8a520989b8c5d09723f",
        ),
        (
            "global_stats.json",
            3_905,
            "be2f80418ef66bad30dcd5556e194f95234d75c5d2b94a92d7a766d31900df87",
        ),
        (
            "cmudict.dict",
            3_618_488,
            "81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22",
        ),
        (
            "cmudict.phones",
            382,
            "ffb588a5e55684723582c7256e1d2f9fadb130011392d9e59237c76e34c2cfd6",
        ),
    ] {
        let file = pack.files.iter().find(|file| file.name == name).unwrap();
        assert_eq!(file.bytes, bytes, "{name}");
        assert_eq!(file.sha256, sha256, "{name}");
        assert!(file.url.starts_with("https://raw.githubusercontent.com/"));
        let revision = if name.starts_with("cmudict.") {
            "74790861f652b15e4ac49015a90074ad62a27690"
        } else {
            "a4749fe14f17b19c071fe5b20005575ec0509ca3"
        };
        assert!(file.url.contains(revision), "{name}");
        assert!(!file.url.contains("/main/"));
    }
}

#[test]
fn refinement_roles_require_operation_and_exact_names_independent_of_file_order() {
    let mut pack = refinement();
    let names = [
        Some("model-0.6.ckpt"),
        Some("global_stats.json"),
        Some("cmudict.dict"),
        Some("cmudict.phones"),
    ];
    assert_eq!(role_names(&pack), names);
    pack.files.reverse();
    assert_eq!(role_names(&pack), names);
    for target in names.into_iter().flatten() {
        let mut changed = pack.clone();
        changed
            .files
            .iter_mut()
            .find(|file| file.name == target)
            .unwrap()
            .name = format!("other-{target}");
        let missing = role_names(&changed);
        assert_eq!(missing.iter().filter(|file| file.is_none()).count(), 1);
    }
    pack.operations = vec![Operation::Transcribe, Operation::SpeechActivity];
    assert_eq!(role_names(&pack), [None; 4]);
    let whisper = approved_pack("whisper-base-en").unwrap();
    assert_eq!(role_names(&whisper), [None; 4]);
    assert!(refinement().transcription_file().is_none());
    assert!(refinement().speech_activity_file().is_none());
}

#[test]
fn refinement_keeps_checkpoint_ambiguity_and_exact_source_dictionary_notices_separate() {
    let pack = refinement();
    let weights = pack
        .licenses
        .iter()
        .find(|license| license.id == "checkpoint-terms")
        .unwrap();
    assert_eq!(
        weights.spdx,
        "LicenseRef-Phoneme-Checkpoint-Terms-Unspecified"
    );
    assert!(!weights.redistribution);
    assert!(weights.text.is_none());
    assert!(!weights.acceptance_required);
    assert!(weights.terms.contains("No separately explicit"));
    assert_eq!(pack.files[0].license.as_deref(), Some("checkpoint-terms"));
    assert_eq!(pack.files[1].license.as_deref(), Some("checkpoint-terms"));
    for (notice, text_file, expected) in [
        (
            "AUTHOR-LICENSE",
            "phoneme-author-bsd-2-clause.txt",
            "93a58861a858cc108e6b6b833e08e76e8b2a66339e4a8007c8a5a8c1ff9c40d6",
        ),
        (
            "CMUDICT-LICENSE",
            "cmudict-license.txt",
            "bd4ce8e44170a5f9f481310ca85c51de3c4f851a65e679b40e603b143bd3542a",
        ),
    ] {
        let bytes = license_text(text_file).unwrap().as_bytes();
        let digest: String = sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let file = pack.files.iter().find(|file| file.name == notice).unwrap();
        assert_eq!(digest, expected);
        assert_eq!(file.sha256, expected);
        assert_eq!(file.bytes, bytes.len() as u64);
    }
    assert_eq!(
        license_text("phoneme-author-bsd-2-clause.txt").unwrap(),
        include_str!("../../../../../native/deadpan-align/assets/AUTHOR-LICENSE")
    );
    assert_eq!(
        pack.licenses
            .iter()
            .map(|license| pack.license_bytes(license))
            .sum::<u64>(),
        pack.total_bytes()
    );
}

#[test]
fn original_source_host_is_exact_and_does_not_admit_lookalikes_credentials_or_ports() {
    let pack = refinement();
    for url in [
        "http://raw.githubusercontent.com/owner/repo/pin/model",
        "https://raw.githubusercontent.com.evil.test/owner/repo/model",
        "https://evil.test/raw.githubusercontent.com/model",
        "https://raw.githubusercontent.com@evil.test/model",
        "https://user@raw.githubusercontent.com/owner/repo/model",
        "https://raw.githubusercontent.com:443/owner/repo/model",
        "https://github.com/owner/repo/model",
    ] {
        let mut changed = pack.clone();
        changed.files[0].url = url.into();
        assert!(changed.validate().is_err(), "{url}");
    }
    assert!(pack.validate().is_ok());
}
