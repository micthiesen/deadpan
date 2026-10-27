use super::*;
use sha2::Digest;

#[test]
fn complete_picture_limit_is_admitted_before_local_range_work() {
    let options = ShotRangeOptions::default();
    // Small local contexts must not conceal an impossible complete result.
    // Exercise these bounds without constructing a huge index or cache.
    assert!(options.range_pictures < MAX_SHOT_PICTURES);
    for pictures in [0, MAX_SHOT_PICTURES + 1, usize::MAX] {
        assert!(matches!(
            validate_bounds(pictures, &options),
            Err(ShotScanError::Failed(message)) if message == "shot picture count bound"
        ));
    }
    assert!(validate_bounds(1, &options).is_ok());
    assert!(validate_bounds(MAX_SHOT_PICTURES, &options).is_ok());
}

fn signatures() -> Vec<PictureSignature> {
    (0..260)
        .map(|frame| {
            let rgba: Vec<_> = (0..36)
                .flat_map(|y| {
                    (0..64).flat_map(move |x| {
                        let warm = [210_u32, 100 + x, 20];
                        let blue = [10_u32, 20, 120 + y];
                        let color = if frame < 64 {
                            warm
                        } else if frame < 89 {
                            let blend = frame - 63;
                            std::array::from_fn(|channel| {
                                (warm[channel] * (26 - blend) + blue[channel] * blend) / 26
                            })
                        } else if frame < 128 {
                            blue
                        } else if frame == 191 {
                            [255; 3]
                        } else {
                            [20, 180 + x / 2, 40 + y]
                        };
                        [color[0] as u8, color[1] as u8, color[2] as u8, 255]
                    })
                })
                .collect();
            PictureSignature::from_rgba(&rgba, 64, 36, 64 * 4).unwrap()
        })
        .collect()
}

fn key(owned: Range<usize>, context: Range<usize>, pictures: usize) -> Key {
    Key {
        implementation: SHOT_RANGE_IMPLEMENTATION.into(),
        rule: SHOT_RULE.into(),
        source: deadpan_store::ShotAnalysisKey {
            content: "a".repeat(64),
            video_stream: 0,
            signature_version: deadpan_analysis::SIGNATURE_VERSION.into(),
            interpretation: None,
        },
        pictures,
        owned,
        context,
    }
}

#[test]
fn out_of_order_context_ranges_equal_every_sequential_measure_and_boundary() {
    let pictures = signatures();
    let complete = ShotAnalysis::from_signatures(&pictures).unwrap();
    let total = pictures.len();
    let mut schedule = RangeSchedule::new(total as u64, 64, REPLAY_PICTURES as u64).unwrap();
    let mut pieces = vec![None; total.div_ceil(64)];
    let mut priority = u64::MAX;
    let mut order = Vec::new();
    while let Some(range) = schedule.next(priority).unwrap() {
        let key = key(
            ordinals(&range.owned).unwrap(),
            ordinals(&range.context).unwrap(),
            total,
        );
        let context = ShotAnalysis::from_signatures(&pictures[key.context.clone()]).unwrap();
        let cached = CachedRange {
            key: key.clone(),
            measures: encode_measures(context.measures()),
        };
        let owned = cached.restore(&key).unwrap();
        assert_eq!(owned, complete.measures()[key.owned.clone()]);
        pieces[key.owned.start / 64] = Some(owned);
        order.push(key.owned);
        schedule.finish(&range).unwrap();
        priority = if order.len() == 1 { 0 } else { 128 };
    }
    assert_eq!(order[0], 256..260);
    assert_eq!(order[1], 0..64);
    assert_eq!(order[2], 128..192);
    let assembled =
        ShotAnalysis::new(pieces.into_iter().flat_map(Option::unwrap).collect()).unwrap();
    assert_eq!(assembled, complete);
    assert_eq!(assembled.boundaries(), complete.boundaries());
    assert!(assembled.boundaries().contains(&128));
    assert!(!assembled.transitions().is_empty());
    assert!(
        !assembled.boundaries().contains(&191),
        "returning flash is not a shot"
    );
}

#[test]
fn cache_refuses_source_window_identity_and_incomplete_context() {
    let pictures = signatures();
    let expected = key(64..128, 14..178, pictures.len());
    let measured = ShotAnalysis::from_signatures(&pictures[expected.context.clone()]).unwrap();
    let record = CachedRange {
        key: expected.clone(),
        measures: encode_measures(measured.measures()),
    };
    assert_eq!(record.restore(&expected).unwrap().len(), 64);
    let mut other = expected.clone();
    other.source.content = "b".repeat(64);
    assert!(record.restore(&other).is_err());
    other = expected.clone();
    other.implementation.push_str("-new");
    assert!(record.restore(&other).is_err());
    other = expected.clone();
    other.owned = 65..129;
    assert!(record.restore(&other).is_err());
    let missing = key(64..128, 64..128, pictures.len());
    let measured = ShotAnalysis::from_signatures(&pictures[64..128]).unwrap();
    assert!(owned_measures(&missing, measured).is_err());
    let truncated = CachedRange {
        key: expected.clone(),
        measures: record.measures[..record.measures.len() - 1].to_vec(),
    };
    assert!(truncated.restore(&expected).is_err());
}

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn registered_fixture(
    scratch: &std::path::Path,
) -> TestResult<(deadpan_store::ProjectStore, ShotInput)> {
    const ENCODED_SHA256: &str = "1b959be2cb9f0b23e334a21997f6b9b6a77fd9cd0d104a920f3b615ecf9ffad6";
    let bytes = include_bytes!("fixtures/controlled-scenes.mp4");
    let hash: String = sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(hash, ENCODED_SHA256);
    let source = scratch.join("source.mp4");
    std::fs::write(&source, bytes)?;
    let package = scratch.join("shots.deadpan");
    crate::single_original::create(
        &package,
        &source,
        "Procedural shot ranges",
        &AtomicBool::new(false),
        |_, _| Ok(()),
    )?;
    let store = deadpan_store::ProjectStore::open(&package, deadpan_store::AccessMode::ReadOnly)?;
    let input = super::super::prepare_shot_input(
        &store,
        None,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(60),
    )?;
    assert_eq!(input.pictures(), 260);
    Ok((store, input))
}

fn options(cache: Option<PathBuf>) -> ShotRangeOptions {
    ShotRangeOptions {
        range_pictures: 64,
        cache_directory: cache,
        ..Default::default()
    }
}

#[test]
fn actual_codec_ranges_equal_sequential_measures_cuts_dissolves_and_flash_policy() -> TestResult {
    let scratch = tempfile::tempdir()?;
    let (store, input) = registered_fixture(scratch.path())?;
    let revision = store.head_revision()?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let sequential = super::super::scan_shots(
        &input,
        &cancelled,
        deadline,
        Default::default(),
        |_, _| {},
        |_| true,
    )?;
    let cursor = AtomicU64::new(u64::MAX);
    let mut prior: Vec<Range<usize>> = Vec::new();
    let mut order = Vec::new();
    let scan = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(None),
        &cursor,
        || Ok(()),
        |update| {
            let fresh: Vec<_> = update
                .coverage
                .iter()
                .filter(|range| !prior.contains(*range))
                .cloned()
                .collect();
            assert_eq!(fresh.len(), 1);
            order.push(fresh[0].clone());
            assert_eq!(
                update.completed_pictures,
                update
                    .coverage
                    .iter()
                    .map(|range| range.end - range.start)
                    .sum::<usize>()
            );
            assert!(
                update
                    .coverage
                    .windows(2)
                    .all(|pair| pair[0].end <= pair[1].start)
            );
            prior = update.coverage;
            cursor.store(if order.len() == 1 { 0 } else { 128 }, Ordering::Release);
        },
    )?;
    assert_eq!(order, [256..260, 0..64, 128..192, 64..128, 192..256]);
    assert_eq!(
        encode_measures(scan.analysis.measures()),
        encode_measures(sequential.analysis.measures())
    );
    assert_eq!(scan.analysis.boundaries(), sequential.analysis.boundaries());
    assert_eq!(
        scan.analysis.transitions(),
        sequential.analysis.transitions()
    );
    assert!(scan.analysis.boundaries().contains(&128));
    assert!(!scan.analysis.transitions().is_empty());
    assert!(!scan.analysis.boundaries().contains(&191));
    assert!(
        scan.decoded > 260,
        "context is decoded, not filled with invented zero spans"
    );
    assert_eq!(store.head_revision()?, revision);
    Ok(())
}

#[test]
fn actual_cancelled_ranges_resume_retarget_and_corrupt_cache_rebuilds_exactly() -> TestResult {
    let scratch = tempfile::tempdir()?;
    let (store, input) = registered_fixture(scratch.path())?;
    let revision = store.head_revision()?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let cursor = AtomicU64::new(u64::MAX);
    let cache = scratch.path().join("ranges");
    let first = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(Some(cache.clone())),
        &cursor,
        || Ok(()),
        |update| {
            assert_eq!(
                update.coverage,
                std::iter::once(256..260).collect::<Vec<_>>()
            );
            assert_eq!(update.completed_pictures, 4);
            cancelled.store(true, Ordering::Release);
        },
    );
    assert!(matches!(first, Err(ShotScanError::Cancelled)));
    cancelled.store(false, Ordering::Release);
    let mut completion = 0;
    let resumed = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(Some(cache.clone())),
        &cursor,
        || Ok(()),
        |update| {
            completion += 1;
            if completion == 1 {
                assert_eq!(
                    update.coverage,
                    std::iter::once(256..260).collect::<Vec<_>>()
                );
                assert_eq!(update.cached_ranges, 1);
                cursor.store(0, Ordering::Release);
            } else if completion == 2 {
                assert_eq!(update.coverage, [0..64, 256..260]);
            }
        },
    )?;
    assert_eq!(completion, 5);
    assert_eq!(resumed.cached_ranges, 1);
    let cached = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(Some(cache.clone())),
        &cursor,
        || Ok(()),
        |_| {},
    )?;
    assert_eq!(cached.cached_ranges, 5);
    assert_eq!(cached.decoded, 0);
    assert_eq!(cached.signature_elapsed, Duration::ZERO);
    assert_eq!(cached.analysis, resumed.analysis);
    let record = std::fs::read_dir(&cache)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .ok_or("no cached range")?;
    std::fs::write(record, b"damaged range record")?;
    let repaired = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(Some(cache)),
        &cursor,
        || Ok(()),
        |_| {},
    )?;
    assert_eq!(repaired.cached_ranges, 4);
    assert_eq!(repaired.cache_failures, 1);
    assert!(repaired.decoded > 0);
    assert_eq!(repaired.analysis, resumed.analysis);
    assert_eq!(store.head_revision()?, revision);
    Ok(())
}

#[test]
fn actual_interpretation_changes_have_separate_range_keys() -> TestResult {
    let scratch = tempfile::tempdir()?;
    let (_, mut input) = registered_fixture(scratch.path())?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let cursor = AtomicU64::new(128);
    let cache = scratch.path().join("ranges");
    let original = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(Some(cache.clone())),
        &cursor,
        || Ok(()),
        |_| {},
    )?;
    input.key.interpretation = Some(deadpan_core::SourceInterpretation {
        range: Some(deadpan_core::SourceColorRange::Full),
        chroma_location: None,
    });
    let corrected = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(Some(cache.clone())),
        &cursor,
        || Ok(()),
        |_| {},
    )?;
    assert_eq!(corrected.cached_ranges, 0);
    assert_ne!(corrected.key, original.key);
    assert_ne!(corrected.analysis, original.analysis);
    let restored = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(Some(cache)),
        &cursor,
        || Ok(()),
        |_| {},
    )?;
    assert_eq!(restored.cached_ranges, 5);
    assert_eq!(restored.decoded, 0);
    assert_eq!(restored.analysis, corrected.analysis);
    Ok(())
}

#[test]
fn interruption_inside_a_context_publishes_no_range_and_static_bounds_precede_work() -> TestResult {
    let scratch = tempfile::tempdir()?;
    let (_, input) = registered_fixture(scratch.path())?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let cursor = AtomicU64::new(128);
    for invalid in [
        ShotRangeOptions {
            range_pictures: 0,
            ..options(None)
        },
        ShotRangeOptions {
            range_pictures: 8_193,
            ..options(None)
        },
        ShotRangeOptions {
            decode_threads: 0,
            ..options(None)
        },
        ShotRangeOptions {
            decode_threads: 17,
            ..options(None)
        },
    ] {
        assert!(matches!(
            scan_shot_ranges(
                &input,
                &cancelled,
                deadline,
                invalid,
                &cursor,
                || panic!("invalid configuration reached work"),
                |_| panic!("invalid configuration published progress")
            ),
            Err(ShotScanError::Failed(_))
        ));
    }
    let cache = scratch.path().join("ranges");
    let mut boundaries = 0;
    let result = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(Some(cache.clone())),
        &cursor,
        || {
            boundaries += 1;
            if boundaries > 8 {
                cancelled.store(true, Ordering::Release);
            }
            Ok(())
        },
        |_| panic!("an incomplete context cannot report completed coverage"),
    );
    assert!(matches!(result, Err(ShotScanError::Cancelled)));
    assert!(
        std::fs::read_dir(cache)?
            .filter_map(Result::ok)
            .all(|entry| entry
                .path()
                .extension()
                .is_none_or(|extension| extension != "json"))
    );
    Ok(())
}

#[test]
fn mutable_labels_and_retained_input_mismatch_are_refused_before_cached_work() -> TestResult {
    let scratch = tempfile::tempdir()?;
    let (_, mut input) = registered_fixture(scratch.path())?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let cursor = AtomicU64::new(128);
    let cache = scratch.path().join("ranges");
    let valid = scan_shot_ranges(
        &input,
        &cancelled,
        deadline,
        options(Some(cache.clone())),
        &cursor,
        || Ok(()),
        |_| {},
    )?;
    assert_eq!(valid.cached_ranges, 0);
    let original = input.key.clone();
    let mut foreign = original.clone();
    foreign.content = "blake3:foreign".into();
    let mut stream = original.clone();
    stream.video_stream += 1;
    let mut version = original.clone();
    version.signature_version = "unknown-signature".into();
    for key in [foreign, stream, version] {
        input.key = key;
        assert!(matches!(
            scan_shot_ranges(
                &input,
                &cancelled,
                deadline,
                options(Some(cache.clone())),
                &cursor,
                || panic!("mismatched key reached a decode boundary"),
                |_| panic!("mismatched key reused cached progress")
            ),
            Err(ShotScanError::Failed(_))
        ));
    }
    input.key = original;
    let bytes = b"different retained bytes";
    let identity = deadpan_media::source_index::SourceContentIdentity::new(
        sha2::Sha256::digest(bytes).into(),
        bytes.len() as u64,
    )?;
    input.input = deadpan_media::source_input::VerifiedSourceInput::copy_verified(
        &mut std::io::Cursor::new(bytes),
        identity,
        identity.byte_length(),
        Duration::from_secs(5),
        &cancelled,
    )?;
    assert!(matches!(
        scan_shot_ranges(
            &input,
            &cancelled,
            deadline,
            options(Some(cache)),
            &cursor,
            || panic!("mismatched retained input reached a decode boundary"),
            |_| panic!("mismatched retained input reused cached progress")
        ),
        Err(ShotScanError::Failed(_))
    ));
    Ok(())
}
