use std::io::Read;
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{Duration, Instant};

use deadpan_jobs::artifact::ArtifactWorkspace;
use deadpan_jobs::{
    AttemptId, CancellationToken, ContextArtifact, HoldTarget, HostMessage, MessageIdentity,
    ProtocolVersion, RequestId, RequestVersion, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_models::{ConditioningLimits, capture_extension_conditioning};

use super::*;

#[test]
fn real_extension_evidence_survives_retention_and_matches_the_python_reader() -> Result {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let mut fixture = Fixture::source("cfr-bframes.mp4")?;
        edge_hold(&mut fixture, direction)?;
        let origin = fixture.store.snapshot()?;
        let inputs = capture(&fixture, &target(Vec::new()), direction)?;
        let context = manifest(&inputs)?;
        assert_eq!(context.continuity().binding(), &inputs.continuity.binding);
        assert_eq!(
            context.continuity().qualify_signatures(
                &inputs.continuity_signatures,
                &active(),
                Instant::now() + Duration::from_secs(30),
            )?,
            inputs.continuity.measurements
        );
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("inputs"))?;
        fs::create_dir(directory.path().join("outputs"))?;
        let manifest_ref = WorkspaceRef::new("inputs/context.json")?;
        let declaration = WorkspaceArtifact::new(
            manifest_ref.clone(),
            inputs.manifest_sha256.clone(),
            inputs.manifest.len() as u64,
        )?;
        fs::write(
            directory.path().join(manifest_ref.as_str()),
            &inputs.manifest,
        )?;
        for (picture, bytes) in context.context().iter().zip(&inputs.context_pngs) {
            fs::write(
                directory.path().join(picture.frame.reference().as_str()),
                bytes,
            )?;
        }
        if let (ExtensionOppositeSeam::PresentUnconditioned { frame, .. }, Some(bytes)) =
            (context.opposite(), &inputs.opposite_png)
        {
            fs::write(directory.path().join(frame.reference().as_str()), bytes)?;
        }
        let signatures_path = directory
            .path()
            .join(context.continuity().signatures().reference().as_str());
        fs::write(&signatures_path, &inputs.continuity_signatures)?;
        let request = HostMessage::GenerateExtension {
            protocol: ProtocolVersion::V3,
            identity: MessageIdentity::new(
                RequestId::new("retained-context")?,
                AttemptId::new("attempt")?,
            ),
            cancellation_token: CancellationToken::new("cancel")?,
            project_id: origin.project_id().clone(),
            revision_id: origin.revision_id().clone(),
            target: HoldTarget {
                hold_id: node("extension"),
                request_version: RequestVersion::new(1)?,
            },
            input: ContextArtifact {
                manifest: manifest_ref,
                sha256: inputs.manifest_sha256.clone(),
            },
            output_workspace: WorkspaceRef::new("outputs")?,
            constraints: inputs.constraints.clone(),
            provider: Box::new(deadpan_jobs::ProviderSelection {
                pack_id: deadpan_jobs::ProviderPackId::new("ltx-2.3-q4-extension-development")?,
                pack_version: deadpan_jobs::ProviderPackVersion::new("1")?,
                runtime_id: deadpan_jobs::RuntimeId::new("ltx-mlx")?,
                runtime_version: deadpan_jobs::RuntimeVersion::new(
                    "0.15.8+deadpan-extension-dev1",
                )?,
                seed: 1,
            }),
            plan: Box::new(inputs.plan.clone()),
        };
        fs::write(
            directory.path().join("request.json"),
            serde_json::to_vec(&request)?,
        )?;
        let mut command = ProcessCommand::new("python3");
        command.args(["-I", "-c", r#"
import json, pathlib, sys
sys.path.insert(0, sys.argv[1])
import worker, worker_protocol
from worker_extension_context import validate_signature_bytes
root = pathlib.Path(sys.argv[2])
wire = json.loads((root / 'request.json').read_bytes())
context = json.loads((root / 'inputs/context.json').read_bytes())
request = worker_protocol.parse_host_message(wire)
references = worker.validate_extension_context(context, request, wire['constraints']['video'])
assert references == [item['frame'] for item in context['context']]
validate_signature_bytes(context['continuity'], (root / context['continuity']['signatures']['reference']).read_bytes())
"#])
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/model-qualification"))
            .arg(directory.path()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let output = deadpan_native_process::spawn(&mut command)?.wait_with_output()?;
        assert!(
            output.status.success(),
            "Python rejected captured {direction:?} context: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let workspace = ArtifactWorkspace::open(directory.path())?;
        let limits = ConditioningLimits::new(1024 * 1024, 16 * 1024 * 1024, 30_000)?;
        let scope = WorkspaceRef::new("inputs")?;
        let retained = capture_extension_conditioning(
            &workspace,
            &request,
            &declaration,
            &scope,
            limits,
            &active(),
        )?;
        assert_eq!(retained.measurements(), inputs.continuity.measurements);
        let receipt = retained.receipt().signatures().clone();
        let mut corrupt = inputs.continuity_signatures.clone();
        corrupt[0] ^= 1;
        fs::write(&signatures_path, corrupt)?;
        assert!(
            capture_extension_conditioning(
                &workspace,
                &request,
                &declaration,
                &scope,
                limits,
                &active()
            )
            .is_err()
        );
        retained.validate_for(&request)?;
        let (_, _, _, mut signatures) = retained.into_parts();
        let mut bytes = Vec::new();
        signatures.read_to_end(&mut bytes)?;
        assert_eq!(bytes, inputs.continuity_signatures);
        assert_eq!(
            receipt.object().content().digest(),
            blake3::hash(&bytes).to_hex().as_str()
        );
        assert_eq!(fixture.store.snapshot()?, origin);
    }
    Ok(())
}
