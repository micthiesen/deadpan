//! Preview proxy encoding through the real isolated worker, then independent
//! verification against the Original's measured index.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_core::{AssetId, SourceFrameId};
use deadpan_media::proxy::{
    ProxyError, ProxyIneligible, ProxyOriginal, ProxyPlan, ProxyReason, ProxySegment, ProxySidecar,
    VerifyControl, expressible, hex, proxy_assemble_request, proxy_plan, proxy_range_request,
    proxy_raster, proxy_request, proxy_segments, verify_proxy,
};
use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_media::{
    ConversionError, ProxyEncodeOptions, assemble_proxy, encode_proxy, encode_proxy_retrying,
};
use deadpan_source::{ColorMatrix, ColorPrimaries, ColorTransfer, SourceStreamInfo};
use sha2::{Digest, Sha256};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn fixture(name: &str) -> PathBuf {
    let own = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/proxy")
        .join(name);
    if own.is_file() {
        return own;
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../deadpan-source/tests/fixtures")
        .join(name)
}

fn worker() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker"))
}

struct Original {
    input: VerifiedSourceInput,
    index: Arc<SourceIndexSnapshot>,
    info: SourceStreamInfo,
    blake3: String,
}

fn original(name: &str) -> Result<Original> {
    let bytes = std::fs::read(fixture(name))?;
    let identity = SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64)?;
    let cancelled = AtomicBool::new(false);
    let input = VerifiedSourceInput::copy_verified(
        &mut bytes.as_slice(),
        identity,
        1 << 30,
        Duration::from_secs(60),
        &cancelled,
    )?;
    let session = SourceSession::open_input(
        input.clone(),
        AssetId::new("original")?,
        SourceSessionLimits::default(),
        &cancelled,
    )?;
    Ok(Original {
        input,
        index: Arc::new(session.index().clone()),
        info: session.info().clone(),
        blake3: blake3::hash(&bytes).to_hex().to_string(),
    })
}

fn private_output(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?)
}

/// The recipe's raster for this Original, as an explicit plan.
fn requested_plan(original: &Original) -> ProxyPlan {
    let (width, height) = proxy_raster(original.info.width, original.info.height);
    ProxyPlan {
        width,
        height,
        reason: ProxyReason::Requested,
    }
}

fn control(cancelled: &AtomicBool) -> VerifyControl<'_> {
    VerifyControl {
        timeout: Duration::from_secs(120),
        cancelled,
        pause: None,
    }
}

/// Stall period for these tiny fixtures. A healthy worker writes or beats at
/// least every 250 ms; production uses `PROXY_STALL_TIMEOUT` (60 s).
///
/// `VT_HANG`: under concurrent load (eight test processes encoding at once)
/// about one encode in a thousand blocked indefinitely inside
/// `VTCompressionSessionCompleteFrames`, in a synchronous XPC request to the
/// VideoToolbox encoder service that never received a reply (sampled stacks
/// in docs/qualification/proxy-2026-10-05.md).
const TEST_STALL: Duration = Duration::from_secs(20);

/// One VideoToolbox proxy session at a time across this suite's test
/// processes (nextest runs each test in its own), as the app's per-user
/// encoder slot does. Concurrent sessions are what made the encoder service
/// hang (`VT_HANG`); stall-and-retry still covers a hang that happens anyway.
fn vt_slot() -> File {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(std::env::temp_dir().join("deadpan-proxy-tests-videotoolbox.lock"))
        .unwrap();
    rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive).unwrap();
    file
}

fn build(original: &Original, scratch: &Path) -> Result<(File, ProxySidecar)> {
    let plan = requested_plan(original);
    let frames = original.index.index().frames().len() as u64;
    let request = proxy_request(
        original.input.identity().byte_length(),
        &original.info,
        frames,
        &plan,
        Duration::from_secs(120),
    )
    .expect("SDR proxy request");
    let cancelled = AtomicBool::new(false);
    // The production path: VideoToolbox's encoder service occasionally never
    // answers a synchronous request (see `VT_HANG` below), which the stall
    // watch stops and one retry in a new process recovers.
    let staged = std::sync::Mutex::new(Vec::new());
    let (path, report) = encode_proxy_retrying(
        worker(),
        &original.input,
        &request,
        || stage(scratch, &staged),
        &cancelled,
        ProxyEncodeOptions {
            stall: TEST_STALL,
            pause: None,
        },
    )?;
    // The descriptor outlives the temporary path's removal.
    let output = File::open(&path)?;
    drop(path);
    assert_eq!(report.frames, frames);
    assert_eq!(report.keyframes, frames);
    let sidecar = verify_proxy(
        &output,
        &original.input,
        &original.blake3,
        Arc::clone(&original.index),
        &original.info,
        plan.reason,
        control(&cancelled),
    )?;
    Ok((output, sidecar))
}

fn identity(original: &Original) -> ProxyOriginal {
    ProxyOriginal {
        blake3: original.blake3.clone(),
        sha256: hex(&original.input.identity().sha256()),
        byte_length: original.input.identity().byte_length(),
        stream_index: original.index.stream_index(),
    }
}

#[test]
fn every_proxy_picture_keeps_its_original_time_and_duration() -> Result {
    let _slot = vt_slot();
    for name in [
        "cfr-bframes.mp4",
        "offset-bframes.mp4",
        "pyramid-bframes.mp4",
        "vfr.mp4",
        "fake-interlaced.mp4",
    ] {
        let original = original(name)?;
        let scratch = tempfile::tempdir()?;
        let (file, sidecar) = build(&original, scratch.path())?;
        let frames = original.index.index().frames();
        let proxy_frames = sidecar.index.index().frames();
        assert_eq!(proxy_frames.len(), frames.len(), "{name}");
        for (proxy, source) in proxy_frames.iter().zip(frames) {
            assert!(proxy.keyframe, "{name}: every proxy picture is intra");
            assert_eq!(proxy.seek_from, Some(proxy.identity), "{name}");
            // Same clock here, so equal ticks.
            assert_eq!(proxy.pts, source.pts, "{name}");
            assert_eq!(proxy.reported_duration, source.reported_duration, "{name}");
        }
        assert_eq!(
            sidecar.index.index().terminal_end(),
            original.index.index().terminal_end()
        );
        eprintln!(
            "{name}: {:?} {} bytes",
            sidecar.fidelity, sidecar.file.byte_length
        );
        sidecar.validate_for(&identity(&original), original.index.index(), &original.info)?;
        // Regenerate the app's committed proxy fixture on request.
        if let Some(directory) = std::env::var_os("DEADPAN_PROXY_FIXTURE_DIR") {
            let mut copy = File::create(Path::new(&directory).join(format!("{name}.proxy.mp4")))?;
            std::io::copy(&mut &file, &mut copy)?;
        }
        // The sidecar round-trips and still validates.
        let reread = ProxySidecar::from_json(&sidecar.to_json()?)?;
        assert_eq!(reread, sidecar);

        // Reading the published file in place decodes every proxy picture at
        // its Original ordinal, checked against the sidecar index.
        let cancelled = AtomicBool::new(false);
        let input = VerifiedSourceInput::from_verified_file(file.try_clone()?, sidecar.content())?;
        let mut session = SourceSession::open_input_indexed(
            input,
            Arc::new(sidecar.index.clone()),
            &sidecar.info,
            SourceSessionLimits::interactive(),
            &cancelled,
        )?;
        for ordinal in [frames.len() as u64 - 1, 0, 37, 38, 5] {
            let picture =
                session.frame(SourceFrameId(ordinal), Duration::from_secs(10), &cancelled)?;
            assert_eq!(picture.metadata.pts, frames[ordinal as usize].pts);
        }
    }
    Ok(())
}

/// The downscale path, rotation, non-square sample aspect and non-BT.709
/// matrix, primaries and transfer: the proxy keeps the Original's
/// interpretation, and the sampled colors match without bias.
#[test]
fn downscaled_rotated_anamorphic_and_wide_gamut_originals_keep_their_interpretation() -> Result {
    let _slot = vt_slot();
    for (name, raster) in [
        ("uhd-bt709.mp4", (1920, 1080)),
        ("anamorphic-bt601.mp4", (1440, 1080)),
        ("p3-srgb.mp4", (640, 360)),
        ("rotated-bt709.mp4", (320, 180)),
    ] {
        let original = original(name)?;
        let scratch = tempfile::tempdir()?;
        if name == "uhd-bt709.mp4" {
            // Policy chooses this one by raster alone.
            let plan = proxy_plan(&original.info, original.index.index())?.unwrap();
            assert_eq!(
                (plan.width, plan.height, plan.reason),
                (1920, 1080, ProxyReason::Raster)
            );
        }
        let (_, sidecar) = build(&original, scratch.path())?;
        let proxy = &sidecar.info;
        assert_eq!((proxy.width, proxy.height), raster, "{name}");
        assert_eq!(
            u64::from(proxy.sample_aspect_num) * u64::from(original.info.sample_aspect_den),
            u64::from(original.info.sample_aspect_num) * u64::from(proxy.sample_aspect_den),
            "{name}: sample aspect"
        );
        assert_eq!(
            proxy.rotation_quarter_turns, original.info.rotation_quarter_turns,
            "{name}"
        );
        assert_eq!(proxy.color.matrix, ColorMatrix::Bt709, "{name}");
        assert_eq!(proxy.color.transfer, original.info.color.transfer, "{name}");
        assert_eq!(
            proxy.color.primaries, original.info.color.primaries,
            "{name}"
        );
        let bias = sidecar.fidelity.bias();
        eprintln!("{name}: {:?}", sidecar.fidelity);
        assert!(
            bias.iter().all(|channel| channel.abs() <= 1.5),
            "{name}: {bias:?}"
        );
        assert!(sidecar.fidelity.mean() <= 3.0, "{name}");
    }
    let anamorphic = original("anamorphic-bt601.mp4")?;
    assert_eq!(anamorphic.info.color.matrix, ColorMatrix::Bt601);
    assert_eq!(
        (
            anamorphic.info.sample_aspect_num,
            anamorphic.info.sample_aspect_den
        ),
        (4, 3)
    );
    let p3 = original("p3-srgb.mp4")?;
    assert_eq!(p3.info.color.primaries, ColorPrimaries::DisplayP3);
    assert_eq!(p3.info.color.transfer, ColorTransfer::Srgb);
    assert_ne!(
        original("rotated-bt709.mp4")?.info.rotation_quarter_turns,
        0
    );
    // A 4×2 full-range RGB picture with hard color edges cannot survive 4:2:0
    // subsampling; verification refuses it rather than serving wrong color.
    let tiny = original("rotated90.mp4")?;
    let scratch = tempfile::tempdir()?;
    assert!(matches!(
        build(&tiny, scratch.path())
            .map(|_| ())
            .unwrap_err()
            .downcast::<ProxyError>()
            .as_deref(),
        Ok(ProxyError::Fidelity { .. })
    ));
    Ok(())
}

/// Encode `original` in keyframe-aligned ranges of about `target` pictures,
/// each written after the previous one into one private file (out of picture
/// order, as a resumed build may), then join them at packet level and verify
/// the result as a single encoding is verified.
fn build_in_ranges(original: &Original, scratch: &Path, target: u64) -> Result<ProxySidecar> {
    let plan = requested_plan(original);
    let ranges = proxy_segments(original.index.index(), target);
    let data = private_output(&scratch.join("segments.bin"))?;
    let cancelled = AtomicBool::new(false);
    let mut segments = Vec::new();
    let mut digests = Vec::new();
    // Last range first: storage order is independent of picture order.
    for &(start, end) in ranges.iter().rev() {
        let offset = data.metadata()?.len();
        let request = proxy_range_request(
            original.input.identity().byte_length(),
            &original.info,
            original.index.index(),
            &plan,
            start,
            end,
            offset,
            Duration::from_secs(120),
        )
        .expect("SDR range request");
        let report = encode_proxy_retrying(
            worker(),
            &original.input,
            &request,
            || Ok(((), data.try_clone()?)),
            &cancelled,
            ProxyEncodeOptions {
                stall: TEST_STALL,
                pause: None,
            },
        )?
        .1;
        assert_eq!(
            (report.frames, report.keyframes),
            (end - start, end - start)
        );
        digests.push(report.extradata_sha256.clone());
        let range = request.range.unwrap();
        segments.push(ProxySegment {
            offset,
            length: report.output_bytes,
            frames: report.frames,
            start_pts: range.start_pts,
            end_pts: range.end_pts,
        });
    }
    segments.reverse();
    digests.dedup();
    assert_eq!(
        digests.len(),
        1,
        "every range has one decoder configuration"
    );
    let request = proxy_assemble_request(
        data.metadata()?.len(),
        &original.info,
        &plan,
        segments,
        Duration::from_secs(120),
    )?;
    let output = private_output(&scratch.join("proxy.mp4"))?;
    let report = assemble_proxy(
        worker(),
        &data,
        &request,
        &output,
        &cancelled,
        ProxyEncodeOptions::default(),
    )?;
    assert_eq!(report.packets, original.index.index().frames().len() as u64);
    Ok(verify_proxy(
        &output,
        &original.input,
        &original.blake3,
        Arc::clone(&original.index),
        &original.info,
        plan.reason,
        control(&cancelled),
    )?)
}

#[test]
fn ranges_joined_at_packet_level_verify_as_one_encoding() -> Result {
    let _slot = vt_slot();
    for (name, target, ranges) in [
        ("cfr-bframes.mp4", 30, 4),
        ("offset-bframes.mp4", 30, 4),
        ("vfr.mp4", 40, 3),
        ("pyramid-bframes.mp4", 30, 2),
        ("rotated-bt709.mp4", 1, 0),
        ("anamorphic-bt601.mp4", 1, 0),
        ("p3-srgb.mp4", 1, 0),
    ] {
        let original = original(name)?;
        let count = proxy_segments(original.index.index(), target).len();
        if ranges > 0 {
            assert_eq!(count, ranges, "{name}");
        }
        let scratch = tempfile::tempdir()?;
        let sidecar = build_in_ranges(&original, scratch.path(), target)?;
        let frames = original.index.index().frames();
        let proxy_frames = sidecar.index.index().frames();
        assert_eq!(proxy_frames.len(), frames.len(), "{name}");
        for (proxy, source) in proxy_frames.iter().zip(frames) {
            assert!(proxy.keyframe, "{name}");
            assert!(same(proxy.pts, &sidecar, source.pts, &original), "{name}");
        }
        let info = &sidecar.info;
        assert_eq!(
            info.rotation_quarter_turns, original.info.rotation_quarter_turns,
            "{name}"
        );
        assert_eq!(
            u64::from(info.sample_aspect_num) * u64::from(original.info.sample_aspect_den),
            u64::from(original.info.sample_aspect_num) * u64::from(info.sample_aspect_den),
            "{name}"
        );
        assert_eq!(
            info.color.primaries, original.info.color.primaries,
            "{name}"
        );
        assert_eq!(info.color.transfer, original.info.color.transfer, "{name}");
        eprintln!("{name}: {count} ranges, {:?}", sidecar.fidelity);
    }
    Ok(())
}

/// Equal exact times in the two streams' clocks.
fn same(proxy: i64, sidecar: &ProxySidecar, original_pts: i64, original: &Original) -> bool {
    let a = sidecar.index.index().time_base();
    let b = original.index.index().time_base();
    i128::from(proxy) * i128::from(a.numerator()) * i128::from(b.denominator())
        == i128::from(original_pts) * i128::from(b.numerator()) * i128::from(a.denominator())
}

#[test]
fn an_assembly_refuses_a_misplaced_or_damaged_range() -> Result {
    let _slot = vt_slot();
    let original = original("cfr-bframes.mp4")?;
    let plan = requested_plan(&original);
    let scratch = tempfile::tempdir()?;
    let data = private_output(&scratch.path().join("segments.bin"))?;
    let cancelled = AtomicBool::new(false);
    let mut segments = Vec::new();
    for (start, end) in proxy_segments(original.index.index(), 60) {
        let offset = data.metadata()?.len();
        let request = proxy_range_request(
            original.input.identity().byte_length(),
            &original.info,
            original.index.index(),
            &plan,
            start,
            end,
            offset,
            Duration::from_secs(120),
        )
        .expect("SDR range request");
        let (_, report) = encode_proxy_retrying(
            worker(),
            &original.input,
            &request,
            || Ok(((), data.try_clone()?)),
            &cancelled,
            ProxyEncodeOptions {
                stall: TEST_STALL,
                pause: None,
            },
        )?;
        let range = request.range.unwrap();
        segments.push(ProxySegment {
            offset,
            length: report.output_bytes,
            frames: report.frames,
            start_pts: range.start_pts,
            end_pts: range.end_pts,
        });
    }
    assert_eq!(segments.len(), 2);
    let assemble = |segments: Vec<ProxySegment>, name: &str| -> ConversionError {
        let request = proxy_assemble_request(
            data.metadata().unwrap().len(),
            &original.info,
            &plan,
            segments,
            Duration::from_secs(60),
        )
        .unwrap();
        let output = private_output(&scratch.path().join(name)).unwrap();
        assemble_proxy(
            worker(),
            &data,
            &request,
            &output,
            &cancelled,
            ProxyEncodeOptions::default(),
        )
        .unwrap_err()
    };
    // The second range's bytes claimed for the first range's times.
    let swapped = vec![
        ProxySegment {
            offset: segments[1].offset,
            length: segments[1].length,
            ..segments[0]
        },
        ProxySegment {
            offset: segments[0].offset,
            length: segments[0].length,
            ..segments[1]
        },
    ];
    let error = assemble(swapped, "swapped.mp4");
    assert!(
        matches!(&error, ConversionError::Worker { code, .. } if code == "invalid_media"),
        "{error}"
    );
    // A truncated range.
    let mut truncated = segments.clone();
    truncated[1].length -= 100;
    let error = assemble(truncated, "truncated.mp4");
    assert!(matches!(&error, ConversionError::Worker { .. }), "{error}");
    Ok(())
}

#[test]
fn a_sidecar_for_other_bytes_or_index_is_refused() -> Result {
    let _slot = vt_slot();
    let original = original("cfr-bframes.mp4")?;
    let other = self::original("offset-bframes.mp4")?;
    let scratch = tempfile::tempdir()?;
    let (_, sidecar) = build(&original, scratch.path())?;
    // Another Original's identity.
    assert!(matches!(
        sidecar.validate_for(&identity(&other), original.index.index(), &original.info),
        Err(ProxyError::Sidecar(_))
    ));
    // The same bytes but an index whose times differ (offset fixture).
    assert!(matches!(
        sidecar.validate_for(&identity(&original), other.index.index(), &original.info),
        Err(ProxyError::Correspondence(_))
    ));
    // A stale recipe, and a recorded color bias beyond tolerance.
    let mut stale = sidecar.clone();
    stale.recipe += 1;
    assert!(
        stale
            .validate_for(&identity(&original), original.index.index(), &original.info)
            .is_err()
    );
    let mut tinted = sidecar.clone();
    tinted.fidelity.bias_milli = [0, 2000, 0];
    assert!(
        tinted
            .validate_for(&identity(&original), original.index.index(), &original.info)
            .is_err()
    );
    Ok(())
}

#[test]
fn irregular_durations_and_fast_originals_get_no_proxy() -> Result {
    // Variable rate is fine while each duration is its presentation interval.
    let vfr = original("vfr.mp4")?;
    expressible(vfr.index.index())?;
    let frames = vfr.index.index().frames();
    let mut irregular = frames.to_vec();
    irregular[3].reported_duration = irregular[3].reported_duration.map(|value| value + 1);
    let irregular = deadpan_core::SourceFrameIndex::new(
        AssetId::new("original")?,
        vfr.index.index().time_base(),
        irregular,
        vfr.index.index().terminal_end(),
        vfr.index.index().terminal_provenance(),
    )?;
    assert_eq!(
        expressible(&irregular),
        Err(ProxyIneligible::IrregularDurations)
    );
    // At or below 1080p no Original gets a proxy, whatever its keyframe
    // spacing: threaded seeks there already meet the target.
    for name in [
        "cfr-bframes.mp4",
        "pyramid-bframes.mp4",
        "anamorphic-bt601.mp4",
    ] {
        let original = original(name)?;
        assert_eq!(
            proxy_plan(&original.info, original.index.index()),
            Ok(None),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn cancellation_stops_the_worker_and_leaves_no_success() -> Result {
    let _slot = vt_slot();
    let original = original("pyramid-bframes.mp4")?;
    let plan = requested_plan(&original);
    let request = proxy_request(
        original.input.identity().byte_length(),
        &original.info,
        original.index.index().frames().len() as u64,
        &plan,
        Duration::from_secs(120),
    )
    .expect("SDR proxy request");
    let scratch = tempfile::tempdir()?;
    let output = private_output(&scratch.path().join("proxy.mp4"))?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let trigger = Arc::clone(&cancelled);
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        trigger.store(true, Ordering::Release);
    });
    let started = std::time::Instant::now();
    let result = encode_proxy(
        worker(),
        &original.input,
        &request,
        &output,
        &cancelled,
        ProxyEncodeOptions::default(),
    );
    canceller.join().unwrap();
    match result {
        Err(ConversionError::Cancelled) => {
            assert!(started.elapsed() < Duration::from_secs(5));
        }
        // A tiny fixture may finish before the cancellation lands; the
        // process was still supervised to completion.
        Ok(report) => assert_eq!(report.frames, 96),
        Err(error) => panic!("unexpected {error}"),
    }
    // A pre-cancelled request never starts.
    let output = private_output(&scratch.path().join("second.mp4"))?;
    assert!(matches!(
        encode_proxy(
            worker(),
            &original.input,
            &request,
            &output,
            &AtomicBool::new(true),
            ProxyEncodeOptions::default(),
        ),
        Err(ConversionError::Cancelled)
    ));
    Ok(())
}

#[test]
fn a_wrong_plan_is_refused_by_the_worker() -> Result {
    let _slot = vt_slot();
    let original = original("cfr-bframes.mp4")?;
    let plan = requested_plan(&original);
    let mut request = proxy_request(
        original.input.identity().byte_length(),
        &original.info,
        original.index.index().frames().len() as u64,
        &plan,
        Duration::from_secs(60),
    )
    .expect("SDR proxy request");
    request.time_base_den += 1;
    let scratch = tempfile::tempdir()?;
    let output = private_output(&scratch.path().join("proxy.mp4"))?;
    let error = encode_proxy(
        worker(),
        &original.input,
        &request,
        &output,
        &AtomicBool::new(false),
        ProxyEncodeOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(&error, ConversionError::Worker { code, .. } if code == "invalid_media"),
        "{error}"
    );
    // Too few planned pictures.
    let request = proxy_request(
        original.input.identity().byte_length(),
        &original.info,
        10,
        &plan,
        Duration::from_secs(60),
    )
    .expect("SDR proxy request");
    let output = private_output(&scratch.path().join("short.mp4"))?;
    let error = encode_proxy(
        worker(),
        &original.input,
        &request,
        &output,
        &AtomicBool::new(false),
        ProxyEncodeOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(&error, ConversionError::Worker { code, .. } if code == "invalid_media"),
        "{error}"
    );
    Ok(())
}

/// A stand-in worker that misbehaves on its first `failures` runs (hanging
/// after writing a little, or refusing a packet) and runs the real worker
/// afterwards. Every run appends one line to `runs.log` and the hanging
/// runs record their process ID.
struct Stub {
    directory: tempfile::TempDir,
    path: PathBuf,
}

impl Stub {
    fn new(first: &str, failures: u32) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir()?;
        let root = directory.path().display().to_string();
        let misbehave = match first {
            // Write some bytes, then neither write nor beat again.
            "hang" => format!("echo $$ >> {root}/pids; printf partial; exec /bin/sleep 6001"),
            "invalid_packet" => r#"printf '{"status":"failure","code":"invalid_packet","message":"stub refused a packet"}\n' >&2; exit 1"#.to_owned(),
            other => panic!("unknown stub mode {other}"),
        };
        let script = format!(
            "#!/bin/sh\necho run >> {root}/runs.log\nrun=$(/usr/bin/wc -l < {root}/runs.log)\nif [ \"$run\" -le {failures} ]; then\n{misbehave}\nfi\nexec {} \"$@\"\n",
            worker().display()
        );
        let path = directory.path().join("worker.sh");
        std::fs::write(&path, script)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        Ok(Self { directory, path })
    }

    fn runs(&self) -> usize {
        std::fs::read_to_string(self.directory.path().join("runs.log"))
            .map_or(0, |log| log.lines().count())
    }

    fn hung_processes_alive(&self) -> bool {
        std::fs::read_to_string(self.directory.path().join("pids"))
            .unwrap_or_default()
            .lines()
            .any(|pid| {
                std::process::Command::new("/bin/kill")
                    .args(["-0", pid.trim()])
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|status| status.success())
            })
    }
}

/// Each attempt's output: a private temporary file, removed when dropped.
fn stage(
    scratch: &Path,
    staged: &std::sync::Mutex<Vec<PathBuf>>,
) -> std::result::Result<(tempfile::TempPath, File), ConversionError> {
    let file = tempfile::NamedTempFile::new_in(scratch)?;
    staged.lock().unwrap().push(file.path().to_owned());
    let (file, path) = file.into_parts();
    let output = file.try_clone()?;
    Ok((path, output))
}

fn retry_request(original: &Original) -> deadpan_media::proxy::ProxyRequest {
    proxy_request(
        original.input.identity().byte_length(),
        &original.info,
        original.index.index().frames().len() as u64,
        &requested_plan(original),
        Duration::from_secs(120),
    )
    .expect("SDR proxy request")
}

#[test]
fn a_stalled_worker_is_torn_down_and_retried_once_in_a_fresh_output() -> Result {
    let _slot = vt_slot();
    let original = original("cfr-bframes.mp4")?;
    let stub = Stub::new("hang", 1)?;
    let scratch = tempfile::tempdir()?;
    let staged = std::sync::Mutex::new(Vec::new());
    let cancelled = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let (path, report) = encode_proxy_retrying(
        &stub.path,
        &original.input,
        &retry_request(&original),
        || stage(scratch.path(), &staged),
        &cancelled,
        ProxyEncodeOptions {
            stall: Duration::from_secs(1),
            pause: None,
        },
    )?;
    assert_eq!(stub.runs(), 2, "one stalled run, one retry");
    assert!(started.elapsed() < Duration::from_secs(30));
    assert!(!stub.hung_processes_alive(), "the stalled group is gone");
    let staged = staged.into_inner().unwrap();
    assert_eq!(staged.len(), 2);
    assert!(
        !staged[0].exists(),
        "the stalled attempt's output was removed"
    );
    assert_eq!(&*path, staged[1].as_path());
    // The kept output is entirely the second run's, and verifies.
    let output = File::open(&path)?;
    assert_eq!(output.metadata()?.len(), report.output_bytes);
    verify_proxy(
        &output,
        &original.input,
        &original.blake3,
        Arc::clone(&original.index),
        &original.info,
        ProxyReason::Requested,
        control(&cancelled),
    )?;
    Ok(())
}

#[test]
fn a_second_stall_is_reported_without_another_retry() -> Result {
    let _slot = vt_slot();
    let original = original("cfr-bframes.mp4")?;
    let stub = Stub::new("hang", 2)?;
    let scratch = tempfile::tempdir()?;
    let staged = std::sync::Mutex::new(Vec::new());
    let error = encode_proxy_retrying(
        &stub.path,
        &original.input,
        &retry_request(&original),
        || stage(scratch.path(), &staged),
        &AtomicBool::new(false),
        ProxyEncodeOptions {
            stall: Duration::from_millis(800),
            pause: None,
        },
    )
    .map(|_| ())
    .unwrap_err();
    assert!(
        matches!(
            error,
            ConversionError::Stalled {
                torn_down: true,
                ..
            }
        ),
        "{error}"
    );
    assert_eq!(stub.runs(), 2);
    assert!(!stub.hung_processes_alive());
    assert!(
        staged
            .into_inner()
            .unwrap()
            .iter()
            .all(|path| !path.exists())
    );
    Ok(())
}

#[test]
fn a_refused_packet_is_retried_once_in_a_new_process() -> Result {
    let _slot = vt_slot();
    let original = original("cfr-bframes.mp4")?;
    let stub = Stub::new("invalid_packet", 1)?;
    let scratch = tempfile::tempdir()?;
    let staged = std::sync::Mutex::new(Vec::new());
    let (_, report) = encode_proxy_retrying(
        &stub.path,
        &original.input,
        &retry_request(&original),
        || stage(scratch.path(), &staged),
        &AtomicBool::new(false),
        ProxyEncodeOptions::default(),
    )?;
    assert_eq!(report.frames, 120);
    assert_eq!(stub.runs(), 2);
    // Two refusals are reported, not retried again.
    let twice = Stub::new("invalid_packet", 2)?;
    let error = encode_proxy_retrying(
        &twice.path,
        &original.input,
        &retry_request(&original),
        || stage(scratch.path(), &staged),
        &AtomicBool::new(false),
        ProxyEncodeOptions::default(),
    )
    .map(|_| ())
    .unwrap_err();
    assert!(
        matches!(&error, ConversionError::Worker { code, .. } if code == "invalid_packet"),
        "{error}"
    );
    assert_eq!(twice.runs(), 2);
    Ok(())
}

#[test]
fn a_paused_build_is_suspended_without_counting_as_a_stall() -> Result {
    let _slot = vt_slot();
    let original = original("pyramid-bframes.mp4")?;
    let scratch = tempfile::tempdir()?;
    // Suspended for 5 s against a 2 s stall period: counting paused time
    // would stall during the pause. A stall well after the pause ended is a
    // VideoToolbox hang (`VT_HANG`), which gets one more attempt.
    let pause_for = Duration::from_secs(5);
    let stall = Duration::from_secs(2);
    for attempt in 0..2 {
        let output = private_output(&scratch.path().join(format!("proxy-{attempt}.mp4")))?;
        let pause = Arc::new(AtomicBool::new(true));
        let release = Arc::clone(&pause);
        let resumer = std::thread::spawn(move || {
            std::thread::sleep(pause_for);
            release.store(false, Ordering::Release);
        });
        let started = std::time::Instant::now();
        let result = encode_proxy(
            worker(),
            &original.input,
            &retry_request(&original),
            &output,
            &AtomicBool::new(false),
            ProxyEncodeOptions {
                stall,
                pause: Some(&pause),
            },
        );
        resumer.join().unwrap();
        match result {
            Ok(report) => {
                assert_eq!(report.frames, 96);
                assert!(started.elapsed() >= pause_for);
                return Ok(());
            }
            Err(ConversionError::Stalled { .. })
                if attempt == 0 && started.elapsed() >= pause_for + stall => {}
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!("the second attempt returns");
}
