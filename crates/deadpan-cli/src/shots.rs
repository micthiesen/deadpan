//! Host side of shot detection.
//!
//! The scan decodes every picture of the Original from its verified retained
//! snapshot, checks each against its qualified index ordinal, reduces it to a
//! [`PictureSignature`] on a second thread and keeps only the last 51
//! signatures and the per-picture measures. A long scan offers checkpoints
//! that a later scan resumes from. Nothing here edits a project: shot
//! boundaries are proposals, stored as an annotation outside history.

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use deadpan_analysis::{
    PictureSignature, SHOT_RULE, SIGNATURE_VERSION, ShotAnalysis, ShotMeasurer, ShotProgress,
    ShotProgressTail,
};
use deadpan_media::picture_scan::{PictureScanError, scan_pictures_from};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_source::{DecodeLimits, DecodedRgbaFrame};
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
    /// Time spent reducing converted pictures to signatures, on the
    /// signature thread; it overlaps decoding.
    pub signature_elapsed: Duration,
    /// The first unmeasured picture of the progress this scan resumed.
    pub resumed_from: Option<usize>,
    /// Pictures decoded and measured by this scan, including the pictures
    /// decoded again before the resumed one.
    pub decoded: usize,
}

/// Codec threads of the scan's decoder. FFmpeg frame and slice threading
/// return the same pictures bit for bit (checked against single-threaded
/// scans in the media tests); this bounds the background scan's share of
/// the machine. See docs/SHOT_DETECTION.md for the measurements.
pub const SHOT_DECODE_THREADS: u32 = 4;
/// How often a scan offers its progress for saving.
pub const SHOT_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(10);
/// How often `detect-shots` saves its progress. Each checkpoint opens the
/// package's writer briefly (so the project stays openable elsewhere), and a
/// writable open revalidates the package, so the command saves less often
/// than the app, whose project service already holds the writer.
pub const CLI_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(60);
/// A checkpoint is skipped when less time than this remains before the
/// scan's deadline, so saving never delays the scan past it.
pub const CHECKPOINT_DEADLINE_MARGIN: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct ShotScanOptions {
    /// Saved progress of the same key to continue. Progress of a different
    /// picture count is ignored and the scan starts over.
    pub resume: Option<ShotProgress>,
    /// Minimum time between `checkpoint` calls; zero offers every picture.
    pub checkpoint_interval: Duration,
    pub decode_threads: u32,
}

impl Default for ShotScanOptions {
    fn default() -> Self {
        Self {
            resume: None,
            checkpoint_interval: SHOT_CHECKPOINT_INTERVAL,
            decode_threads: SHOT_DECODE_THREADS,
        }
    }
}

/// The qualification receipt whose pictures to scan: the single-Original
/// project's Original by default, or an explicitly registered source.
pub(crate) fn shot_receipt(
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

/// Saved scan progress for `input`, if a readable checkpoint of the same
/// pictures exists. Progress is rebuildable, so an unreadable row is
/// skipped and the scan starts over.
pub fn stored_shot_progress(
    store: &deadpan_store::ProjectStore,
    input: &ShotInput,
) -> Option<ShotProgress> {
    store
        .shot_scan_progress(&input.key, input.pictures())
        .ok()
        .flatten()
}

/// Decode and measure every picture, continuing `options.resume` when
/// given. Signatures are computed on a second thread while the next picture
/// decodes. `progress(measured, total)` follows each measured picture and
/// `checkpoint` receives, at most every `options.checkpoint_interval`, the
/// measures changed since the previous checkpoint (or since the resumed
/// progress) to append to the saved progress. It returns whether it saved
/// (or handed off) the tail; after a refused tail the next one carries every
/// measure, replacing whatever progress was saved. No checkpoint is offered once the scan is cancelled or
/// within [`CHECKPOINT_DEADLINE_MARGIN`] of its deadline. Keep both cheap.
pub fn scan_shots(
    input: &ShotInput,
    cancelled: &AtomicBool,
    deadline: Instant,
    options: ShotScanOptions,
    mut progress: impl FnMut(usize, usize),
    mut checkpoint: impl FnMut(ShotProgressTail) -> bool,
) -> Result<ShotScan, ShotScanError> {
    let started = Instant::now();
    let video = input.receipt.snapshot().video().ok_or_else(|| {
        ShotScanError::Unavailable("the Original has no qualified picture".into())
    })?;
    let total = video.index().index().frames().len();
    let failed = |error: deadpan_analysis::ShotError| ShotScanError::Failed(error.to_string());
    let mut measurer = match options.resume.filter(|resume| resume.pictures() == total) {
        Some(resume) => ShotMeasurer::resume(resume),
        None => ShotMeasurer::new(total),
    }
    .map_err(failed)?;
    let resumed_from = (measurer.measured() > 0).then_some(measurer.measured());
    let start = measurer.next_ordinal();
    let limits = DecodeLimits {
        threads: options.decode_threads,
        ..DecodeLimits::default()
    };
    let mut signature_elapsed = Duration::ZERO;
    let mut last_checkpoint = Instant::now();
    let mut decoded = 0;
    std::thread::scope(|scope| -> Result<(), ShotScanError> {
        let (pictures, received) = mpsc::sync_channel::<(usize, DecodedRgbaFrame)>(2);
        let (reduced, signatures) = mpsc::channel();
        std::thread::Builder::new()
            .name("deadpan-shot-signatures".into())
            .spawn_scoped(scope, move || {
                for (ordinal, picture) in received {
                    let began = Instant::now();
                    let signature = PictureSignature::from_rgba(
                        &picture.rgba,
                        picture.width,
                        picture.height,
                        picture.row_stride_bytes,
                    );
                    let stop = signature.is_err();
                    if reduced
                        .send(signature.map(|signature| (ordinal, signature, began.elapsed())))
                        .is_err()
                        || stop
                    {
                        break;
                    }
                }
            })
            .map_err(|error| ShotScanError::Failed(error.to_string()))?;
        let mut absorb = |result: Result<(usize, PictureSignature, Duration), _>| {
            let (ordinal, signature, took) = result.map_err(failed)?;
            measurer.push(ordinal, signature).map_err(failed)?;
            signature_elapsed += took;
            decoded += 1;
            let measured = measurer.measured();
            progress(measured, total);
            if measured < total
                && last_checkpoint.elapsed() >= options.checkpoint_interval
                && checkpoint_allowed(cancelled, deadline)
            {
                if !checkpoint(measurer.take_tail()) {
                    // The saved progress may be older than the tail or gone;
                    // the next checkpoint replaces it from the first picture.
                    measurer.unsave_from(0);
                }
                last_checkpoint = Instant::now();
            }
            Ok::<_, ShotScanError>(())
        };
        scan_pictures_from(
            &input.input,
            video,
            limits,
            deadline,
            cancelled,
            start,
            |ordinal, picture| {
                pictures
                    .send((ordinal, picture))
                    .map_err(|_| "the signature thread stopped".to_string())?;
                while let Ok(result) = signatures.try_recv() {
                    absorb(result).map_err(|error| error.to_string())?;
                }
                Ok::<_, String>(())
            },
        )?;
        drop(pictures);
        for result in signatures {
            absorb(result)?;
        }
        Ok(())
    })?;
    let analysis = measurer.finish().map_err(failed)?;
    Ok(ShotScan {
        key: input.key.clone(),
        analysis,
        elapsed: started.elapsed(),
        signature_elapsed,
        resumed_from,
        decoded,
    })
}

/// Whether a scan may still save a checkpoint: not cancelled and not
/// within [`CHECKPOINT_DEADLINE_MARGIN`] of its deadline.
pub fn checkpoint_allowed(cancelled: &AtomicBool, deadline: Instant) -> bool {
    !cancelled.load(std::sync::atomic::Ordering::Acquire)
        && Instant::now()
            .checked_add(CHECKPOINT_DEADLINE_MARGIN)
            .is_some_and(|latest| latest < deadline)
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
/// the analysis outside history. A scan interrupted earlier continues from
/// its last saved checkpoint.
pub fn run_detect_shots(arguments: &[&str]) -> Result<(), crate::CliError> {
    const USAGE: &str =
        "usage: detect-shots <project.deadpan> [--asset <id>] [--decode-threads <1-16>]";
    // `--decode-threads` overrides the codec thread count, for measurement.
    let mut arguments = arguments.to_vec();
    let mut decode_threads = SHOT_DECODE_THREADS;
    if let Some(position) = arguments
        .iter()
        .position(|item| *item == "--decode-threads")
    {
        decode_threads = arguments
            .get(position + 1)
            .and_then(|value| value.parse().ok())
            .filter(|value| (1..=16).contains(value))
            .ok_or_else(|| crate::CliError::Usage(USAGE.into()))?;
        arguments.drain(position..position + 2);
    }
    let arguments = arguments.as_slice();
    let asset = parse_asset(arguments, USAGE)?;
    let path = std::path::Path::new(arguments[0]);
    // Read-only while scanning; the writer is taken only to save.
    let reader = deadpan_store::ProjectStore::open(path, deadpan_store::AccessMode::ReadOnly)?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(6 * 60 * 60);
    let input = prepare_shot_input(&reader, asset.as_ref(), &cancelled, deadline)?;
    let resume = stored_shot_progress(&reader, &input);
    drop(reader);
    let (mut saved, mut unsaved) = (0_u64, 0_u64);
    let options = ShotScanOptions {
        resume,
        decode_threads,
        checkpoint_interval: CLI_CHECKPOINT_INTERVAL,
    };
    let scan = scan_shots(
        &input,
        &cancelled,
        deadline,
        options,
        |_, _| {},
        |tail| {
            // Checkpoints are best effort: the writer is taken briefly and
            // only the changed measures are appended. A project open
            // elsewhere keeps the previous checkpoint, and the refused
            // measures join the next one. The writer open can take a while,
            // so check cancellation and the deadline again before writing.
            let appended =
                deadpan_store::ProjectStore::open(path, deadpan_store::AccessMode::ReadWrite)
                    .ok()
                    .filter(|_| checkpoint_allowed(&cancelled, deadline))
                    .is_some_and(|writer| {
                        writer.append_shot_scan_progress(&input.key, &tail).is_ok()
                    });
            if appended {
                saved += 1;
            } else {
                unsaved += 1;
            }
            appended
        },
    )?;
    let writer = deadpan_store::ProjectStore::open(path, deadpan_store::AccessMode::ReadWrite)?;
    writer.save_shot_analysis(&scan.key, &scan.analysis)?;
    drop(writer);
    let pictures = scan.analysis.pictures();
    let seconds = scan.elapsed.as_secs_f64();
    let boundaries = scan.analysis.boundaries();
    crate::write_json(&serde_json::json!({
        "protocol": 1,
        "rule": SHOT_RULE,
        "key": scan.key,
        "pictures": pictures,
        "boundaries": boundaries,
        "cuts": scan.analysis.cuts(),
        "transitions": scan.analysis.transitions().iter().map(|transition| serde_json::json!({
            "kind": transition.kind,
            "first": transition.range.start,
            "end": transition.range.end,
            "boundary": transition.boundary,
        })).collect::<Vec<_>>(),
        "shots": boundaries.len() + usize::from(pictures > 0),
        "resumed_from": scan.resumed_from,
        "decoded_pictures": scan.decoded,
        "checkpoints_saved": saved,
        "checkpoints_unsaved": unsaved,
        "decode_threads": decode_threads,
        "elapsed_ms": millis(scan.elapsed),
        "signature_elapsed_ms": millis(scan.signature_elapsed),
        "pictures_per_second": if seconds > 0.0 { scan.decoded as f64 / seconds } else { 0.0 },
    }))
}

/// `shots PROJECT [--asset ID]`: print the stored analysis's boundaries with
/// exact picture ordinals and PTS, and its gradual transitions.
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
            "transitions": [],
        }));
    };
    let base = index.time_base();
    let time = |picture: usize| -> Result<serde_json::Value, crate::CliError> {
        let pts = frames[picture].pts;
        let exact = deadpan_core::ExactRatio::new(
            i128::from(pts) * i128::from(base.numerator()),
            i128::from(base.denominator()),
        )?;
        Ok(serde_json::json!({
            "pts": pts,
            "seconds_exact": exact,
            "seconds": exact.numerator() as f64 / exact.denominator() as f64,
        }))
    };
    let transitions = analysis.transitions();
    let boundaries = analysis
        .boundaries()
        .into_iter()
        .map(|picture| {
            let [cell, histogram, skip] = analysis.change(picture).unwrap_or_default();
            let mut entry = time(picture)?;
            entry["picture"] = picture.into();
            entry["kind"] = match transitions
                .iter()
                .find(|transition| transition.boundary == picture)
            {
                Some(transition) => serde_json::to_value(transition.kind)?,
                None => "cut".into(),
            };
            entry["cell_change"] = cell.into();
            entry["histogram_change"] = histogram.into();
            entry["change_from_two_before"] = skip.into();
            Ok(entry)
        })
        .collect::<Result<Vec<_>, crate::CliError>>()?;
    let transitions = transitions
        .iter()
        .map(|transition| {
            Ok(serde_json::json!({
                "kind": transition.kind,
                "first": transition.range.start,
                "end": transition.range.end,
                "boundary": transition.boundary,
                "start": time(transition.range.start)?,
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
        "transitions": transitions,
    }))
}
