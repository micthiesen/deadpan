//! One-anchor observation request, sharing Bridge's checked process ownership.

use deadpan_analysis::NormalizedRect;
use deadpan_analysis::generated_extension::ExtensionCoverage;
use deadpan_jobs::landmarks::InspectionExtensionObservations;

use super::*;
use crate::RetainedExtensionConditioning;

pub(crate) struct ObservedExtension {
    pub batch: InspectionExtensionObservations,
    pub runtime: RuntimeReport,
    pub region_runtime: Option<RegionRuntimeReport>,
    pub timings: InspectionTimings,
}

pub(crate) fn inspect(
    executable: &Path,
    native: &mut CanonicalMedia,
    conditioning: &mut RetainedExtensionConditioning,
    coverage: ExtensionCoverage,
    region_seed: Option<NormalizedRect>,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<ObservedExtension, QualificationError> {
    let control = Control {
        deadline,
        cancelled,
    };
    control.check()?;
    let contract = native.report().video;
    let expected = expected_pts(contract)?;
    coverage.validate(&expected).map_err(invalid)?;
    let directory = tempfile::Builder::new()
        .prefix("deadpan-extension-landmarks-")
        .tempdir()?;
    std::fs::create_dir(directory.path().join("input"))?;
    std::fs::create_dir(directory.path().join("output"))?;
    let pinned = ArtifactWorkspace::open(directory.path())?;
    let source_object = native.object().clone();
    let source_hash = native
        .verified_source_input()
        .map_err(invalid)?
        .identity()
        .sha256();
    let source = copy_input(
        native,
        &source_object,
        source_hash,
        directory.path(),
        "native.mkv",
        landmarks::MAX_SOURCE_BYTES,
        &control,
    )?;
    let input = conditioning.anchor_mut();
    let object = input.object().clone();
    let hash = decode_hash(input.declaration().sha256());
    let anchor = copy_input(
        input,
        &object,
        hash,
        directory.path(),
        "anchor.png",
        landmarks::MAX_PNG_BYTES,
        &control,
    )?;
    let output_scope = WorkspaceRef::new("output").map_err(invalid)?;
    let remaining = control.remaining()?.min(Duration::from_secs(3600));
    let request = HostMessage::InspectExtensionLandmarks {
        protocol: landmarks::VERSION,
        request: RequestId::new("extension-landmarks").map_err(invalid)?,
        attempt: AttemptId::new("inspection").map_err(invalid)?,
        cancellation_token: CancellationToken::new("cancel-inspection").map_err(invalid)?,
        source,
        stream: landmarks::ExpectedStream {
            stream_index: 0,
            width: contract.width,
            height: contract.height,
            time_base_num: 1,
            time_base_den: 1000,
            rotation_quarter_turns: 0,
        },
        picture_pts: expected.clone(),
        anchor,
        coverage,
        region_seed,
        output_scope: output_scope.clone(),
        maximum_output_bytes: landmarks::MAX_OBSERVATION_BYTES,
        timeout_millis: u64::try_from(remaining.as_millis()).map_err(invalid)?,
    };
    request.validate().map_err(invalid)?;
    let (snapshot, completion) = execute_inspection(
        executable,
        directory.path(),
        &pinned,
        request,
        &output_scope,
        &control,
    )?;
    let batch: InspectionExtensionObservations = serde_json::from_reader(snapshot)?;
    control.check()?;
    batch
        .validate(&expected, &coverage, region_seed.as_ref())
        .map_err(invalid)?;
    if region_seed.is_some() != completion.region_runtime.is_some() {
        return Err(invalid(
            "extension region runtime differs from the retained selection",
        ));
    }
    Ok(ObservedExtension {
        batch,
        runtime: completion.runtime,
        region_runtime: completion.region_runtime,
        timings: completion.timings,
    })
}

pub(crate) fn expected_pts(contract: VideoContract) -> Result<Vec<i64>, QualificationError> {
    contract.validate().map_err(invalid)?;
    if !(2..=deadpan_analysis::generated_extension::MAX_NATIVE_FRAMES as u32)
        .contains(&contract.frames)
    {
        return Err(invalid(
            "extension native count exceeds the landmark inspection bound",
        ));
    }
    (0..contract.frames)
        .map(|ordinal| contract.matroska_pts(ordinal).map_err(invalid))
        .collect()
}
