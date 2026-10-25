use super::*;

#[test]
fn ranged_follow_window_counts_before_reading_its_fields_and_null_is_absent() {
    let mut counts = Counts {
        records: MAX_FRAMING_RECORDS - 2,
        nodes: 0,
    };
    let mut decoder =
        serde_json::Deserializer::from_str(r#"{"fallback":{},"scale_transition":{"range":{"#);
    let error = Scan {
        role: Role::Value,
        counts: &mut counts,
        charge: false,
    }
    .deserialize(&mut decoder)
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("aggregate framing record limit exceeded"),
        "{error}"
    );
    assert_eq!(counts.records, MAX_FRAMING_RECORDS);
    let mut counts = Counts::default();
    let mut decoder =
        serde_json::Deserializer::from_str(r#"{"fallback":{},"scale_transition":{"range":null}}"#);
    Scan {
        role: Role::Value,
        counts: &mut counts,
        charge: false,
    }
    .deserialize(&mut decoder)
    .unwrap();
    assert_eq!(counts.records, 2);
    let mut decoder = serde_json::Deserializer::from_str(
        r#"{"fallback":{},"scale_transition":{"ra\u006ege":{}}}"#,
    );
    let mut counts = Counts::default();
    Scan {
        role: Role::Value,
        counts: &mut counts,
        charge: false,
    }
    .deserialize(&mut decoder)
    .unwrap();
    assert_eq!(counts.records, 3);
}

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
