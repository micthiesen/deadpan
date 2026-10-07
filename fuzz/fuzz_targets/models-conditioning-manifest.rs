#![no_main]
//! Retained bridge conditioning context manifests, read by the model
//! qualification path and by the CLI's colour summary.
use deadpan_cli::generation::conditioning::ConditioningColour;
use deadpan_fuzz::refused;
use deadpan_models::BridgeContext;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let _ = ConditioningColour::from_manifest(input);
    match serde_json::from_slice::<BridgeContext>(input) {
        Ok(context) => {
            let _ = context.check_model_output();
            let encoded = serde_json::to_vec(&context).expect("accepted manifest re-encodes");
            let again: BridgeContext =
                serde_json::from_slice(&encoded).expect("accepted manifest re-parses");
            assert!(again == context, "manifest round trip changed the value");
        }
        Err(error) => refused(error),
    }
});
