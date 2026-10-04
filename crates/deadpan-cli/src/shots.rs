//! Host side of shot detection.
//!
//! The scan decodes every picture of the Original from its verified retained
//! snapshot, checks each against its qualified index ordinal, reduces it to a
//! [`PictureSignature`] and keeps only the previous signature and the
//! three-byte change. Nothing here edits a project: shot boundaries are
//! proposals, stored as an annotation outside history.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_analysis::{PictureSignature, SHOT_RULE, SIGNATURE_VERSION, ShotAnalysis};
use deadpan_media::picture_scan::{PictureScanError, scan_pictures};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_source::DecodeLimits;
use deadpan_store::ShotAnalysisKey;
use deadpan_store::original_media::OriginalMediaLimits;
use deadpan_store::source_registration::SourceQualificationReceipt;

#[derive(Debug, thiserror::Error)]
pub enum ShotScanError {
    #[error("shot detection is unavailable: {0}")]
    Unavailable(String),
    #[error("shot detection cancelled")]
    Cancelled,
    #[error("shot detection deadline elapsed")]
    Deadline,
    #[error("shot detection failed: {0}")]
    Failed(String),
}

impl From<PictureScanError> for ShotScanError {
    fn from(error: PictureScanError) -> Self {
        match error {
            PictureScanError::Cancelled => Self::Cancelled,
            PictureScanError::Deadline => Self::Deadline,
            error => Self::Failed(error.to_string()),
        }
    }
}

/// The verified Original pictures to scan, prepared from a store that may be
/// closed before the scan runs.
pub struct ShotInput {
    input: VerifiedSourceInput,
    receipt: SourceQualificationReceipt,
    /// The key the scan's analysis is saved under.
    pub key: ShotAnalysisKey,
}

impl ShotInput {
    /// Pictures in the Original's qualified index.
    pub fn pictures(&self) -> usize {
        self.receipt
            .snapshot()
            .video()
            .map_or(0, |video| video.index().index().frames().len())
    }
}

#[derive(Debug, Clone)]
pub struct ShotScan {
    pub key: ShotAnalysisKey,
    pub analysis: ShotAnalysis,
    pub elapsed: Duration,
    /// Time spent reducing converted pictures to signatures, within `elapsed`.
    pub signature_elapsed: Duration,
}

/// The qualification receipt whose pictures to scan: the single-Original
/// project's Original by default, or an explicitly registered source.
fn shot_receipt(
    store: &deadpan_store::ProjectStore,
    asset: Option<&deadpan_core::AssetId>,
) -> Result<SourceQualificationReceipt, ShotScanError> {
    use deadpan_store::single_source::SingleSourceState;
    let unavailable =
        |error: deadpan_store::StoreError| ShotScanError::Unavailable(error.to_string());
    if let Some(asset) = asset {
        let head = store.head_revision().map_err(unavailable)?;
        return store.registered_source(&head, asset).map_err(unavailable);
    }
    let Some(SingleSourceState::Ready {
        asset,
        baseline_revision,
        ..
    }) = store.single_source_state().map_err(unavailable)?
    else {
        return Err(ShotScanError::Unavailable(
            "project has no ready Original; choose a registered source asset".into(),
        ));
    };
    // The Original is immutable; its baseline revision always holds it.
    store
        .registered_source(&baseline_revision, &asset)
        .map_err(unavailable)
}

/// Copy the Original's retained bytes into a private verified snapshot.
pub fn prepare_shot_input(
    store: &deadpan_store::ProjectStore,
    asset: Option<&deadpan_core::AssetId>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<ShotInput, ShotScanError> {
    let receipt = shot_receipt(store, asset)?;
    let video = receipt.snapshot().video().ok_or_else(|| {
        ShotScanError::Unavailable("the Original has no qualified picture".into())
    })?;
    let identity = video.index().content();
    let maximum_bytes = DecodeLimits::default().max_input_bytes;
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(3_600));
    if remaining.is_zero() {
        return Err(ShotScanError::Deadline);
    }
    let failed = |error: &dyn std::fmt::Display| ShotScanError::Failed(error.to_string());
    let mut original = store
        .snapshot_original(
            receipt.original().content(),
            OriginalMediaLimits::new(maximum_bytes, remaining).map_err(|e| failed(&e))?,
            cancelled,
        )
        .map_err(|e| failed(&e))?;
    if original.record().object() != receipt.original() {
        return Err(ShotScanError::Failed(
            "retained Original differs from its receipt".into(),
        ));
    }
    let input = VerifiedSourceInput::copy_verified(
        &mut original,
        identity,
        maximum_bytes,
        remaining,
        cancelled,
    )
    .map_err(|e| failed(&e))?;
    let key = ShotAnalysisKey {
        content: receipt.original().content().to_string(),
        video_stream: video.index().stream_index(),
        signature_version: SIGNATURE_VERSION.into(),
    };
    Ok(ShotInput {
        input,
        receipt,
        key,
    })
}

/// Decode and measure every picture. `progress(done, total)` is called after
/// each picture; keep it cheap.
pub fn scan_shots(
    input: &ShotInput,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(usize, usize),
) -> Result<ShotScan, ShotScanError> {
    let started = Instant::now();
    let video = input.receipt.snapshot().video().ok_or_else(|| {
        ShotScanError::Unavailable("the Original has no qualified picture".into())
    })?;
    let total = video.index().index().frames().len();
    let mut changes = Vec::new();
    changes
        .try_reserve_exact(total)
        .map_err(|_| ShotScanError::Failed("shot change allocation failed".into()))?;
    let mut previous: Option<PictureSignature> = None;
    let mut before: Option<PictureSignature> = None;
    let mut signature_elapsed = Duration::ZERO;
    scan_pictures(
        &input.input,
        video,
        DecodeLimits::default(),
        deadline,
        cancelled,
        |ordinal, picture| {
            let reduced = Instant::now();
            let signature = PictureSignature::from_rgba(
                &picture.rgba,
                picture.width,
                picture.height,
                picture.row_stride_bytes,
            )?;
            changes.push(match &previous {
                Some(previous) => previous.change(&signature, before.as_ref()),
                None => [0, 0, 0],
            });
            before = previous.replace(signature);
            signature_elapsed += reduced.elapsed();
            progress(ordinal + 1, total);
            Ok::<_, deadpan_analysis::ShotError>(())
        },
    )?;
    let analysis = ShotAnalysis::new(changes).map_err(|e| ShotScanError::Failed(e.to_string()))?;
    Ok(ShotScan {
        key: input.key.clone(),
        analysis,
        elapsed: started.elapsed(),
        signature_elapsed,
    })
}

/// The stored shot analysis of one Original under the current signature
/// version. Analyses are rebuildable, so an unreadable row (including one
/// whose picture count differs from `expected_pictures`) is skipped.
pub fn stored_shots(
    store: &deadpan_store::ProjectStore,
    content: &str,
    video_stream: u32,
    expected_pictures: usize,
) -> Option<(ShotAnalysisKey, ShotAnalysis)> {
    store
        .shot_analysis_keys_for_content(content)
        .ok()?
        .into_iter()
        .filter(|key| {
            key.signature_version == SIGNATURE_VERSION && key.video_stream == video_stream
        })
        .find_map(|key| {
            let analysis = store.shot_analysis(&key, expected_pictures).ok()??;
            Some((key, analysis))
        })
}

fn parse_asset(
    arguments: &[&str],
    usage: &str,
) -> Result<Option<deadpan_core::AssetId>, crate::CliError> {
    match arguments {
        [_] => Ok(None),
        [_, "--asset", asset] => Ok(Some(
            deadpan_core::AssetId::new(*asset)
                .map_err(|e| crate::CliError::Usage(e.to_string()))?,
        )),
        _ => Err(crate::CliError::Usage(usage.into())),
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// `detect-shots PROJECT [--asset ID]`: scan every Original picture and store
/// the analysis outside history.
pub fn run_detect_shots(arguments: &[&str]) -> Result<(), crate::CliError> {
    let asset = parse_asset(
        arguments,
        "usage: detect-shots <project.deadpan> [--asset <id>]",
    )?;
    let path = std::path::Path::new(arguments[0]);
    // Read-only while scanning; the writer is taken only to save.
    let reader = deadpan_store::ProjectStore::open(path, deadpan_store::AccessMode::ReadOnly)?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(6 * 60 * 60);
    let input = prepare_shot_input(&reader, asset.as_ref(), &cancelled, deadline)?;
    drop(reader);
    let scan = scan_shots(&input, &cancelled, deadline, |_, _| {})?;
    let writer = deadpan_store::ProjectStore::open(path, deadpan_store::AccessMode::ReadWrite)?;
    writer.save_shot_analysis(&scan.key, &scan.analysis)?;
    drop(writer);
    let pictures = scan.analysis.pictures();
    let seconds = scan.elapsed.as_secs_f64();
    crate::write_json(&serde_json::json!({
        "protocol": 1,
        "rule": SHOT_RULE,
        "key": scan.key,
        "pictures": pictures,
        "boundaries": scan.analysis.boundaries(),
        "shots": scan.analysis.boundaries().len() + usize::from(pictures > 0),
        "elapsed_ms": millis(scan.elapsed),
        "signature_elapsed_ms": millis(scan.signature_elapsed),
        "pictures_per_second": if seconds > 0.0 { pictures as f64 / seconds } else { 0.0 },
    }))
}

/// `shots PROJECT [--asset ID]`: print the stored analysis's boundaries with
/// exact picture ordinals and PTS.
pub fn run_shots(arguments: &[&str]) -> Result<(), crate::CliError> {
    let asset = parse_asset(arguments, "usage: shots <project.deadpan> [--asset <id>]")?;
    let store = deadpan_store::ProjectStore::open(
        std::path::Path::new(arguments[0]),
        deadpan_store::AccessMode::ReadOnly,
    )?;
    let receipt = shot_receipt(&store, asset.as_ref())?;
    let video = receipt
        .snapshot()
        .video()
        .ok_or_else(|| ShotScanError::Unavailable("the source has no qualified picture".into()))?;
    let index = video.index().index();
    let frames = index.frames();
    let content = receipt.original().content().to_string();
    let Some((key, analysis)) =
        stored_shots(&store, &content, video.index().stream_index(), frames.len())
    else {
        return crate::write_json(&serde_json::json!({
            "protocol": 1,
            "rule": SHOT_RULE,
            "analysis": null,
            "boundaries": [],
        }));
    };
    let base = index.time_base();
    let boundaries = analysis
        .boundaries()
        .into_iter()
        .map(|picture| {
            let pts = frames[picture].pts;
            let [cell, histogram, skip] = analysis.changes()[picture];
            let exact = deadpan_core::ExactRatio::new(
                i128::from(pts) * i128::from(base.numerator()),
                i128::from(base.denominator()),
            )?;
            Ok(serde_json::json!({
                "picture": picture,
                "pts": pts,
                "seconds_exact": exact,
                "seconds": exact.numerator() as f64 / exact.denominator() as f64,
                "cell_change": cell,
                "histogram_change": histogram,
                "change_from_two_before": skip,
            }))
        })
        .collect::<Result<Vec<_>, crate::CliError>>()?;
    crate::write_json(&serde_json::json!({
        "protocol": 1,
        "rule": SHOT_RULE,
        "analysis": {
            "key": key,
            "pictures": analysis.pictures(),
            "time_base": [base.numerator(), base.denominator()],
        },
        "boundaries": boundaries,
    }))
}
