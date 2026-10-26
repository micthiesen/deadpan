//! Independent moving-target/scale oracle on a real qualified A/V Original.
//! Samples are authored deterministic regions, not tracker output or a claim
//! that the fixture's pictured subject follows those regions.

use super::*;
use deadpan_cli::audio::ProjectAudioSession;
use deadpan_core::{
    FollowScaleRange, FollowScaleTransition, SemanticInstruction, SemanticZoomCenter,
    TargetCorrection,
    framing_zoom::{ZoomCurve, ZoomKind},
};
use std::sync::atomic::AtomicBool;

pub const FRAMES: i64 = 120;
pub const SAMPLES: i64 = 192_192;
const GRID: i128 = 1_i128 << 32;

/// Round with the public exact-number primitive, independently of Framing's
/// interpolation helpers. The fixture's bounded numbers fit i128 directly.
fn grid(value: ExactRatio) -> Result<i128> {
    Ok(value.checked_mul(ExactRatio::new(GRID, 1)?)?.round_even()?)
}

fn quantized(value: ExactRatio) -> Result<ExactRatio> {
    ratio(grid(value)?, GRID)
}

fn scale(base: ExactRatio, frame: i64, ranged: bool, retuned: bool) -> Result<ExactRatio> {
    // The picture is sampled at owner position frame+1/2. Quantize progress
    // and actual endpoint scales separately, then round the weighted sum.
    // Multiplying by a rounded scale factor would give a different answer.
    if ranged && frame < 20 {
        return Ok(base);
    }
    let end_factor = if retuned {
        ratio(2, 1)?
    } else if ranged {
        ratio(8, 5)?
    } else {
        ratio(4, 3)?
    };
    let end = base.checked_mul(end_factor)?;
    if ranged && frame >= 80 {
        return Ok(end);
    }
    let t = grid(if ranged {
        ratio(i128::from(2 * (frame - 20) + 1), 120)?
    } else {
        ratio(i128::from(2 * frame + 1), i128::from(2 * FRAMES))?
    })?;
    let t = if retuned {
        ratio(t * t * (3 * GRID - 2 * t), GRID * GRID)?.round_even()?
    } else {
        t
    };
    let start = grid(if ranged {
        base.checked_mul(ratio(6, 5)?)?
    } else {
        base
    })?;
    let end = grid(end)?;
    ratio(
        ratio(start * (GRID - t) + end * t, GRID)?.round_even()?,
        GRID,
    )
}

fn expected_pose(frame: i64, ranged: bool, retuned: bool) -> Result<FramingPose> {
    if !(10..110).contains(&frame) {
        // No target region outside its half-open span. Fallback centers are
        // authored exact values, including 1/3, and must not be quantized.
        return Ok(FramingPose::new(
            ratio(1, 3)?,
            ratio(2, 5)?,
            scale(ratio(5, 4)?, frame, ranged, retuned)?,
        )?);
    }
    let x = match frame {
        10..50 => 250_000 + 10_000 * (frame - 10) + 5_000,
        // The next sample has a different state, so the last Tracked region
        // is held before loss; Lost keeps it until the manual correction.
        50..80 => 650_000,
        80..100 => 450_001,
        100..110 => 700_000,
        _ => unreachable!("span checked above"),
    };
    Ok(FramingPose::new(
        quantized(ratio(i128::from(x), 1_000_000)?)?,
        quantized(ratio(2, 5)?)?,
        quantized(scale(ratio(3, 2)?, frame, ranged, retuned)?)?,
    )?)
}

fn check_poses(
    document: &ProjectDocument,
    owner: &NodeId,
    ranged: bool,
    retuned: bool,
) -> Result<Value> {
    let plan = deadpan_plan::RenderPlan::compile(document)?;
    assert_eq!(document.duration()?.frames(), FRAMES);
    let mut evidence = Vec::new();
    for frame in 0..FRAMES {
        let picture = plan.picture(ProjectFrame(frame))?;
        let layer = picture
            .framing
            .iter()
            .find(|layer| !layer.escalation && layer.instance.node == *owner)
            .ok_or("followed Source framing layer")?;
        assert!(layer.instance.repeats.is_empty());
        assert_eq!(layer.local_position, ratio(i128::from(2 * frame + 1), 2)?);
        assert_eq!(layer.effective_duration, ExactRatio::integer(FRAMES));
        let expected = expected_pose(frame, ranged, retuned)?;
        assert_eq!(layer.pose, Some(expected), "follow creep frame {frame}");
        evidence.push(json!({
            "frame":frame,"owner_position":layer.local_position,
            "expected":expected,"actual":layer.pose,
            "branch":if !(10..110).contains(&frame) { "fallback" }
                else if (60..80).contains(&frame) { "lost_holds_region" }
                else if (80..100).contains(&frame) { "manual_correction" }
                else { "tracked" },
        }));
    }
    Ok(json!(evidence))
}

#[derive(Debug, PartialEq, serde::Serialize)]
struct Pcm {
    authored: Vec<[u32; 2]>,
    limited: Vec<[u32; 2]>,
    limiter_gain: Vec<u64>,
}

fn pcm(package: &Path, partitions: &[u32]) -> Result<Pcm> {
    assert!(!partitions.is_empty());
    assert!(partitions.iter().all(|count| (1..=256).contains(count)));
    let mut session = ProjectAudioSession::open(package)?;
    assert_eq!(session.plan().audio_duration()?.0, SAMPLES);
    let cancelled = AtomicBool::new(false);
    let mut output = Pcm {
        authored: Vec::new(),
        limited: Vec::new(),
        limiter_gain: Vec::new(),
    };
    let mut at = 0;
    let mut partition = 0;
    while at < SAMPLES {
        let count = u32::try_from((SAMPLES - at).min(i64::from(partitions[partition])))?;
        let authored = session.read_authored_bus(AudioSample(at), count, &cancelled)?;
        let limited = session.read_limited(AudioSample(at), count, &cancelled)?;
        assert_eq!(authored.start, AudioSample(at));
        assert_eq!(limited.start, AudioSample(at));
        assert_eq!(authored.samples.len(), usize::try_from(count)?);
        assert_eq!(limited.samples.len(), usize::try_from(count)?);
        assert_eq!(limited.gain.len(), usize::try_from(count)?);
        for sample in authored.samples.iter().chain(&limited.samples) {
            assert!(sample.iter().all(|value| value.is_finite()));
        }
        assert!(limited.gain.iter().all(|value| value.is_finite()));
        output.authored.extend(
            authored
                .samples
                .into_iter()
                .map(|pair| pair.map(f32::to_bits)),
        );
        output.limited.extend(
            limited
                .samples
                .into_iter()
                .map(|pair| pair.map(f32::to_bits)),
        );
        output
            .limiter_gain
            .extend(limited.gain.into_iter().map(f64::to_bits));
        at += i64::from(count);
        partition = (partition + 1) % partitions.len();
    }
    assert_eq!(output.authored.len(), usize::try_from(SAMPLES)?);
    assert_eq!(output.limited.len(), usize::try_from(SAMPLES)?);
    assert_eq!(output.limiter_gain.len(), usize::try_from(SAMPLES)?);
    Ok(output)
}

fn same_pcm(actual: &Pcm, expected: &Pcm, context: &str) {
    for (kind, actual, expected) in [
        ("authored", &actual.authored, &expected.authored),
        ("limited", &actual.limited, &expected.limited),
    ] {
        assert_eq!(actual.len(), expected.len(), "{context}: {kind} length");
        for (sample, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(actual, expected, "{context}: {kind} sample {sample}");
        }
    }
    assert_eq!(actual.limiter_gain.len(), expected.limiter_gain.len());
    for (sample, (actual, expected)) in actual
        .limiter_gain
        .iter()
        .zip(&expected.limiter_gain)
        .enumerate()
    {
        assert_eq!(actual, expected, "{context}: limiter gain {sample}");
    }
}

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

pub fn build(directory: &Path) -> Result<Fixture> {
    build_with_range(directory, false, false)
}

pub fn build_ranged(directory: &Path) -> Result<Fixture> {
    build_with_range(directory, true, false)
}

pub fn build_retuned(directory: &Path, ranged: bool) -> Result<Fixture> {
    build_with_range(directory, ranged, true)
}

fn build_with_range(directory: &Path, ranged: bool, retuned: bool) -> Result<Fixture> {
    let mut project = Project::create(
        directory,
        if ranged {
            "ranged-follow-creep"
        } else {
            "follow-creep"
        },
    )?;
    let asset = project.asset.clone();
    let owner = root_child_at(&project.document()?, 0)?.0;
    assert!(matches!(
        project.document()?.nodes()[&owner].kind,
        NodeKind::Source { .. }
    ));
    let target = TargetId::new("moving-region")?;
    project.apply(|_, document, _| {
        let (time_base, per_frame) = original_clock(document, &asset)?;
        let stamp = |frame| SourceTimestamp {
            ticks: frame * per_frame,
            time_base,
        };
        let region = |x| TargetRegion {
            center: [x, 400_000],
            size: [100_000, 100_000],
        };
        Ok(Command::SetTarget {
            id: target.clone(),
            target: AttentionTarget {
                label: "Deterministic moving region".into(),
                asset: asset.clone(),
                span: SourceSpan::new(stamp(10), stamp(110))?,
                region: region(250_000).into(),
                samples: vec![
                    TargetSample {
                        at: 10 * per_frame,
                        region: region(250_000).into(),
                        confidence: 1000,
                        state: TrackState::Tracked,
                    },
                    TargetSample {
                        at: 50 * per_frame,
                        region: region(650_000).into(),
                        confidence: 1000,
                        state: TrackState::Tracked,
                    },
                    TargetSample {
                        at: 60 * per_frame,
                        region: region(650_000).into(),
                        confidence: 0,
                        state: TrackState::Lost,
                    },
                    TargetSample {
                        at: 100 * per_frame,
                        region: region(700_000).into(),
                        confidence: 1000,
                        state: TrackState::Tracked,
                    },
                ],
                corrections: vec![TargetCorrection {
                    at: 80 * per_frame,
                    region: region(450_001).into(),
                }],
                provenance: None,
                display_aspect: Some(ratio(16, 9)?),
            },
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetFraming {
            node: owner.clone(),
            framing: Some(Framing {
                clock: FramingClock::OwnerOutput,
                value: FramingValue::Follow {
                    target: target.clone(),
                    scale: ratio(3, 2)?,
                    fallback: FramingPose::new(ratio(1, 3)?, ratio(2, 5)?, ratio(5, 4)?)?,
                    scale_transition: None,
                },
            }),
        })
    })?;
    let before = project.document()?;
    let oracle = pcm(&project.package, &[256])?;
    fs::write(
        directory.join("follow-creep-before-pcm.json"),
        serde_json::to_vec(&json!({
            "samples":SAMPLES,"channels":2,"encoding":"IEEE float bits",
            "independent_DSP_claim":false,"canonical":oracle,
        }))?,
    )?;
    assert!(
        oracle.authored[48_000..48_800]
            .iter()
            .flatten()
            .any(|sample| f32::from_bits(*sample).abs() > 0.05),
        "the preserved Original must carry a real nonzero click"
    );
    let instruction = SemanticInstruction::Zoom {
        kind: ZoomKind::Creep {
            from: ranged.then(|| ratio(9, 5)).transpose()?,
            to: if ranged {
                ratio(12, 5)?
            } else {
                ExactRatio::integer(2)
            },
            curve: ZoomCurve::Linear,
        },
        center: SemanticZoomCenter::Keep,
    };
    let output = project.macro_operation(
        json!({
            "type":"apply","program":{"instructions":[instruction]},
            "parent":before.root(),"cursor":20,"selected_child":owner,
            "visual_selection":if ranged { json!({"type":"time","anchor":20,"head":80,"extending":false}) } else { Value::Null },
        }),
        false,
    )?;
    fs::write(
        directory.join("follow-creep-apply.stdout.json"),
        &output.stdout,
    )?;
    fs::write(
        directory.join("follow-creep-apply.stderr.txt"),
        &output.stderr,
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let created = project.document()?;
    assert_eq!(
        created.nodes()[&owner].framing,
        Some(Framing {
            clock: FramingClock::OwnerOutput,
            value: FramingValue::Follow {
                target,
                scale: ratio(3, 2)?,
                fallback: FramingPose::new(ratio(1, 3)?, ratio(2, 5)?, ratio(5, 4)?)?,
                scale_transition: Some(FollowScaleTransition {
                    factor: if ranged { ratio(8, 5)? } else { ratio(4, 3)? },
                    curve: ZoomCurve::Linear,
                    range: if ranged {
                        Some(FollowScaleRange {
                            start: ratio(1, 6)?,
                            end: ratio(2, 3)?,
                            from_factor: ratio(6, 5)?,
                        })
                    } else {
                        None
                    },
                }),
            },
        })
    );
    let before = if retuned {
        // Retune through the same public semantic writer used by :creep,
        // with no new Visual range. The original ramp window is retained.
        let instruction = SemanticInstruction::Zoom {
            kind: ZoomKind::Creep {
                from: None,
                to: ExactRatio::integer(3),
                curve: ZoomCurve::Smoothstep,
            },
            center: SemanticZoomCenter::Keep,
        };
        let output = project.macro_operation(
            json!({
                "type":"apply","program":{"instructions":[instruction]},
                "parent":created.root(),"cursor":40,"selected_child":owner,
                "visual_selection":Value::Null,
            }),
            false,
        )?;
        fs::write(
            directory.join("follow-retune-apply.stdout.json"),
            &output.stdout,
        )?;
        fs::write(
            directory.join("follow-retune-apply.stderr.txt"),
            &output.stderr,
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        created.clone()
    } else {
        before
    };
    let after = project.document()?;
    if retuned {
        let mut expected = created.nodes()[&owner]
            .framing
            .clone()
            .ok_or("Follow framing")?;
        let FramingValue::Follow {
            scale_transition: Some(transition),
            ..
        } = &mut expected.value
        else {
            return Err("Expected animated Follow".into());
        };
        transition.factor = ExactRatio::integer(2);
        transition.curve = ZoomCurve::Smoothstep;
        assert_eq!(after.nodes()[&owner].framing, Some(expected));
    }
    let poses = check_poses(&after, &owner, ranged, retuned)?;
    let actual = pcm(&project.package, &[37, 251, 173])?;
    fs::write(
        directory.join("follow-creep-after-pcm.json"),
        serde_json::to_vec(&actual)?,
    )?;
    same_pcm(&actual, &oracle, "framing");
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let undone = project.document()?;
    same_authored(&undone, &before)?;
    success(&[
        "project",
        "redo",
        project.path(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let restored = project.document()?;
    same_authored(&restored, &after)?;
    assert_eq!(check_poses(&restored, &owner, ranged, retuned)?, poses);
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    let history = store.validate_report(true)?;
    drop(store);
    fs::write(
        directory.join("follow-creep-authored-evidence.json"),
        serde_json::to_vec_pretty(&json!({
            "before":before,"created":created,"after":after,"undone":undone,"restored":restored,
            "history":history,"poses":poses,
            "independent_pose_oracle":true,"independent_DSP_claim":false,
            "tracking_accuracy_claim":false,"listening_claim":false,
            "range":if ranged { json!([20,80]) } else { Value::Null },
            "retuned_existing_transition":retuned,
        }))?,
    )?;
    let mut fixture = project.finish(
        vec!["Slow creep", "Framing"],
        (0..u64::try_from(FRAMES)?).map(|frame| (frame, original(frame))).collect(),
        vec![format!("Qualified 120-picture A/V CFR Original; manually authored deterministic target regions move, hold through Lost, receive a correction, and fall back outside their span. Keep-center creep is authored through the shared semantic CLI; {}. Every picture is checked against independent Q32 pose arithmetic; all 192192 stereo authored/limited samples and limiter gain bits equal the pre-creep baseline. The source picture is the existing counter fixture, not a generated moving subject; this does not qualify tracking accuracy, listening or independent DSP.", if ranged { "range [20,80) keeps live 1.5/fallback1.25 before it, jumps to live1.8/fallback1.5 at its start, ramps to live2.4/fallback2, then holds terminal scale while centers keep following" } else { "live 1.5->2 and fallback 1.25->5/3 use one owner clock" })],
    )?;
    if retuned {
        fixture.notes = vec!["Qualified 120-picture A/V CFR Original with deterministic target regions, including Lost, correction and fallback. Existing linear Follow creep retuned in one shared semantic transaction to live arrival 3x / fallback arrival 2.5x with smoothstep. Both bases, fallback center, target, clock and original whole/ranged ramp window are preserved; Undo restores the previous linear animation exactly except revision. Every picture is checked with independent Q32 arithmetic and all 192192 stereo authored/limited samples and limiter gain bits equal the baseline. This does not qualify tracking accuracy, listening or independent DSP.".into()];
    }
    let portable = directory.join("follow-creep-portable.deadpan");
    let copied = success(&[
        "project",
        "copy-portable",
        fixture.package.to_str().ok_or("UTF-8")?,
        portable.to_str().ok_or("UTF-8")?,
    ])?;
    assert_eq!(copied["portable_copy"]["revision_id"], fixture.revision);
    assert_eq!(copied["portable_copy"]["originals"][0]["from"], "managed");
    let store = ProjectStore::open(&portable, AccessMode::ReadOnly)?;
    same_authored(&store.snapshot()?, &restored)?;
    assert_eq!(
        check_poses(&store.snapshot()?, &owner, ranged, retuned)?,
        poses
    );
    store.validate_report(true)?;
    drop(store);
    same_pcm(&pcm(&portable, &[173, 37, 251])?, &oracle, "portable PCM");
    fs::write(
        directory.join("follow-creep-portable-copy.json"),
        serde_json::to_vec_pretty(&copied)?,
    )?;
    fixture.package = portable;
    Ok(fixture)
}
