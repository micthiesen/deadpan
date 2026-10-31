//! Complete native behavior before fresh Structural Point content admission.
use super::*;

#[test]
fn complete_ordinary_point_content_native_body_matches_frozen_before() {
    let bytes =
        include_bytes!("../../tests/fixtures/point-content-native-baseline-v1/snapshot.json");
    assert_eq!(
        hex(Sha256::digest(bytes)),
        "164f9543d7db901b20a8d1a4d200a9853cd3a89e6fc0615aaf61f4ee2dee38c3"
    );
    let expected: Value = serde_json::from_slice(bytes).unwrap();
    let mut actual = point_content_request::with_legacy_point_content(|| {
        captured_body(
            "point-content-before-regression",
            &"0".repeat(64),
            "87ab6192e765a34dae9595139e10bd9010d6315e",
        )
    });
    actual.as_object_mut().unwrap().remove("variant");
    actual.as_object_mut().unwrap().remove("source_sha256");
    assert_eq!(
        actual, expected,
        "complete ordinary Point content native body"
    );
}
