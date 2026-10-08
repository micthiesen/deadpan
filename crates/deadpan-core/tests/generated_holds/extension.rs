use super::*;

const DIRECTIONS: [ExtensionDirection; 2] =
    [ExtensionDirection::FromLeft, ExtensionDirection::FromRight];

fn sampling(direction: ExtensionDirection, output: i64) -> ExtensionSamplingMap {
    ExtensionSamplingMap::new(
        direction,
        rate(),
        FrameRate::new(24, 1).unwrap(),
        duration(9),
        duration(8),
        duration(output),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
    )
    .unwrap()
}

fn fixture(
    direction: ExtensionDirection,
    output: i64,
) -> (GeneratedArtifact, BTreeMap<AssetId, AssetRecord>) {
    let (mut artifact, mut assets) = generated_fixture();
    artifact.sampling = sampling(direction, output).into();
    assets.get_mut(&artifact.sampled_asset).unwrap().frame_count = Some(duration(output));
    assets.get_mut(&artifact.native_asset).unwrap().frame_count = Some(duration(17));
    (artifact, assets)
}

fn before_acceptance(output: i64) -> ProjectDocument {
    edit(
        &with_hold(HoldVideo::Background),
        Command::SetHoldDuration {
            node: node("hold"),
            duration: duration(output),
        },
        "requested",
    )
}

fn accepted(direction: ExtensionDirection, output: i64) -> (ProjectDocument, GeneratedArtifact) {
    let before = before_acceptance(output);
    let (artifact, assets) = fixture(direction, output);
    let request = request(
        &before,
        Command::AcceptGeneratedHold {
            node: node("hold"),
            artifact: artifact.clone(),
            assets,
        },
        "accepted",
    );
    let request: CommandRequest =
        serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
    let tx = apply(&before, &request).unwrap();
    let retained: EditTransaction =
        serde_json::from_slice(&serde_json::to_vec(&tx).unwrap()).unwrap();
    assert_eq!(retained, tx);
    let next = retained.forward.apply(&before).unwrap();
    let undone = retained.inverse.apply(&next).unwrap();
    assert_eq!(undone, before);
    assert_eq!(retained.forward.apply(&undone).unwrap(), next);
    let reopened = ProjectDocument::from_json(&next.to_json().unwrap()).unwrap();
    assert_eq!(reopened, next);
    (reopened, artifact)
}

fn assert_artifact(document: &ProjectDocument, expected: &GeneratedArtifact) {
    let HoldVideo::Generated { accepted } = &hold_recipe(document, "hold").video else {
        panic!("expected accepted extension")
    };
    assert_eq!(&accepted.artifact, expected);
    assert_eq!(accepted.fallback, HoldFallback::Background);
    assert_eq!(hold_recipe(document, "hold").audio, HoldAudio::Silence);
}

#[test]
fn accepted_sampling_sum_dispatches_exact_maps_and_requires_strict_operation_wire() {
    let bridge = generated_fixture().0.sampling;
    let mut maps = vec![bridge];
    for direction in DIRECTIONS {
        for output in [1, 13] {
            let concrete = sampling(direction, output);
            let sum: GeneratedSamplingMap = concrete.clone().into();
            assert_eq!(sum.project_rate(), rate());
            assert_eq!(sum.native_rate(), FrameRate::new(24, 1).unwrap());
            assert_eq!(sum.native_frame_count(), duration(17));
            assert_eq!(sum.output_frame_count(), duration(output));
            assert_eq!(sum.interpolation(), concrete.interpolation());
            for j in 0..output {
                assert_eq!(sum.native_position(j), concrete.native_position(j));
            }
            assert_eq!(sum.native_position(-1), concrete.native_position(-1));
            assert_eq!(
                sum.native_position(output),
                concrete.native_position(output)
            );
            if output == 1 {
                let numerator = match direction {
                    ExtensionDirection::FromLeft => 25,
                    ExtensionDirection::FromRight => 7,
                };
                assert_eq!(
                    sum.native_position(0).unwrap(),
                    ExactRatio::new(numerator, 2).unwrap()
                );
            }
            maps.push(sum);
        }
    }
    for map in maps {
        let wire = serde_json::to_value(&map).unwrap();
        let operation = match &map {
            GeneratedSamplingMap::Bridge(concrete) => {
                assert_eq!(map.native_position(0), concrete.native_position(0));
                "bridge"
            }
            GeneratedSamplingMap::Extension(_) => "extension",
        };
        assert_eq!(wire["operation"], operation);
        assert_eq!(
            serde_json::from_value::<GeneratedSamplingMap>(wire.clone()).unwrap(),
            map
        );
        // Old untagged artifacts and a tag from the other operation cannot be guessed.
        assert!(serde_json::from_value::<GeneratedSamplingMap>(wire["sampling"].clone()).is_err());
        for (key, value) in [
            ("operation", json!("unknown")),
            (
                "operation",
                json!(if operation == "bridge" {
                    "extension"
                } else {
                    "bridge"
                }),
            ),
            ("unknown", json!(true)),
            ("sampling", serde_json::Value::Null),
        ] {
            let mut bad = wire.clone();
            bad[key] = value;
            assert!(
                serde_json::from_value::<GeneratedSamplingMap>(bad).is_err(),
                "{key}"
            );
        }
        let mut bad = wire.clone();
        bad["sampling"]["unknown"] = json!(true);
        assert!(serde_json::from_value::<GeneratedSamplingMap>(bad).is_err());
        let mut bad = wire.clone();
        bad["sampling"]["schema_version"] = json!(2);
        assert!(serde_json::from_value::<GeneratedSamplingMap>(bad).is_err());
        let duplicate = format!(
            "{{\"operation\":\"{operation}\",\"operation\":\"{operation}\",\"sampling\":{}}}",
            wire["sampling"]
        );
        assert!(serde_json::from_str::<GeneratedSamplingMap>(&duplicate).is_err());
    }
}

#[test]
fn extension_acceptance_history_and_shortening_keep_complete_native_context_and_sampling() {
    for direction in DIRECTIONS {
        for output in [1, 13] {
            let (accepted, artifact) = accepted(direction, output);
            assert_artifact(&accepted, &artifact);
            assert_eq!(
                accepted.assets()[&artifact.native_asset].frame_count,
                Some(duration(17))
            );
            let shorter = edit(
                &accepted,
                Command::SetHoldDuration {
                    node: node("hold"),
                    duration: duration(1),
                },
                "shorter",
            );
            assert_artifact(&shorter, &artifact);
            let regrown = edit(
                &shorter,
                Command::SetHoldDuration {
                    node: node("hold"),
                    duration: duration(output),
                },
                "regrown",
            );
            assert_artifact(&regrown, &artifact);
            let beyond = edit(
                &regrown,
                Command::SetHoldDuration {
                    node: node("hold"),
                    duration: duration(output + 1),
                },
                "beyond",
            );
            assert_eq!(hold_recipe(&beyond, "hold").video, HoldVideo::Background);
            assert_eq!(beyond.assets(), regrown.assets());
        }
    }
}

#[test]
fn extension_acceptance_rejects_generated_only_native_count_sample_count_and_wrong_rate() {
    for direction in DIRECTIONS {
        let before = before_acceptance(13);
        let (artifact, assets) = fixture(direction, 13);
        for (id, count) in [(&artifact.native_asset, 8), (&artifact.sampled_asset, 12)] {
            let mut wrong = assets.clone();
            wrong.get_mut(id).unwrap().frame_count = Some(duration(count));
            assert!(
                apply(
                    &before,
                    &request(
                        &before,
                        Command::AcceptGeneratedHold {
                            node: node("hold"),
                            artifact: artifact.clone(),
                            assets: wrong,
                        },
                        "bad-count"
                    )
                )
                .is_err()
            );
        }
        let mut wrong = artifact.clone();
        wrong.sampling = ExtensionSamplingMap::new(
            direction,
            FrameRate::new(30, 1).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            duration(9),
            duration(8),
            duration(13),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap()
        .into();
        assert!(
            apply(
                &before,
                &request(
                    &before,
                    Command::AcceptGeneratedHold {
                        node: node("hold"),
                        artifact: wrong,
                        assets: assets.clone(),
                    },
                    "bad-rate"
                )
            )
            .is_err()
        );
        let mut too_short = artifact;
        too_short.sampling = sampling(direction, 12).into();
        let mut matching_assets = assets;
        matching_assets
            .get_mut(&too_short.sampled_asset)
            .unwrap()
            .frame_count = Some(duration(12));
        assert!(
            apply(
                &before,
                &request(
                    &before,
                    Command::AcceptGeneratedHold {
                        node: node("hold"),
                        artifact: too_short,
                        assets: matching_assets,
                    },
                    "too-short"
                )
            )
            .is_err()
        );
    }
}

#[test]
fn copied_extension_survives_original_hold_deletion_and_slice_roundtrip_without_new_sampling() {
    for direction in DIRECTIONS {
        let (before, artifact) = accepted(direction, 13);
        let timing = |name| AudioTimingId {
            allocation: RevisionId::new(name).unwrap(),
            ordinal: 0,
        };
        let slice = CapturedEditSlice::capture_selection(
            &before,
            &node("root"),
            &SliceCaptureSelection::Child { node: node("hold") },
            timing("capture"),
        )
        .unwrap();
        slice.validate_capture(&before).unwrap();
        let slice = CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap();
        let deleted = edit(&before, Command::Delete { node: node("hold") }, "deleted");
        let required = slice.identity_requirements().unwrap();
        let copied = edit(
            &deleted,
            Command::SpliceSlice {
                parent: node("root"),
                index: 0,
                slice,
                timing: timing("copied"),
                identities: SlicePasteIdentities {
                    authored: OccurrenceIdentities {
                        nodes: (0..required.nodes)
                            .map(|n| node(&format!("copy-{n}")))
                            .collect(),
                        marks: (0..required.marks)
                            .map(|n| MarkId::new(format!("mark-{n}")).unwrap())
                            .collect(),
                    },
                    aliases: (0..required.aliases)
                        .map(|n| node(&format!("alias-{n}")))
                        .collect(),
                },
            },
            "copied",
        );
        let generated: Vec<_> = copied
            .nodes()
            .values()
            .filter_map(|node| match &node.kind {
                NodeKind::Hold { recipe } => match &recipe.video {
                    HoldVideo::Generated { accepted } => Some(accepted),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(generated.len(), 1);
        assert_eq!(generated[0].artifact, artifact);
        assert_eq!(generated[0].fallback, HoldFallback::Background);
        assert_eq!(copied.duration().unwrap(), duration(13));
        assert_eq!(copied.assets(), before.assets());
        assert_eq!(
            ProjectDocument::from_json(&copied.to_json().unwrap()).unwrap(),
            copied
        );
    }
}
