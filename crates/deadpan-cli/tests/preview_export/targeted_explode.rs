//! Typed CLI Explode qualification over the real counter A/V Original.
//! Targets are authored geometry, not tracking-accuracy or listening evidence.
use super::*;
use deadpan_core::{
    Caption, CaptionPlacement, CapturedCanvas, CapturedFit, CapturedFraming, FollowCentering,
    InstancePath, OccurrenceEdit, OccurrenceIdentities, RepeatInstance, TargetCorrection,
};
use follow_creep::{pcm_samples, same_authored, same_pcm};

const PLAY_FRAMES: i64 = 30;
const DEFAULT_GAP: i64 = 6;
const OWNED_GAP: i64 = 9;
const THIRD_START: i64 = 2 * PLAY_FRAMES + DEFAULT_GAP + OWNED_GAP;

pub fn samples(document: &ProjectDocument) -> Result<i64> {
    let rate = document.presentation_basis().frame_rate;
    // Independently derive the endpoint from the common origin. Never sum
    // per-play rounded lengths at the NTSC project rate.
    Ok(i64::try_from(
        ExactRatio::new(
            i128::from(document.duration()?.frames()) * 48_000 * i128::from(rate.denominator()),
            i128::from(rate.numerator()),
        )?
        .round_even()?,
    )?)
}

fn posed(sample: &deadpan_plan::PictureSample) -> Vec<FramingPose> {
    sample
        .framing
        .iter()
        .filter_map(|layer| layer.pose)
        .collect()
}

fn same_pictures(before: &ProjectDocument, after: &ProjectDocument) -> Result<Value> {
    let old = deadpan_plan::RenderPlan::compile(before)?;
    let new = deadpan_plan::RenderPlan::compile(after)?;
    assert_eq!(old.duration(), new.duration());
    let mut evidence = Vec::new();
    for frame in 0..old.duration().frames() {
        let a = old.picture(ProjectFrame(frame))?;
        let b = new.picture(ProjectFrame(frame))?;
        assert_eq!(a.picture, b.picture, "provider/exact coordinate at {frame}");
        assert_eq!(
            a.picture_context, b.picture_context,
            "captured geometry at {frame}"
        );
        assert_eq!(posed(&a), posed(&b), "resolved poses at {frame}");
        assert_eq!(a.captions, b.captions, "caption reveal at {frame}");
        evidence.push(json!({
            "frame":frame, "picture":a.picture, "picture_context":a.picture_context,
            "poses":posed(&a), "captions":a.captions,
        }));
    }
    Ok(json!(evidence))
}

fn wrappers(document: &ProjectDocument) -> Result<Vec<NodeId>> {
    let NodeKind::Sequence { children } = &document.nodes()[&node("targeted-repeat")?].kind else {
        return Err("exploded Repeat must be an ordinary Sequence".into());
    };
    let found: Vec<_> = children
        .iter()
        .filter(|child| {
            matches!(
                &document.nodes()[*child].framing,
                Some(Framing {
                    value: FramingValue::Follow {
                        centering: FollowCentering::CanvasClamped,
                        ..
                    },
                    ..
                })
            )
        })
        .cloned()
        .collect();
    assert_eq!(
        found.len(),
        2,
        "only plays after the first have escalation wrappers"
    );
    Ok(found)
}

fn undo(project: &Project) -> Result<ProjectDocument> {
    let current = project.document()?;
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        current.revision_id().as_str(),
    ])?;
    project.document()
}

fn redo(project: &Project) -> Result<ProjectDocument> {
    let current = project.document()?;
    success(&[
        "project",
        "redo",
        project.path(),
        "--expected",
        current.revision_id().as_str(),
    ])?;
    project.document()
}

pub fn build(directory: &Path, multiply: bool) -> Result<Fixture> {
    let name = if multiply {
        "targeted-explode-multiply"
    } else {
        "targeted-explode-add"
    };
    let mut project = Project::create(directory, name)?;
    project.shorten()?;
    let child = root_child_at(&project.document()?, 0)?.0;
    let host = source_host(&project.document()?, &child)?.0;
    let target_id = TargetId::new("authored-explode-target")?;
    project.apply(|project, document, _| {
        let (time_base, per_frame) = original_clock(document, &project.asset)?;
        let stamp = |frame| SourceTimestamp {
            ticks: frame * per_frame,
            time_base,
        };
        let region = |x| TargetRegion {
            center: [x, 500_000],
            size: [100_000, 100_000],
        };
        let sample = |frame, x, state| TargetSample {
            at: frame * per_frame,
            region: region(x).into(),
            confidence: if state == TrackState::Lost { 0 } else { 1000 },
            state,
        };
        Ok(Command::SetTarget {
            id: target_id.clone(),
            target: AttentionTarget {
                label: "Authored moving, Lost and corrected target".into(),
                asset: project.asset.clone(),
                span: SourceSpan::new(stamp(0), stamp(120))?,
                region: region(0).into(),
                samples: vec![
                    sample(0, 0, TrackState::Tracked),
                    sample(12, 0, TrackState::Tracked),
                    sample(20, 900_000, TrackState::Tracked),
                    sample(25, 900_000, TrackState::Lost),
                    sample(50, 750_000, TrackState::Tracked),
                ],
                corrections: vec![TargetCorrection {
                    at: 32 * per_frame,
                    region: region(50_000).into(),
                }],
                provenance: None,
                display_aspect: Some(ratio(16, 9)?),
            },
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetFraming {
            node: host.clone(),
            framing: Some(Framing::static_pose(FramingPose::new(
                ratio(1, 2)?,
                ratio(1, 2)?,
                ratio(5, 4)?,
            )?)?),
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetCaptions {
            node: host.clone(),
            captions: vec![Caption {
                range: FrameRange::new(ProjectFrame(14), ProjectFrame(40))?,
                text: "Only from the second play".into(),
                placement: CaptionPlacement::Top,
                reveal: std::num::NonZeroU32::new(2),
            }],
        })
    })?;
    let repeated = project.apply(|project, document, _| {
        let (time_base, per_frame) = original_clock(document, &project.asset)?;
        Ok(Command::WrapRepeat {
            node: child.clone(),
            id: node("targeted-repeat")?,
            plays: 3,
            gap: Some(silent(
                DEFAULT_GAP,
                HoldVideo::Freeze {
                    asset: project.asset.clone(),
                    timestamp: SourceTimestamp {
                        ticks: 29 * per_frame,
                        time_base,
                    },
                },
            )?),
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let NodeKind::Repeat {
        child, iterations, ..
    } = &repeated.nodes()[&node("targeted-repeat")?].kind
    else {
        return Err("wrapped Repeat".into());
    };
    let (child, second) = (child.clone(), iterations.at(1).ok_or("second play")?);
    project.apply(|project, document, _| {
        Ok(Command::EditOccurrence {
            instance: InstancePath {
                node: child,
                repeats: vec![RepeatInstance {
                    node: node("targeted-repeat")?,
                    iteration: second.clone(),
                }],
            },
            edit: OccurrenceEdit::SetAudioTreatments {
                treatments: AudioTreatments::from_clip_gain(ClipGain::new(
                    GainDb::new(-3000)?,
                    false,
                    Vec::new(),
                    Vec::new(),
                )?),
            },
            identities: OccurrenceIdentities {
                nodes: project.fresh(document.nodes().len() + 4)?,
                marks: Vec::new(),
            },
        })
    })?;
    project.apply(|_, _, revision| {
        Ok(Command::IsolateGap {
            node: node("targeted-repeat")?,
            iteration: second,
            id: node("owned-targeted-gap")?,
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetHoldDuration {
            node: node("owned-targeted-gap")?,
            duration: FrameDuration::new(OWNED_GAP)?,
        })
    })?;
    project.apply(|_, document, _| {
        let basis = document.presentation_basis();
        Ok(Command::SetHoldPictureContext {
            node: node("owned-targeted-gap")?,
            context: Some(CapturedFraming::new(vec![CapturedCanvas {
                width: basis.width,
                height: basis.height,
                fit: CapturedFit::Fit,
                layers: vec![Some(FramingPose::new(
                    ratio(1, 2)?,
                    ratio(1, 2)?,
                    ratio(1, 2)?,
                )?)],
            }])?),
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetRepeatEscalation {
            node: node("targeted-repeat")?,
            escalation: Some(RepeatEscalation {
                gain_step: GainDb::new(1000)?,
                zoom: Some(ZoomStep {
                    step: if multiply { ratio(1, 2)? } else { ratio(1, 1)? },
                    progression: if multiply {
                        ZoomProgression::Multiply
                    } else {
                        ZoomProgression::Add
                    },
                    target: Some(target_id.clone()),
                }),
            }),
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetFraming {
            node: node("targeted-repeat")?,
            framing: Some(Framing {
                clock: FramingClock::OwnerOutput,
                value: FramingValue::Follow {
                    centering: FollowCentering::Unconstrained,
                    target: target_id.clone(),
                    scale: ratio(3, 4)?,
                    fallback: FramingPose::new(ratio(3, 8)?, ratio(1, 2)?, ratio(3, 4)?)?,
                    scale_transition: None,
                },
            }),
        })
    })?;
    project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: document.root().clone(),
            framing: Some(Framing::static_pose(FramingPose::new(
                ratio(11, 20)?,
                ratio(1, 2)?,
                ratio(11, 10)?,
            )?)?),
        })
    })?;
    let before = project.document()?;
    assert_eq!(
        before.duration()?.frames(),
        3 * PLAY_FRAMES + DEFAULT_GAP + OWNED_GAP
    );
    let count = samples(&before)?;
    let oracle = pcm_samples(&project.package, &[256], count)?;
    let exploded = explode(&mut project, "targeted-repeat")?;
    let pictures = same_pictures(&before, &exploded)?;
    same_pcm(
        &pcm_samples(&project.package, &[37, 251, 173], count)?,
        &oracle,
        "Explode",
    );
    let undone = undo(&project)?;
    same_authored(&undone, &before)?;
    same_pictures(&before, &undone)?;
    same_pcm(
        &pcm_samples(&project.package, &[251, 37, 173], count)?,
        &oracle,
        "Undo",
    );
    let redone = redo(&project)?;
    same_authored(&redone, &exploded)?;
    same_pictures(&before, &redone)?;
    same_pcm(
        &pcm_samples(&project.package, &[173, 251, 37], count)?,
        &oracle,
        "Redo",
    );

    // A meaningful ordinary edit to the last materialized play changes only
    // that play. Its Follow recipe retains the live target and clamp policy.
    let last = wrappers(&redone)?.last().ok_or("third wrapper")?.clone();
    let mut adjusted = redone.nodes()[&last]
        .framing
        .clone()
        .ok_or("third Follow")?;
    let FramingValue::Follow {
        scale, fallback, ..
    } = &mut adjusted.value
    else {
        return Err("third Follow".into());
    };
    *scale = if multiply { ratio(3, 8)? } else { ratio(5, 2)? };
    fallback.scale = *scale;
    let edited = project.apply(|_, _, _| {
        Ok(Command::SetFraming {
            node: last.clone(),
            framing: Some(adjusted),
        })
    })?;
    let old = deadpan_plan::RenderPlan::compile(&redone)?;
    let new = deadpan_plan::RenderPlan::compile(&edited)?;
    let mut changed = false;
    for frame in 0..before.duration()?.frames() {
        let a = old.picture(ProjectFrame(frame))?;
        let b = new.picture(ProjectFrame(frame))?;
        assert_eq!(a.picture, b.picture);
        assert_eq!(a.captions, b.captions);
        if frame < THIRD_START {
            assert_eq!(posed(&a), posed(&b));
        } else {
            changed |= posed(&a) != posed(&b);
        }
    }
    assert!(changed, "independent later-play edit changes that play");
    let mut target = edited.targets()[&target_id].clone();
    let (_, per_frame) = original_clock(&edited, &project.asset)?;
    target.corrections.push(TargetCorrection {
        at: 36 * per_frame,
        region: TargetRegion {
            center: [800_000, 500_000],
            size: [100_000, 100_000],
        }
        .into(),
    });
    let retargeted = project.apply(|_, _, _| {
        Ok(Command::SetTarget {
            id: target_id.clone(),
            target,
        })
    })?;
    let live = deadpan_plan::RenderPlan::compile(&retargeted)?;
    assert!(
        (THIRD_START..before.duration()?.frames()).any(|frame| {
            let prior = new.picture(ProjectFrame(frame)).unwrap();
            let next = live.picture(ProjectFrame(frame)).unwrap();
            let own = |sample: &deadpan_plan::PictureSample| {
                sample
                    .framing
                    .iter()
                    .find(|layer| layer.instance.node == last)
                    .and_then(|layer| layer.pose)
            };
            own(&prior) != own(&next)
        }),
        "the independently edited third Follow still evaluates later target corrections"
    );
    same_pcm(
        &pcm_samples(&project.package, &[173, 37, 251], count)?,
        &oracle,
        "Picture edits",
    );
    let target_undone = undo(&project)?;
    same_authored(&target_undone, &edited)?;
    let restored = undo(&project)?;
    same_authored(&restored, &exploded)?;
    assert_eq!(same_pictures(&before, &restored)?, pictures);
    same_pcm(
        &pcm_samples(&project.package, &[37, 173, 251], count)?,
        &oracle,
        "Restored Explode",
    );

    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    let history = store.validate_report(true)?;
    drop(store);
    let portable = directory.join("portable.deadpan");
    let copied = success(&[
        "project",
        "copy-portable",
        project.path(),
        portable.to_str().ok_or("UTF-8")?,
    ])?;
    let store = ProjectStore::open(&portable, AccessMode::ReadOnly)?;
    let portable_document = store.snapshot()?;
    same_authored(&portable_document, &restored)?;
    let portable_history = store.validate_report(true)?;
    drop(store);
    same_pictures(&before, &portable_document)?;
    same_pcm(
        &pcm_samples(&portable, &[251, 173, 37], count)?,
        &oracle,
        "Portable",
    );
    fs::write(
        directory.join("targeted-explode-evidence.json"),
        serde_json::to_vec(&json!({
            "before":before,"exploded":exploded,"undone":undone,"redone":redone,
            "independent_play_edit":edited,"retargeted":retargeted,
            "target_undone":target_undone,"restored":restored,"pictures":pictures,
            "samples":count,"canonical_pcm":oracle,"history":history,
            "portable_history":portable_history,"portable_copy":copied,
            "authored_limited_and_limiter_bits_equal":true,
            "independent_DSP_claim":false,"tracking_accuracy_claim":false,"listening_claim":false,
        }))?,
    )?;
    // Expected decoded ordinals come from this authored schedule, not the plan.
    let mut expected = Vec::new();
    for play in 0..3 {
        expected.extend((12..42).map(original));
        if play < 2 {
            expected.extend(std::iter::repeat_n(
                original(29),
                usize::try_from(if play == 0 { DEFAULT_GAP } else { OWNED_GAP })?,
            ));
        }
    }
    let expectations = expected
        .into_iter()
        .zip(0u64..)
        .map(|(picture, frame)| (frame, picture))
        .collect();
    let notes = vec![format!(
        "Three 30-frame plays of Original 12..42; a 6-frame default Freeze and a 9-frame owned captured Freeze; child crop, parent Follow/static framing, second-play audio override and +1 dB per-play gain. {} Every before/after source coordinate, resolved pose and caption and all authored/limited PCM and limiter-gain bits agree through Explode, Undo/Redo and portable history. A temporary third-play edit and later target correction prove independence and live target evaluation, then both are undone. Targets are authored geometry, not tracker output.",
        if multiply {
            "Multiply scales are 1/2 and 1/4."
        } else {
            "Add scales are 2 and 3."
        },
    )];
    project.finish(vec!["Explode targeted repeat"], expectations, notes)
}

/// Deliberate encoded-file negative: replace only play1's live target center
/// with its centered fallback while retaining its exact scale and all audio.
pub fn center_second(fixture: &Fixture) -> Result<String> {
    let document = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.snapshot()?;
    let second = wrappers(&document)?.remove(0);
    let Some(Framing {
        value: FramingValue::Follow { fallback, .. },
        ..
    }) = &document.nodes()[&second].framing
    else {
        return Err("second Follow".into());
    };
    let centered = Framing::static_pose(*fallback)?;
    write_control(fixture, second, Some(centered), 900)
}

/// Remove the second play's framing only. Its group, gain, contents and gap
/// remain, so the observable negative cannot change audio or timing.
pub fn omit_second(fixture: &Fixture) -> Result<String> {
    let document = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.snapshot()?;
    let NodeKind::Sequence { children } = &document.nodes()[&node("targeted-repeat")?].kind else {
        return Err("exploded Repeat".into());
    };
    assert_eq!(
        children.len(),
        4,
        "first play, its gap, and two escalated groups"
    );
    let second = children[2].clone();
    write_control(fixture, second, None, 901)
}

fn write_control(
    fixture: &Fixture,
    second: NodeId,
    framing: Option<Framing>,
    step: u32,
) -> Result<String> {
    let document = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.snapshot()?;
    let asset = document
        .assets()
        .iter()
        .find(|(_, record)| record.video.is_some())
        .map(|(id, _)| id.clone())
        .ok_or("Original")?;
    let mut project = Project {
        name: fixture.name,
        directory: fixture.package.parent().ok_or("parent")?.to_owned(),
        package: fixture.package.clone(),
        step,
        ids: 9000,
        asset,
    };
    Ok(project
        .apply(|_, _, _| {
            Ok(Command::SetFraming {
                node: second,
                framing,
            })
        })?
        .revision_id()
        .to_string())
}
