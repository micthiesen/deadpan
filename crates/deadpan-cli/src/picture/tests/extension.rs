//! Real retained Original media through the public extension capture path.

use deadpan_core::{
    AudioSample, AudioTimingId, Cutaway, CutawayFit, ExtensionDirection, IterationId, LinkRelation,
    RepeatEditBranch, RepeatEditStep, ScopedNodeTarget, SourceAudioMapping, SourceSpan,
    SourceVideo, SourceVideoMapping,
};
use deadpan_jobs::GenerationOptions;
use deadpan_models::{BoundaryClock, BoundaryPicture, ExtensionContext, ExtensionOppositeSeam};
use deadpan_plan::ScopedHoldContextRequest;

use super::*;
use crate::generation::conditioning::{ExtensionInputs, prepare_extension_scoped_with_options};

fn target(repeats: Vec<RepeatEditStep>) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: node("extension"),
        repeats,
    }
}

fn edge_hold(fixture: &mut Fixture, direction: ExtensionDirection) -> Result {
    let document = fixture.store.snapshot()?;
    let index = fixture
        .store
        .source_video_index(document.revision_id(), &asset())?;
    let ordinal = match direction {
        ExtensionDirection::FromLeft => index.frames().len() - 1,
        ExtensionDirection::FromRight => 0,
    };
    let recipe = HoldRecipe {
        duration: frames(3),
        picture_context: None,
        video: HoldVideo::Freeze {
            asset: asset(),
            timestamp: SourceTimestamp {
                ticks: index.frames()[ordinal].pts,
                time_base: index.time_base(),
            },
        },
        audio: HoldAudio::Silence,
    };
    fixture.edit(
        "edge-hold",
        Command::Insert {
            parent: node("root"),
            index: usize::from(direction == ExtensionDirection::FromLeft),
            subtree: Subtree {
                root: node("extension"),
                nodes: BTreeMap::from([(node("extension"), BeatNode::hold("Extension", recipe))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
}

fn capture(
    fixture: &Fixture,
    target: &ScopedNodeTarget,
    direction: ExtensionDirection,
) -> Result<ExtensionInputs> {
    Ok(prepare_extension_scoped_with_options(
        &fixture.path,
        fixture.store.snapshot()?.revision_id(),
        target,
        direction,
        &GenerationOptions::default(),
        &active(),
    )?)
}

fn manifest(inputs: &ExtensionInputs) -> Result<ExtensionContext> {
    Ok(serde_json::from_slice(&inputs.manifest)?)
}

fn assert_unchanged(fixture: &Fixture, before: &ProjectDocument, history: (bool, bool)) -> Result {
    assert_eq!(fixture.store.snapshot()?, *before);
    assert_eq!(fixture.store.head_revision()?, *before.revision_id());
    assert_eq!(fixture.store.history_availability()?, history);
    assert!(fixture.store.generation_preparations(None, 64)?.is_empty());
    Ok(())
}

#[test]
fn extension_capture_at_both_definition_edges_retains_fractional_clocks_measured_ids_and_pngs()
-> Result {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let mut fixture = Fixture::source("cfr-bframes.mp4")?;
        edge_hold(&mut fixture, direction)?;
        let before = fixture.store.snapshot()?;
        let history = fixture.store.history_availability()?;
        let index = fixture
            .store
            .source_video_index(before.revision_id(), &asset())?;
        assert_eq!(index.frames().len(), 120);
        let inputs = capture(&fixture, &target(Vec::new()), direction)?;
        assert_unchanged(&fixture, &before, history)?;
        assert_eq!(inputs.plan.project_frames(), frames(3));
        assert_eq!(
            inputs.plan.project_frame_rate(),
            FrameRate::new(30_000, 1001)?
        );
        assert_eq!(
            (
                inputs.plan.context_frame_count(),
                inputs.plan.generated_frame_count(),
                inputs.plan.native_frame_count()
            ),
            (9, 8, 17)
        );
        let context = manifest(&inputs)?;
        assert_eq!(context.context().len(), 9);
        assert!(matches!(context.opposite(), ExtensionOppositeSeam::Absent));
        assert!(inputs.opposite_png.is_none());
        assert_eq!(
            inputs.continuity.capture_policy,
            "deadpan-extension-context-1"
        );
        assert_eq!(inputs.continuity.inputs.len(), 9);
        assert!(inputs.continuity.opposite.is_none());
        assert!(!inputs.continuity.support.is_empty());
        assert!(inputs.continuity.decoded_pictures > 0);
        let anchor = match direction {
            ExtensionDirection::FromLeft => ExactRatio::new(239, 2)?,
            ExtensionDirection::FromRight => ExactRatio::new(7, 2)?,
        };
        // Independent exact oracle: one 24 Hz interval is 1250/1001 project
        // frames. The leading Hold translates the Original by exactly 3.
        let step = ExactRatio::new(1250, 1001)?;
        let mut observed = Vec::new();
        for (i, (entry, png)) in context
            .context()
            .iter()
            .zip(&inputs.context_pngs)
            .enumerate()
        {
            let offset = step.checked_mul(ExactRatio::integer(i64::try_from(i)?))?;
            let position = match direction {
                ExtensionDirection::FromLeft => anchor
                    .checked_sub(step.checked_mul(ExactRatio::integer(8))?)?
                    .checked_add(offset)?,
                ExtensionDirection::FromRight => anchor.checked_add(offset)?,
            };
            assert_eq!(
                entry.picture.clock(),
                &BoundaryClock::Definition {
                    project_id: before.project_id().clone(),
                    revision_id: before.revision_id().clone(),
                    definition: node("root"),
                    position,
                }
            );
            let original_position = match direction {
                ExtensionDirection::FromLeft => position,
                ExtensionDirection::FromRight => position.checked_sub(ExactRatio::integer(3))?,
            };
            let ordinal = usize::try_from(original_position.floor())?;
            let BoundaryPicture::Original {
                asset: recorded_asset,
                qualification,
                picture,
                ..
            } = &entry.picture
            else {
                panic!("retained Original input")
            };
            assert_eq!(recorded_asset, &asset());
            assert_eq!(
                Some(qualification),
                before.assets()[&asset()].source_qualification.as_ref()
            );
            assert_eq!(picture.source_frame, SourceFrameId(u64::try_from(ordinal)?));
            assert_eq!(
                picture.pts,
                SourceTimestamp {
                    ticks: index.frames()[ordinal].pts,
                    time_base: index.time_base()
                }
            );
            assert_eq!(
                entry.frame.reference().as_str(),
                format!("inputs/context-{i:03}.png")
            );
            assert_eq!(
                entry.frame.sha256(),
                &crate::generation::conditioning::sha256(png)?
            );
            let decoded = image::load_from_memory(png)?.to_rgb8();
            assert_eq!(decoded.dimensions(), (768, 320));
            assert_eq!(decoded.get_pixel(0, 160).0, [0, 0, 0]);
            assert_eq!(entry.content, Some(context.presentation()));
            observed.push(picture.source_frame.0);
        }
        assert!(observed.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            observed.windows(2).any(|pair| pair[1] - pair[0] == 2),
            "native spacing must skip some 29.97 Hz pictures"
        );
    }
    Ok(())
}

#[test]
fn extension_outer_repeat_and_retime_do_not_change_definition_context_or_borrow_other_plays()
-> Result {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let mut fixture = Fixture::source("cfr-bframes.mp4")?;
        edge_hold(&mut fixture, direction)?;
        let baseline = capture(&fixture, &target(Vec::new()), direction)?;
        fixture.edit(
            "grouped",
            Command::Group {
                parent: node("root"),
                start: 0,
                end: 2,
                id: node("local"),
                label: "Local definition".into(),
            },
        )?;
        fixture.edit(
            "retimed",
            Command::WrapRetime {
                node: node("local"),
                id: node("speed"),
                duration: frames(246),
                pitch: PitchPolicy::Preserve,
            },
        )?;
        fixture.edit(
            "repeated",
            Command::WrapRepeat {
                node: node("speed"),
                id: node("repeat"),
                plays: 3,
                gap: None,
                anchor_policy: Default::default(),
            },
        )?;
        let before = fixture.store.snapshot()?;
        let history = fixture.store.history_availability()?;
        for branch in [
            RepeatEditBranch::Default,
            RepeatEditBranch::Play {
                iteration: IterationId {
                    allocation: revision("repeated"),
                    ordinal: 1,
                },
            },
        ] {
            let scoped = target(vec![RepeatEditStep {
                repeat: node("repeat"),
                branch,
            }]);
            let result = capture(&fixture, &scoped, direction)?;
            assert_eq!(result.plan, baseline.plan);
            assert_eq!(result.context_pngs, baseline.context_pngs);
            assert!(
                result.opposite_png.is_none(),
                "outer adjacent plays cannot supply an opposite seam"
            );
            assert_eq!(result.continuity.inputs, baseline.continuity.inputs);
            assert_eq!(result.continuity.support, baseline.continuity.support);
            let context = manifest(&result)?;
            for picture in context.context() {
                assert!(
                    matches!(picture.picture.clock(), BoundaryClock::Definition { definition, revision_id, .. }
                    if definition == &node("local") && revision_id == &revision("repeated"))
                );
            }
        }
        assert!(
            capture(&fixture, &target(Vec::new()), direction).is_err(),
            "a Repeat Play cannot be picked by omission"
        );
        assert_unchanged(&fixture, &before, history)?;
    }
    Ok(())
}

#[test]
fn extension_interior_capture_retains_the_existing_opposite_as_unconditioned() -> Result {
    let mut fixture = Fixture::source("cfr-bframes.mp4")?;
    let before = fixture.store.snapshot()?;
    let at = ProjectFrame(40);
    let split = before
        .insert_time_target(at)?
        .split
        .ok_or("interior Source split")?;
    fixture.edit(
        "interior-hold",
        Command::InsertTime {
            at,
            hold: HoldRecipe {
                duration: frames(3),
                picture_context: None,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: node("extension"),
            identities: SplitIdentities {
                nodes: (0..split.required_ids)
                    .map(|i| node(&format!("interior-{i}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("interior-hold"),
                ordinal: 0,
            },
        },
    )?;
    let before = fixture.store.snapshot()?;
    let history = fixture.store.history_availability()?;
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let inputs = capture(&fixture, &target(Vec::new()), direction)?;
        let context = manifest(&inputs)?;
        let ExtensionOppositeSeam::PresentUnconditioned { picture, frame, .. } = context.opposite()
        else {
            panic!("interior opposite retained")
        };
        let BoundaryPicture::Original {
            picture: decoded,
            clock,
            ..
        } = picture.as_ref()
        else {
            panic!("Original opposite")
        };
        let (ordinal, position) = match direction {
            ExtensionDirection::FromLeft => (40, ExactRatio::new(87, 2)?),
            ExtensionDirection::FromRight => (39, ExactRatio::new(79, 2)?),
        };
        assert_eq!(decoded.source_frame, SourceFrameId(ordinal));
        assert!(
            matches!(clock, BoundaryClock::Definition { position: actual, .. } if *actual == position)
        );
        let png = inputs
            .opposite_png
            .as_ref()
            .ok_or("retained opposite PNG")?;
        assert_eq!(frame.reference().as_str(), "inputs/opposite.png");
        assert_eq!(
            frame.sha256(),
            &crate::generation::conditioning::sha256(png)?
        );
        assert_eq!(
            image::load_from_memory(png)?.to_rgb8().dimensions(),
            (768, 320)
        );
        assert_eq!(
            inputs.context_pngs.len(),
            9,
            "opposite is not an extra conditioning frame"
        );
        assert_eq!(
            inputs
                .continuity
                .opposite
                .as_ref()
                .ok_or("opposite identity")?
                .relative_position,
            ExactRatio::integer(if direction == ExtensionDirection::FromLeft {
                4
            } else {
                -4
            })
        );
    }
    assert_unchanged(&fixture, &before, history)?;
    Ok(())
}

fn sparse_source_ids(fixture: &Fixture) -> Result<Vec<SourceFrameId>> {
    let session = fixture.open(None)?;
    let context = session.plan().scoped_hold_context(
        &ScopedHoldContextRequest {
            target: target(Vec::new()),
            direction: ExtensionDirection::FromLeft,
            native_rate: FrameRate::new(24, 1)?,
            frame_count: 9,
        },
        BoundaryQueryLimits::default(),
    )?;
    let index = fixture
        .store
        .source_video_index(session.revision(), &asset())?;
    let mut ids = Vec::new();
    for picture in context.pictures {
        assert!(
            picture.position.compare_integer(111).is_lt()
                || !picture.position.compare_integer(112).is_lt(),
            "the regression interruption must fall strictly between retained context samples"
        );
        ids.push(picture.picture.select_source_frame(&index)?.identity);
    }
    Ok(ids)
}

#[test]
fn extension_rejects_one_frame_cutaway_hidden_between_context_samples_without_mutation() -> Result {
    let mut fixture = Fixture::source("cfr-bframes.mp4")?;
    edge_hold(&mut fixture, ExtensionDirection::FromLeft)?;
    let baseline = sparse_source_ids(&fixture)?;
    let index = fixture
        .store
        .source_video_index(fixture.store.snapshot()?.revision_id(), &asset())?;
    fixture.edit(
        "tiny-cutaway",
        Command::SetCutaways {
            node: node("source"),
            cutaways: vec![Cutaway {
                range: range(111, 112),
                asset: asset(),
                fit: CutawayFit::Hold,
                removed: false,
                selection: SourceSpan::new(
                    SourceTimestamp {
                        ticks: index.frames()[20].pts,
                        time_base: index.time_base(),
                    },
                    SourceTimestamp {
                        ticks: index.frames()[21].pts,
                        time_base: index.time_base(),
                    },
                )?
                .into(),
            }],
        },
    )?;
    assert_eq!(
        sparse_source_ids(&fixture)?,
        baseline,
        "all nine conditioned pictures miss the cutaway"
    );
    let before = fixture.store.snapshot()?;
    let history = fixture.store.history_availability()?;
    let error = capture(&fixture, &target(Vec::new()), ExtensionDirection::FromLeft)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("discontinuous") || error.contains("picture seam"),
        "{error}"
    );
    assert_unchanged(&fixture, &before, history)?;
    Ok(())
}

#[test]
fn extension_rejects_same_original_source_jump_hidden_between_samples() -> Result {
    let mut fixture = Fixture::source("cfr-bframes.mp4")?;
    edge_hold(&mut fixture, ExtensionDirection::FromLeft)?;
    let baseline = sparse_source_ids(&fixture)?;
    let snapshot = fixture.store.snapshot()?;
    let original = snapshot.nodes()[&node("source")].clone();
    let index = fixture
        .store
        .source_video_index(snapshot.revision_id(), &asset())?;
    let mut nodes = BTreeMap::new();
    for (name, start, end, duration) in [
        ("prefix", 0, 111, 111),
        ("jump", 20, 21, 1),
        ("suffix", 112, 120, 8),
    ] {
        let mut part = original.clone();
        let NodeKind::Source { source } = &mut part.kind else {
            panic!("registered Source")
        };
        source.duration = frames(duration);
        source.edit_window = None;
        source.video_mapping = SourceVideoMapping::FitBeat;
        source.video = SourceVideo::Stream {
            asset: asset(),
            span: SourceSpan::new(
                SourceTimestamp {
                    ticks: index.frames()[start].pts,
                    time_base: index.time_base(),
                },
                SourceTimestamp {
                    ticks: if end == index.frames().len() {
                        index.terminal_end()
                    } else {
                        index.frames()[end].pts
                    },
                    time_base: index.time_base(),
                },
            )?,
        };
        source.audio = None;
        source.link = LinkRelation::Independent;
        source.audio_mapping = SourceAudioMapping::FitBeat;
        source.audio_offset = AudioSample(0);
        nodes.insert(node(name), part);
    }
    nodes.insert(
        node("slices"),
        BeatNode::sequence(
            "Discontinuous Original",
            vec![node("prefix"), node("jump"), node("suffix")],
        ),
    );
    fixture.edit(
        "remove-whole",
        Command::Delete {
            node: node("source"),
        },
    )?;
    fixture.edit(
        "source-jump",
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("slices"),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?;
    assert_eq!(sparse_source_ids(&fixture)?, baseline);
    let before = fixture.store.snapshot()?;
    let history = fixture.store.history_availability()?;
    let error = capture(&fixture, &target(Vec::new()), ExtensionDirection::FromLeft)
        .unwrap_err()
        .to_string();
    assert!(error.contains("discontinuous"), "{error}");
    assert_unchanged(&fixture, &before, history)?;
    Ok(())
}

#[test]
fn extension_short_context_missing_anchor_and_cancellation_leave_project_unchanged() -> Result {
    let mut fixture = Fixture::source("cfr-bframes.mp4")?;
    edge_hold(&mut fixture, ExtensionDirection::FromRight)?;
    let before = fixture.store.snapshot()?;
    let history = fixture.store.history_availability()?;
    assert!(
        capture(&fixture, &target(Vec::new()), ExtensionDirection::FromLeft).is_err(),
        "a saved freeze cannot invent a missing left anchor"
    );
    let error = prepare_extension_scoped_with_options(
        &fixture.path,
        before.revision_id(),
        &target(Vec::new()),
        ExtensionDirection::FromRight,
        &GenerationOptions::default(),
        &AtomicBool::new(true),
    )
    .unwrap_err();
    assert!(error.contains("cancelled"), "{error}");
    assert_unchanged(&fixture, &before, history)?;

    // Put a 5-frame Source fragment and the Hold in a separate definition. The
    // rest of the Original remains outside it and must not provide context.
    let mut fixture = Fixture::source("cfr-bframes.mp4")?;
    fixture.edit(
        "short-split",
        Command::Split {
            node: node("source"),
            at: frames(5),
            identities: SplitIdentities {
                nodes: (0..8).map(|i| node(&format!("short-{i}"))).collect(),
            },
        },
    )?;
    let split = fixture.store.snapshot()?;
    let NodeKind::Sequence { children } = &split.nodes()[split.root()].kind else {
        panic!("root Sequence")
    };
    let left = children[0].clone();
    fixture.edit(
        "short-group",
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 1,
            id: node("short-definition"),
            label: "Short definition".into(),
        },
    )?;
    fixture.edit(
        "short-hold",
        Command::Insert {
            parent: node("short-definition"),
            index: 1,
            subtree: Subtree {
                root: node("extension"),
                nodes: BTreeMap::from([(
                    node("extension"),
                    BeatNode::hold(
                        "Extension",
                        HoldRecipe {
                            duration: frames(3),
                            picture_context: None,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )?;
    fixture.edit(
        "short-wrapped",
        Command::WrapRepeat {
            node: node("short-definition"),
            id: node("repeat"),
            plays: 3,
            gap: None,
            anchor_policy: Default::default(),
        },
    )?;
    let before = fixture.store.snapshot()?;
    let history = fixture.store.history_availability()?;
    assert!(before.nodes().contains_key(&left));
    let scoped = target(vec![RepeatEditStep {
        repeat: node("repeat"),
        branch: RepeatEditBranch::Play {
            iteration: IterationId {
                allocation: revision("short-wrapped"),
                ordinal: 1,
            },
        },
    }]);
    assert!(
        capture(&fixture, &scoped, ExtensionDirection::FromLeft).is_err(),
        "neither an outer play nor source beyond the definition can pad short context"
    );
    assert_unchanged(&fixture, &before, history)?;
    Ok(())
}

#[test]
fn extension_rejects_a_real_gradual_fade_after_one_frame_partitions_preserve_every_picture()
-> Result {
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extension-fade.mp4");
    let mut fixture = Fixture::source_from(&input, "extension-fade.mp4")?;
    edge_hold(&mut fixture, ExtensionDirection::FromLeft)?;
    let index = fixture
        .store
        .source_video_index(fixture.store.snapshot()?.revision_id(), &asset())?;
    assert_eq!(index.time_base(), SourceTimeBase::new(1, 30_000)?);
    assert_eq!(index.frames().len(), 120);
    for (ordinal, frame) in index.frames().iter().enumerate() {
        assert_eq!(
            frame.pts,
            i64::try_from(ordinal)? * 1001,
            "fixture must retain exact project-aligned source PTS"
        );
    }
    let mut baseline = fixture.open(None)?;
    assert_eq!(baseline.plan().duration(), frames(123));
    let mut expected = Vec::new();
    for at in 0..123 {
        let picture = baseline.prepare(ProjectFrame(at), &active())?;
        let (id, frame) = decoded(&picture);
        expected.push((*id, frame.metadata().pts, frame.bytes().to_vec()));
    }
    drop(baseline);
    let before = fixture.store.snapshot()?;
    let history = fixture.store.history_availability()?;
    let error = capture(&fixture, &target(Vec::new()), ExtensionDirection::FromLeft)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("detected picture transition"),
        "uncut fade: {error}"
    );
    assert_unchanged(&fixture, &before, history)?;

    // The complete context occupies the last eleven Original frames. Split it
    // into one-frame neutral Partitions while retaining the original full
    // Source clocks. No crop, source remap, gain or time scaling is authored.
    for boundary in 109..120 {
        let document = fixture.store.snapshot()?;
        let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
            panic!("root Sequence")
        };
        let tail = children[children.len() - 2].clone();
        fixture.edit(
            &format!("fade-split-{boundary}"),
            Command::Split {
                node: tail,
                at: frames(if boundary == 109 { 109 } else { 1 }),
                identities: SplitIdentities {
                    nodes: (0..8)
                        .map(|i| node(&format!("fade-{boundary}-{i}")))
                        .collect(),
                },
            },
        )?;
    }
    let mut partitioned = fixture.open(None)?;
    assert_eq!(partitioned.plan().duration(), frames(123));
    for (at, (expected_id, expected_pts, expected_bytes)) in expected.iter().enumerate() {
        let picture = partitioned.prepare(ProjectFrame(i64::try_from(at)?), &active())?;
        let (id, frame) = decoded(&picture);
        assert_eq!(id, expected_id, "identity at {at}");
        assert_eq!(&frame.metadata().pts, expected_pts, "PTS at {at}");
        assert_eq!(frame.bytes(), expected_bytes, "pixels at {at}");
    }
    let context = partitioned.plan().scoped_hold_context(
        &ScopedHoldContextRequest {
            target: target(Vec::new()),
            direction: ExtensionDirection::FromLeft,
            native_rate: FrameRate::new(24, 1)?,
            frame_count: 9,
        },
        BoundaryQueryLimits::default(),
    )?;
    assert!(
        context.coverage.spans.len() >= 10,
        "the retained fade must be covered by separate singleton spans"
    );
    let index = fixture
        .store
        .source_video_index(partitioned.revision(), &asset())?;
    for span in &context.coverage.spans {
        assert!(
            !span
                .end_exclusive
                .checked_sub(span.start.position)?
                .compare_integer(1)
                .is_gt(),
            "context span is longer than one project frame"
        );
        let first = span.start.picture.select_source_frame(&index)?.identity;
        let near_end = span
            .end_exclusive
            .checked_sub(ExactRatio::new(1, 1_000_000)?)?;
        let last = partitioned
            .plan()
            .definition_picture(
                &context.boundaries.definition,
                near_end,
                BoundaryQueryLimits::default(),
            )?
            .picture
            .select_source_frame(&index)?
            .identity;
        assert_eq!(
            first, last,
            "old singleton-span bypass must apply to every retained span"
        );
    }
    drop(partitioned);
    let before = fixture.store.snapshot()?;
    let history = fixture.store.history_availability()?;
    let error = capture(&fixture, &target(Vec::new()), ExtensionDirection::FromLeft)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("detected picture transition"),
        "partitioned fade: {error}"
    );
    assert_unchanged(&fixture, &before, history)?;
    Ok(())
}
