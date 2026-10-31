//! Complete native body retained before public Point mask ownership.
use super::*;

#[test]
fn complete_native_body_preserves_all_current_query_admissions() {
    let bytes =
        include_bytes!("../../tests/fixtures/owned-suppression-native-baseline-v1/snapshot.json");
    assert_eq!(
        hex(Sha256::digest(bytes)),
        "3d6338e4e4b0f5617647ab6e0fa4d80bf8882a87c87a55efe306002c3011dbdc"
    );
    let expected: Value = serde_json::from_slice(bytes).unwrap();
    let mut value = crate::stages::internal_mask_request::with_legacy_internal_masks(|| {
        captured_body_with_point(
            "owned-output-regression",
            &"0".repeat(64),
            "87ab6192e765a34dae9595139e10bd9010d6315e",
            true,
        )
    });
    value["published_point_content_checkpoint"] = json!("af34e1e1143e9abc11f49b77ee49c602c116aac4");
    value["preceding_raw_domain_schema"] = value["schema"].clone();
    value["schema"] = json!("point-content-native-admitted-v1");
    value["actual_point_content_mode"] = json!("admitted_query_forwarded_debit");
    value["point_content_scope"] = json!(
        "Complete actual paired.new observations against point-content-native-baseline-v1 paired.new, with independent Point constructor and accepted global host-eviction receipts. Embedded preceding pair names and fields remain unchanged."
    );
    value["pre_point_content_frozen_sha256"] =
        json!("164f9543d7db901b20a8d1a4d200a9853cd3a89e6fc0615aaf61f4ee2dee38c3");
    value.as_object_mut().unwrap().remove("variant");
    value.as_object_mut().unwrap().remove("source_sha256");
    assert_eq!(value, expected, "all 744 native observations stay literal");
}
