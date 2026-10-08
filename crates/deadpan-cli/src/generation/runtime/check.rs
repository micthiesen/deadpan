//! A successful process must report the exact selected operation and identity.

use std::collections::BTreeMap;

use deadpan_jobs::Sha256;
use deadpan_models::packs::Operation;
use serde::Deserialize;

use super::BridgeRuntime;

// worker.check appends this complete adapter inventory to check_runtime's
// report. These are worker-reported diagnostics; bundle verification owns
// approval of the executable sources, separately from model-pack identity.
const ADAPTER_SOURCE_NAMES: [&str; 7] = [
    "ltx-source-manifest.json",
    "mlx_backend.py",
    "runtime_source.py",
    "worker.py",
    "worker_extension_context.py",
    "worker_media.py",
    "worker_protocol.py",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckReport {
    schema_version: u32,
    runtime_commit: String,
    operation: Operation,
    pack_id: String,
    pack_version: String,
    runtime_id: String,
    runtime_version: String,
    model_manifest_sha256: String,
    adapter_sources_sha256: BTreeMap<String, Sha256>,
    device: String,
    python: String,
    mlx: String,
    verified_assets: usize,
    safetensors_tensors: u64,
    loaded_ltx_sources: usize,
    seconds: f64,
}

impl BridgeRuntime {
    pub(super) fn validate_check_report(&self, bytes: &[u8]) -> Result<serde_json::Value, String> {
        super::selection::manifest_mode(&self.model_manifest)?;
        let report: CheckReport = serde_json::from_slice(bytes)
            .map_err(|error| format!("the AI runtime check printed an invalid report: {error}"))?;
        let manifest = &self.model_manifest;
        if report.schema_version != 2
            || report.runtime_commit != super::RUNTIME_COMMIT
            || manifest.operations != [report.operation]
            || report.pack_id != manifest.pack_id
            || report.pack_version != manifest.pack_version
            || report.runtime_id != manifest.runtime_id
            || manifest.runtime_versions != [report.runtime_version]
            || report.model_manifest_sha256 != self.model_manifest_sha256()
            || report
                .adapter_sources_sha256
                .keys()
                .map(String::as_str)
                .ne(ADAPTER_SOURCE_NAMES)
            || !report.device.starts_with("Device(gpu,")
            || report.python.trim().is_empty()
            || report.mlx.trim().is_empty()
            || report.verified_assets != manifest.files.len()
            || report.safetensors_tensors == 0
            || report.loaded_ltx_sources == 0
            || !report.seconds.is_finite()
            || report.seconds < 0.0
        {
            return Err(
                "the AI runtime check differs from the selected pack, operation or Metal contract"
                    .into(),
            );
        }
        serde_json::from_slice(bytes).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_models::packs::approved_pack;

    fn runtime(pack: &str) -> BridgeRuntime {
        BridgeRuntime {
            python: "python".into(),
            runtime_source: "source".into(),
            model_cache: "models".into(),
            model_manifest: approved_pack(pack).unwrap(),
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            worker_script: "worker".into(),
            media_worker: "media".into(),
            landmark_worker: "tracker".into(),
        }
    }

    /// Complete stdout shape from worker.check, including its additions to
    /// the MLX backend report. Both operations use this same wrapper.
    fn worker_report(runtime: &BridgeRuntime) -> serde_json::Value {
        let manifest = &runtime.model_manifest;
        let adapter_sources: BTreeMap<_, _> = ADAPTER_SOURCE_NAMES
            .into_iter()
            .map(|name| (name, "a".repeat(64)))
            .collect();
        serde_json::json!({
            "schema_version": 2, "runtime_commit": super::super::RUNTIME_COMMIT,
            "operation": manifest.operations[0], "pack_id": manifest.pack_id, "pack_version": manifest.pack_version,
            "runtime_id": manifest.runtime_id, "runtime_version": manifest.runtime_versions[0],
            "model_manifest_sha256": runtime.model_manifest_sha256(), "adapter_sources_sha256": adapter_sources,
            "device": "Device(gpu, 0)", "python": "3.12.13", "mlx": "0.31.1",
            "verified_assets": manifest.files.len(), "safetensors_tensors": 1, "loaded_ltx_sources": 1, "seconds": 1.0,
        })
    }

    #[test]
    fn smoke_report_must_match_every_captured_identity_and_operation() {
        for pack in [
            crate::generation::runtime::BRIDGE_PACK,
            crate::generation::runtime::EXTENSION_PACK,
        ] {
            let runtime = runtime(pack);
            let report = worker_report(&runtime);
            assert!(
                runtime
                    .validate_check_report(&serde_json::to_vec(&report).unwrap())
                    .is_ok()
            );
            for (field, value) in [
                ("schema_version", serde_json::json!(1)),
                ("runtime_commit", serde_json::json!("other")),
                ("operation", serde_json::json!("transcribe")),
                ("pack_id", serde_json::json!("other")),
                ("pack_version", serde_json::json!("2")),
                ("runtime_id", serde_json::json!("other")),
                ("runtime_version", serde_json::json!("other")),
                ("model_manifest_sha256", serde_json::json!("0".repeat(64))),
                ("device", serde_json::json!("Device(cpu, 0)")),
                ("verified_assets", serde_json::json!(0)),
                ("safetensors_tensors", serde_json::json!(0)),
                ("loaded_ltx_sources", serde_json::json!(0)),
                ("seconds", serde_json::json!(-1)),
                ("extra", serde_json::json!(true)),
            ] {
                let mut changed = report.clone();
                changed[field] = value;
                assert!(
                    runtime
                        .validate_check_report(&serde_json::to_vec(&changed).unwrap())
                        .is_err(),
                    "{pack} {field}"
                );
            }
        }
    }

    #[test]
    fn complete_worker_smoke_report_requires_every_well_formed_adapter_source() {
        for pack in [
            crate::generation::runtime::BRIDGE_PACK,
            crate::generation::runtime::EXTENSION_PACK,
        ] {
            let runtime = runtime(pack);
            let report = worker_report(&runtime);
            let bytes = serde_json::to_vec(&report).unwrap();
            assert_eq!(runtime.validate_check_report(&bytes).unwrap(), report);
            for source in ADAPTER_SOURCE_NAMES {
                for malformed in [
                    serde_json::Value::Null,
                    serde_json::json!("a".repeat(63)),
                    serde_json::json!("g".repeat(64)),
                    serde_json::json!(7),
                ] {
                    let mut changed = report.clone();
                    changed["adapter_sources_sha256"][source] = malformed;
                    assert!(
                        runtime
                            .validate_check_report(&serde_json::to_vec(&changed).unwrap())
                            .is_err(),
                        "{pack} {source}"
                    );
                }
                let mut missing = report.clone();
                missing["adapter_sources_sha256"]
                    .as_object_mut()
                    .unwrap()
                    .remove(source);
                assert!(
                    runtime
                        .validate_check_report(&serde_json::to_vec(&missing).unwrap())
                        .is_err(),
                    "missing {source}"
                );
            }
            let mut changed = report.clone();
            changed["adapter_sources_sha256"]["unexpected.py"] = serde_json::json!("a".repeat(64));
            assert!(
                runtime
                    .validate_check_report(&serde_json::to_vec(&changed).unwrap())
                    .is_err()
            );
            changed
                .as_object_mut()
                .unwrap()
                .remove("adapter_sources_sha256");
            assert!(
                runtime
                    .validate_check_report(&serde_json::to_vec(&changed).unwrap())
                    .is_err()
            );
        }
    }
}
