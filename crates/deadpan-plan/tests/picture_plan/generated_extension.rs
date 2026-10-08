use super::*;

fn fixture(
    direction: ExtensionDirection,
    output: i64,
) -> (GeneratedArtifact, BTreeMap<AssetId, AssetRecord>) {
    let (mut artifact, mut assets) = generated_fixture("extension-", ['7', '8', '9']);
    artifact.sampling = ExtensionSamplingMap::new(
        direction,
        FrameRate::new(30_000, 1001).unwrap(),
        FrameRate::new(24, 1).unwrap(),
        duration(9),
        duration(8),
        duration(output),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
    )
    .unwrap()
    .into();
    for (id, count) in [
        (&artifact.sampled_asset, output),
        (&artifact.native_asset, 17),
    ] {
        let record = assets.get_mut(id).unwrap();
        record.frame_count = Some(duration(count));
        record.video = Some(span(0, count * 1001));
    }
    (artifact, assets)
}

fn edit(before: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let transaction = apply(
        before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap();
    let after = transaction.forward.apply(before).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *before);
    after
}

fn assert_sampled(
    plan: &RenderPlan,
    project_frame: i64,
    sampled_position: ExactRatio,
    artifact: &GeneratedArtifact,
    index: &SourceFrameIndex,
) {
    let picture = plan.picture(ProjectFrame(project_frame)).unwrap().picture;
    let Picture::Accepted {
        asset,
        frame,
        generated: Some(actual),
        position,
        ..
    } = &picture
    else {
        panic!("expected sampled extension picture")
    };
    assert_eq!(asset, &artifact.sampled_asset);
    assert_eq!(actual.as_ref(), artifact);
    assert_eq!(
        *frame,
        SourceFrameId(u64::try_from(sampled_position.floor()).unwrap())
    );
    assert_eq!(*position, sampled_position);
    assert_eq!(picture.select_source_frame(index).unwrap().identity, *frame);
}

#[test]
fn extension_output_and_shortened_prefix_use_only_sampled_ordinals_in_both_directions() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        for output in [1, 13] {
            let initial = document(
                &["hold", "source"],
                vec![
                    ("hold", hold(output)),
                    ("source", source(10, 10_000, 20_010)),
                ],
            );
            let (artifact, assets) = fixture(direction, output);
            let accepted = edit(
                &initial,
                "accepted",
                Command::AcceptGeneratedHold {
                    node: id("hold"),
                    artifact: artifact.clone(),
                    assets,
                },
            );
            let accepted = ProjectDocument::from_json(&accepted.to_json().unwrap()).unwrap();
            let full = RenderPlan::compile(&accepted).unwrap();
            let pts: Vec<_> = (0..output).map(|n| n * 1001).collect();
            let sampled_index = index(
                artifact.sampled_asset.as_str(),
                clock(),
                &pts,
                output * 1001,
            );
            for frame in 0..output {
                assert_sampled(
                    &full,
                    frame,
                    ExactRatio::new(i128::from(2 * frame + 1), 2).unwrap(),
                    &artifact,
                    &sampled_index,
                );
            }
            let following = full.picture(ProjectFrame(output)).unwrap().picture;
            let shorter_count = output.min(2);
            let shorter = edit(
                &accepted,
                "shorter",
                Command::SetHoldDuration {
                    node: id("hold"),
                    duration: duration(shorter_count),
                },
            );
            let short = RenderPlan::compile(&shorter).unwrap();
            // FromRight keeps the same chronological prefix too. Neither the
            // direction nor the shortened duration chooses a different suffix.
            for frame in 0..shorter_count {
                assert_sampled(
                    &short,
                    frame,
                    ExactRatio::new(i128::from(2 * frame + 1), 2).unwrap(),
                    &artifact,
                    &sampled_index,
                );
                assert_eq!(
                    short.picture(ProjectFrame(frame)).unwrap().picture,
                    full.picture(ProjectFrame(frame)).unwrap().picture
                );
            }
            assert_eq!(
                short.picture(ProjectFrame(shorter_count)).unwrap().picture,
                following
            );
            let regrown = edit(
                &shorter,
                "regrown",
                Command::SetHoldDuration {
                    node: id("hold"),
                    duration: duration(output),
                },
            );
            let grown = RenderPlan::compile(&regrown).unwrap();
            for frame in 0..output + 10 {
                assert_eq!(
                    grown.picture(ProjectFrame(frame)).unwrap().picture,
                    full.picture(ProjectFrame(frame)).unwrap().picture
                );
            }
            // The complete native movie cannot masquerade as the sampled index.
            let native_pts: Vec<_> = (0..17).map(|n| n * 1001).collect();
            let wrong = index(
                artifact.sampled_asset.as_str(),
                clock(),
                &native_pts,
                17 * 1001,
            );
            assert!(
                deadpan_plan::PictureClockSlope::Constant
                    .source_ordinals(
                        &full.picture(ProjectFrame(0)).unwrap().picture,
                        ExactRatio::ZERO,
                        &wrong,
                    )
                    .is_err()
            );
        }
    }
}

#[test]
fn repeated_and_retimed_extension_keeps_the_materialized_sampled_clock() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let initial = document(
            &["outer"],
            vec![
                ("hold", hold(8)),
                ("fast", retime("hold", 4, 0, 8)),
                ("outer", repeat("fast", 3, 0, "plays")),
            ],
        );
        let (artifact, assets) = fixture(direction, 8);
        let accepted = edit(
            &initial,
            "accepted",
            Command::AcceptGeneratedHold {
                node: id("hold"),
                artifact: artifact.clone(),
                assets,
            },
        );
        let plan = RenderPlan::compile(&accepted).unwrap();
        let pts: Vec<_> = (0..8).map(|n| n * 1001).collect();
        let sampled_index = index(artifact.sampled_asset.as_str(), clock(), &pts, 8 * 1001);
        assert_eq!(plan.duration(), duration(12));
        for frame in 0..12 {
            assert_sampled(
                &plan,
                frame,
                ExactRatio::integer((frame % 4) * 2 + 1),
                &artifact,
                &sampled_index,
            );
        }
    }
}
