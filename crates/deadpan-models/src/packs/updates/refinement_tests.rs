use super::*;
use crate::updates::generate_key;

fn update() -> PackUpdate {
    let mut pack = approved_pack("phoneme-ctc-en").unwrap();
    pack.pack_version = "2".into();
    pack.title = "Updated approximate-proposal description".into();
    PackUpdate {
        schema: UPDATE_SCHEMA,
        serial: 1,
        issued: "2026-10-25".into(),
        min_app_version: "0.1.0".into(),
        pack,
    }
}

fn incompatible(result: Result<(), PackError>) {
    assert!(matches!(
        result,
        Err(PackError::Update {
            code: "UpdateIncompatible",
            ..
        })
    ));
}

#[test]
fn refinement_updates_may_change_descriptions_but_keep_all_qualified_source_identities() {
    let baseline = approved_pack("phoneme-ctc-en").unwrap();
    let descriptive = update();
    descriptive.validate().unwrap();
    validate_refinement_update(&descriptive.pack, &baseline).unwrap();
    for name in [
        "model-0.6.ckpt",
        "global_stats.json",
        "cmudict.dict",
        "cmudict.phones",
        "AUTHOR-LICENSE",
        "CMUDICT-LICENSE",
    ] {
        for change in 0..4 {
            let mut candidate = descriptive.clone();
            let file = candidate
                .pack
                .files
                .iter_mut()
                .find(|file| file.name == name)
                .unwrap();
            match change {
                0 => file.sha256 = "a".repeat(64),
                1 => file.bytes += 1,
                2 => {
                    file.url = file
                        .url
                        .replace(
                            "/a4749fe14f17b19c071fe5b20005575ec0509ca3/",
                            "/0000000000000000000000000000000000000000/",
                        )
                        .replace(
                            "/74790861f652b15e4ac49015a90074ad62a27690/",
                            "/0000000000000000000000000000000000000000/",
                        )
                }
                _ => file.name = format!("other-{name}"),
            }
            candidate.pack.validate().unwrap();
            incompatible(candidate.validate());
        }
    }
    for mutate in [
        |pack: &mut PackManifest| pack.operations = vec![Operation::SpeechActivity],
        |pack: &mut PackManifest| pack.model_family = "different-phoneme-model".into(),
        |pack: &mut PackManifest| pack.runtime_versions.push("2".into()),
        |pack: &mut PackManifest| pack.languages.push("fr".into()),
        |pack: &mut PackManifest| pack.memory_bytes += 1,
        |pack: &mut PackManifest| pack.temporary_bytes += 1,
        |pack: &mut PackManifest| pack.licenses[0].redistribution = true,
        |pack: &mut PackManifest| pack.licenses[0].spdx = "MIT".into(),
        |pack: &mut PackManifest| {
            pack.files.pop();
        },
    ] {
        let mut candidate = descriptive.clone();
        mutate(&mut candidate.pack);
        candidate.pack.validate().unwrap();
        incompatible(candidate.validate());
    }
}

#[test]
fn valid_signature_cannot_replace_the_compiled_refinement_checkpoint() {
    let (private, public) = generate_key().unwrap();
    let keys = [TrustedKey {
        id: "refinement-test".into(),
        public_key: public,
    }];
    let sign = |update: &PackUpdate| {
        SignedManifest::sign(
            UpdateKind::ModelPack,
            serde_json::to_string(update).unwrap(),
            "refinement-test",
            &private,
        )
        .unwrap()
        .to_bytes()
    };
    let mut candidate = update();
    inspect_update(&sign(&candidate), &keys).unwrap();
    candidate.pack.files[0].sha256 = "a".repeat(64);
    assert!(matches!(
        inspect_update(&sign(&candidate), &keys),
        Err(PackError::Update {
            code: "UpdateIncompatible",
            ..
        })
    ));
}

#[test]
fn refinement_operation_cannot_be_smuggled_into_the_transcription_pack() {
    let mut candidate = update();
    candidate.pack = approved_pack("whisper-base-en").unwrap();
    candidate.pack.pack_version = "3".into();
    candidate.pack.operations.push(Operation::RefineWords);
    candidate.pack.validate().unwrap();
    incompatible(candidate.validate());
}
