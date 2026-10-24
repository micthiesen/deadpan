use super::*;

#[test]
fn mixed_fixed_animated_and_pose_envelope_records_share_the_real_preflight_cap() {
    let fixed = r#"{"framing":{"value":{"fallback":{}}}}"#;
    let animated = r#"{"framing":{"value":{"fallback":{},"sc\u0061le_transition":{}}}}"#;
    let envelope = r#"{"framing":{"value":{"envelope":{"initial":{},"segments":[{"curve":{"type":"linear"}}]}}}}"#;
    // Each three-node cycle consumes exactly 1 + 2 + 2 records. This is a
    // scanner test, not a claim that empty record bodies are typed documents.
    let mut entries = Vec::new();
    for n in 0..MAX_FRAMING_RECORDS / 5 {
        for (kind, body) in [
            ("fixed", fixed),
            ("animated", animated),
            ("envelope", envelope),
        ] {
            entries.push(format!(r#""{kind}-{n}":{body}"#));
        }
    }
    let entries = entries.join(",");
    assert!(check(&format!(r#"{{"nodes":{{{entries}}}}}"#)).is_ok());
    let error = check(&format!(
        r#"{{"nodes":{{{entries},"extra":{{"framing":{{"value":{{"sc\u0061le_transition":{{"#
    ))
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("aggregate framing record limit exceeded"),
        "{error}"
    );
}
