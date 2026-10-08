use super::*;
use crate::packs::constraints::extension_tests::extension_pack;

#[test]
fn extension_pack_cannot_enter_through_an_existing_bridge_update() {
    let mut pack = extension_pack();
    pack.pack_id = "ltx-2.3-q4-bridge".into();
    pack.pack_version = "2".into();
    pack.validate().unwrap();
    let update = PackUpdate {
        schema: UPDATE_SCHEMA,
        serial: 1,
        issued: "2026-10-08".into(),
        min_app_version: "0.1.0".into(),
        pack,
    };
    assert!(matches!(
        update.validate(),
        Err(PackError::Update {
            code: "UpdateIncompatible",
            ..
        })
    ));
}

#[test]
fn extension_update_keeps_its_independent_runtime_data_and_duration_contract() {
    let baseline = extension_pack();
    let mut version = baseline.clone();
    version.pack_version = "2".into();
    version.title = "Updated description".into();
    validate_extension_update(&version, &baseline).unwrap();
    for mutate in [
        |pack: &mut PackManifest| pack.operations = vec![Operation::BridgeHold],
        |pack: &mut PackManifest| pack.runtime_id = "bridge-runtime".into(),
        |pack: &mut PackManifest| pack.runtime_versions.push("another-pipeline".into()),
        |pack: &mut PackManifest| pack.model_family = "another-model".into(),
        |pack: &mut PackManifest| pack.languages.push("en".into()),
        |pack: &mut PackManifest| pack.memory_bytes += 1,
        |pack: &mut PackManifest| pack.temporary_bytes += 1,
        |pack: &mut PackManifest| {
            pack.constraints
                .extension
                .as_mut()
                .unwrap()
                .maximum_project_frames += 1
        },
        |pack: &mut PackManifest| {
            pack.constraints
                .extension
                .as_mut()
                .unwrap()
                .maximum_requested_duration = deadpan_core::ExactRatio::new(1, 2).unwrap()
        },
        |pack: &mut PackManifest| pack.files[0].sha256 = "a".repeat(64),
        |pack: &mut PackManifest| pack.files[0].bytes += 1,
        |pack: &mut PackManifest| {
            pack.files.pop();
        },
    ] {
        let mut changed = version.clone();
        mutate(&mut changed);
        assert!(matches!(
            validate_extension_update(&changed, &baseline),
            Err(PackError::Update {
                code: "UpdateIncompatible",
                ..
            })
        ));
    }
}
