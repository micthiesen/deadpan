//! A chosen-boundary return from an existing path through Split and the
//! copied inner owner's ordinary SetFraming(None), as native Camera Reset.

use super::*;
use deadpan_core::{
    Anchor, AnchorLossPolicy, BoundaryAnchor, FollowScaleTransition, FramingEnvelope,
    FramingSegment, InsertionBias, MarkId, SourceMoment, SourceStream, framing_zoom::ZoomCurve,
};
use follow_creep::{FRAMES, SAMPLES, pcm, same_authored, same_pcm};

pub const SEAM: i64 = 80;

fn path(follow: bool, target: &TargetId) -> Result<Framing> {
    let pose = |x, scale| -> Result<FramingPose> {
        Ok(FramingPose::new(
            ratio(x, 10)?,
            ratio(2, 5)?,
            ratio(scale, 10)?,
        )?)
    };
    Ok(Framing {
        clock: FramingClock::RetainedOutput {
            offset: ratio(15, 2)?,
            duration: ExactRatio::integer(180),
        },
        value: if follow {
            FramingValue::Follow {
                target: target.clone(),
                scale: ratio(3, 2)?,
                fallback: FramingPose::new(ratio(1, 3)?, ratio(2, 5)?, ratio(5, 4)?)?,
                scale_transition: Some(FollowScaleTransition {
                    factor: ratio(3, 2)?,
                    curve: ZoomCurve::Smoothstep,
                    range: None,
                }),
            }
        } else {
            FramingValue::Envelope {
                envelope: FramingEnvelope {
                    initial: pose(3, 12)?,
                    segments: vec![
                        FramingSegment {
                            end: ratio(1, 3)?,
                            pose: pose(6, 18)?,
                            curve: FramingCurve::Smoothstep,
                        },
                        FramingSegment {
                            end: ExactRatio::ONE,
                            pose: pose(4, 22)?,
                            curve: FramingCurve::Cubic {
                                control1: pose(7, 20)?,
                                control2: pose(2, 16)?,
                            },
                        },
                    ],
                },
            }
        },
    })
}

fn compare_pictures(
    before: &ProjectDocument,
    divided: &ProjectDocument,
    after: &ProjectDocument,
    right_owner: &NodeId,
) -> Result<Value> {
    let root = before.root().clone();
    let before = deadpan_plan::RenderPlan::compile(before)?;
    let divided = deadpan_plan::RenderPlan::compile(divided)?;
    let after = deadpan_plan::RenderPlan::compile(after)?;
    let mut evidence = Vec::new();
    for frame in 0..FRAMES {
        let old = before.picture(ProjectFrame(frame))?;
        let split = divided.picture(ProjectFrame(frame))?;
        let new = after.picture(ProjectFrame(frame))?;
        assert_eq!(old.picture, split.picture, "Split provider at {frame}");
        assert_eq!(old.picture, new.picture, "Return provider at {frame}");
        let posed = |picture: &deadpan_plan::PictureSample| {
            picture
                .framing
                .iter()
                .filter_map(|layer| layer.pose)
                .collect::<Vec<_>>()
        };
        assert_eq!(posed(&old), posed(&split), "Split path at {frame}");
        let expected: Vec<_> = split
            .framing
            .iter()
            .filter_map(|layer| {
                if frame >= SEAM && layer.instance.node == *right_owner {
                    None
                } else {
                    layer.pose
                }
            })
            .collect();
        assert_eq!(posed(&new), expected, "Return at {frame}");
        assert_eq!(posed(&new).len(), if frame < SEAM { 2 } else { 1 });
        let old_inner = old
            .framing
            .iter()
            .find(|layer| layer.pose.is_some() && layer.instance.node != root)
            .ok_or("old path")?;
        // The split preserves the original inner recipe's complete owner
        // coordinate, rather than restarting its easing on a cropped half.
        let split_inner = split
            .framing
            .iter()
            .find(|layer| layer.pose == old_inner.pose && layer.instance.node != root)
            .ok_or("split path")?;
        assert_eq!(old_inner.local_position, split_inner.local_position);
        assert_eq!(old_inner.effective_duration, split_inner.effective_duration);
        let canonical = |sample: &deadpan_plan::PictureSample| -> Result<Value> {
            let mut value = serde_json::to_value(sample)?;
            // Undo/Redo creates a fresh revision, while every observable
            // picture field and structural identity must remain identical.
            value
                .as_object_mut()
                .ok_or("picture object")?
                .remove("revision_id");
            Ok(value)
        };
        evidence.push(json!({"frame":frame,"before":canonical(&old)?,
            "split":canonical(&split)?,"after":canonical(&new)?}));
    }
    Ok(json!(evidence))
}

pub fn build(directory: &Path, follow: bool) -> Result<Fixture> {
    let mut project = Project::create(
        directory,
        if follow {
            "abrupt-return-follow"
        } else {
            "abrupt-return-envelope"
        },
    )?;
    let owner = root_child_at(&project.document()?, 0)?.0;
    let target = TargetId::new("return-target")?;
    if follow {
        project.apply(|project, document, _| {
            let (time_base, per_frame) = original_clock(document, &project.asset)?;
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
                    label: "Authored moving/lost/corrected region".into(),
                    asset: project.asset.clone(),
                    span: SourceSpan::new(stamp(10), stamp(75))?,
                    region: region(250_000).into(),
                    samples: vec![
                        TargetSample {
                            at: 10 * per_frame,
                            region: region(250_000).into(),
                            confidence: 1000,
                            state: TrackState::Tracked,
                        },
                        TargetSample {
                            at: 30 * per_frame,
                            region: region(650_000).into(),
                            confidence: 1000,
                            state: TrackState::Tracked,
                        },
                        TargetSample {
                            at: 40 * per_frame,
                            region: region(650_000).into(),
                            confidence: 0,
                            state: TrackState::Lost,
                        },
                    ],
                    corrections: vec![deadpan_core::TargetCorrection {
                        at: 60 * per_frame,
                        region: region(450_001).into(),
                    }],
                    provenance: None,
                    display_aspect: Some(ratio(16, 9)?),
                },
            })
        })?;
    }
    project.apply(|_, _, _| {
        Ok(Command::SetFraming {
            node: owner.clone(),
            framing: Some(path(follow, &target)?),
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
    project.apply(|project, document, _| {
        let (time_base, per_frame) = original_clock(document, &project.asset)?;
        Ok(Command::SetMark {
            id: MarkId::new("return-cue")?,
            owner: owner.clone(),
            label: "Original cue across split".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Source {
                    asset: project.asset.clone(),
                    moment: SourceMoment::Timestamp {
                        stream: SourceStream::Video,
                        timestamp: SourceTimestamp {
                            ticks: SEAM * per_frame,
                            time_base,
                        },
                    },
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::DeleteOwned,
        })
    })?;
    let before = project.document()?;
    let oracle = pcm(&project.package, &[256])?;
    let divided = project.split_root(SEAM)?;
    same_pcm(
        &pcm(&project.package, &[37, 251, 173])?,
        &oracle,
        "Split PCM",
    );
    let right = root_child_at(&divided, SEAM)?.0;
    let (right_owner, offset) = source_host(&divided, &right)?;
    assert_eq!(offset, SEAM);
    assert_eq!(
        divided.nodes()[&right_owner].framing,
        before.nodes()[&owner].framing
    );
    assert!(
        divided.nodes()[&right].framing.is_none(),
        "Partition is not the path owner"
    );
    assert_eq!(divided.marks().len(), 1);
    assert_eq!(
        divided.marks()[&MarkId::new("return-cue")?].binding_count(),
        2
    );
    let after = project.apply(|_, _, _| {
        Ok(Command::SetFraming {
            node: right_owner.clone(),
            framing: None,
        })
    })?;
    assert_eq!(after.marks(), divided.marks());
    assert_eq!(after.duration()?.frames(), FRAMES);
    same_pcm(
        &pcm(&project.package, &[173, 37, 251])?,
        &oracle,
        "Return PCM",
    );
    let pictures = compare_pictures(&before, &divided, &after, &right_owner)?;
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let reset_undone = project.document()?;
    same_authored(&reset_undone, &divided)?;
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        reset_undone.revision_id().as_str(),
    ])?;
    let split_undone = project.document()?;
    same_authored(&split_undone, &before)?;
    for _ in 0..2 {
        let current = project.document()?;
        success(&[
            "project",
            "redo",
            project.path(),
            "--expected",
            current.revision_id().as_str(),
        ])?;
    }
    let restored = project.document()?;
    same_authored(&restored, &after)?;
    let restored_pictures = compare_pictures(&before, &divided, &restored, &right_owner)?;
    for (frame, (actual, expected)) in restored_pictures
        .as_array()
        .ok_or("restored pictures")?
        .iter()
        .zip(pictures.as_array().ok_or("pictures")?)
        .enumerate()
    {
        assert_eq!(actual, expected, "Redo picture witness at {frame}");
    }
    same_pcm(
        &pcm(&project.package, &[251, 173, 37])?,
        &oracle,
        "Redo PCM",
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
    same_authored(&store.snapshot()?, &after)?;
    let portable_history = store.validate_report(true)?;
    drop(store);
    same_pcm(&pcm(&portable, &[37, 173, 251])?, &oracle, "Portable PCM");
    fs::write(
        directory.join("abrupt-return-evidence.json"),
        serde_json::to_vec(&json!({
            "before":before,"divided":divided,"after":after,"reset_undone":reset_undone,
            "split_undone":split_undone,"restored":restored,"pictures":pictures,
            "history":history,"portable_history":portable_history,"portable_copy":copied,
            "seam":SEAM,"samples":SAMPLES,"canonical_pcm":oracle,
            "full_authored_limited_limiter_bits_equal":true,"independent_DSP_claim":false,
            "tracking_accuracy_claim":false,"listening_claim":false,
        }))?,
    )?;
    project.finish(vec!["Abrupt return"],(0..u64::try_from(FRAMES)?).map(|frame|(frame,original(frame))).collect(),
        vec!["Existing nonzero-retained-clock Smoothstep/Cubic Envelope or live animated Follow, with parent framing retained once. Split at80 followed by Reset on the copied Source owner preserves every prefix picture/path coordinate, restores the entering canvas exactly at80, leaves all192192 stereo PCM/limiter bits unchanged, and preserves marks and complete Undo/Redo/portable history. Authored target regions are not tracking-accuracy evidence.".into()])
}
