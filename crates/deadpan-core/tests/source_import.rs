use deadpan_core::*;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset_id() -> AssetId {
    AssetId::new("original").unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: -48000,
            time_base,
        },
        SourceTimestamp {
            ticks: 48000,
            time_base,
        },
    )
    .unwrap()
}
fn asset() -> AssetRecord {
    AssetRecord {
        label: "Qualified original".into(),
        content_hash: "a".repeat(64),
        video: Some(span()),
        audio: Some(span()),
        still_image: false,
        frame_count: Some(duration(60)),
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    }
}
fn insertion(name: &str, index: usize) -> SourceInsertion {
    SourceInsertion {
        parent: node("root"),
        index,
        node: node(name),
        label: "Imported source".into(),
        source: SourceNode {
            duration: duration(60),
            video: SourceVideo::Stream {
                asset: asset_id(),
                span: span(),
            },
            audio: Some(SourceAudio {
                asset: asset_id(),
                span: span(),
            }),
            video_mapping: SourceVideoMapping::Placement {
                start: ExactRatio::new(1, 3).unwrap(),
                frames: ExactRatio::new(175, 3).unwrap(),
                endpoints: EndpointPolicy::HoldAdjacent,
            },
            audio_mapping: SourceAudioMapping::Placement {
                start: ExactRatio::ZERO,
                frames: ExactRatio::new(60000, 1001).unwrap(),
            },
            audio_offset: AudioSample(0),
            link: LinkRelation::Linked,
        },
    }
}
fn empty() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("import-project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: FrameRate::new(30000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap()
}
fn request(document: &ProjectDocument, command: Command, next: &str) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(next).unwrap(),
        command,
    }
}
fn edit(
    document: &ProjectDocument,
    command: Command,
    next: &str,
) -> (ProjectDocument, EditTransaction) {
    let request = request(document, command, next);
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    (after, transaction)
}
fn import(insertion: Option<SourceInsertion>) -> Command {
    Command::ImportSource {
        id: asset_id(),
        asset: asset(),
        insertion: insertion.map(Box::new),
    }
}

#[test]
fn qualification_identity_is_exact_lowercase_hex_and_optional_wire_omits_absence() {
    let id = SourceQualificationId::new("0123456789abcdef".repeat(4)).unwrap();
    assert_eq!(id.as_str(), "0123456789abcdef".repeat(4));
    assert_eq!(id.to_string(), id.as_str());
    assert_eq!(
        serde_json::to_value(&id).unwrap(),
        serde_json::json!(id.as_str())
    );
    assert_eq!(
        serde_json::from_value::<SourceQualificationId>(serde_json::json!(id.as_str())).unwrap(),
        id
    );
    for invalid in [
        String::new(),
        "a".repeat(63),
        "a".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
        format!("blake3:{}", "a".repeat(64)),
        "é".repeat(32),
    ] {
        assert!(SourceQualificationId::new(invalid.clone()).is_err());
        assert!(
            serde_json::from_value::<SourceQualificationId>(serde_json::json!(invalid)).is_err()
        );
    }
    let mut old = asset();
    old.source_qualification = None;
    let wire = serde_json::to_value(&old).unwrap();
    assert!(wire.get("source_qualification").is_none());
    assert_eq!(serde_json::from_value::<AssetRecord>(wire).unwrap(), old);
}

#[test]
fn import_registers_and_inserts_in_one_reversible_edit_without_changing_basis() {
    let before = empty();
    let insertion = insertion("first", 0);
    let (after, transaction) = edit(&before, import(Some(insertion.clone())), "imported");
    assert_eq!(after.assets().get(&asset_id()), Some(&asset()));
    assert_eq!(
        after.nodes()[&node("first")],
        BeatNode {
            label: insertion.label,
            kind: NodeKind::Source {
                source: insertion.source
            }
        }
    );
    assert_eq!(after.presentation_basis(), before.presentation_basis());
    assert_eq!(transaction.duration_delta, 60);
    assert_eq!(transaction.forward.assets.len(), 1);
    assert_eq!(transaction.forward.nodes.len(), 2);
    assert_eq!(transaction.forward.assets[&asset_id()].before, None);
    assert_eq!(transaction.inverse.assets[&asset_id()].after, None);
    assert_eq!(
        transaction
            .forward
            .apply(&transaction.inverse.apply(&after).unwrap())
            .unwrap(),
        after
    );
}

#[test]
fn matching_assets_can_be_registered_once_and_inserted_repeatedly() {
    let (registered, first) = edit(&empty(), import(None), "registered");
    assert_eq!(registered.nodes().len(), 1);
    assert_eq!(registered.duration().unwrap(), duration(0));
    assert_eq!(first.forward.nodes.len(), 0);
    let (inserted, insertion_edit) =
        edit(&registered, import(Some(insertion("first", 0))), "inserted");
    assert!(insertion_edit.forward.assets.is_empty());
    let (twice, second) = edit(
        &inserted,
        import(Some(insertion("second", 1))),
        "inserted-again",
    );
    assert!(second.forward.assets.is_empty());
    assert_eq!(twice.assets().len(), 1);
    assert_eq!(twice.duration().unwrap(), duration(120));
    let (same, idempotent) = edit(&twice, import(None), "registered-again");
    assert!(idempotent.forward.assets.is_empty());
    assert!(idempotent.forward.nodes.is_empty());
    assert_eq!(same.assets(), twice.assets());
    for mismatch in ["label", "receipt", "span"] {
        let mut changed = asset();
        match mismatch {
            "label" => changed.label = "Different".into(),
            "receipt" => {
                changed.source_qualification =
                    Some(SourceQualificationId::new("c".repeat(64)).unwrap())
            }
            "span" => changed.audio = None,
            _ => unreachable!(),
        }
        let command = Command::ImportSource {
            id: asset_id(),
            asset: changed,
            insertion: Some(Box::new(insertion("third", 2))),
        };
        assert_eq!(
            apply(&twice, &request(&twice, command, "invalid"))
                .unwrap_err()
                .code,
            EditErrorCode::ImmutableAsset
        );
    }
}

#[test]
fn invalid_import_insertion_or_missing_qualification_leaves_no_registered_asset_or_node() {
    let before = empty();
    let snapshot = before.to_json().unwrap();
    for case in [
        "duplicate-node",
        "missing-parent",
        "bad-index",
        "other-video",
        "other-audio",
        "zero-duration",
        "bad-span",
        "missing-receipt",
    ] {
        let mut insert = insertion("first", 0);
        let mut record = asset();
        match case {
            "duplicate-node" => insert.node = node("root"),
            "missing-parent" => insert.parent = node("missing"),
            "bad-index" => insert.index = 1,
            "other-video" => {
                insert.source.video = SourceVideo::Stream {
                    asset: AssetId::new("other").unwrap(),
                    span: span(),
                }
            }
            "other-audio" => {
                insert.source.audio.as_mut().unwrap().asset = AssetId::new("other").unwrap()
            }
            "zero-duration" => insert.source.duration = duration(0),
            "bad-span" => {
                record.video = Some(
                    SourceSpan::new(
                        SourceTimestamp {
                            ticks: 0,
                            time_base: span().start().time_base,
                        },
                        span().end(),
                    )
                    .unwrap(),
                )
            }
            "missing-receipt" => record.source_qualification = None,
            _ => unreachable!(),
        }
        let command = Command::ImportSource {
            id: asset_id(),
            asset: record,
            insertion: Some(Box::new(insert)),
        };
        assert!(
            apply(&before, &request(&before, command, "invalid")).is_err(),
            "{case}"
        );
        assert_eq!(before.to_json().unwrap(), snapshot);
        assert!(before.assets().is_empty());
        assert_eq!(before.nodes().len(), 1);
    }
}

#[test]
fn imported_audio_only_and_still_sources_use_the_supplied_asset() {
    for still in [false, true] {
        let mut record = asset();
        record.video = None;
        record.audio = if still { None } else { Some(span()) };
        record.still_image = still;
        record.frame_count = None;
        let mut insert = insertion("first", 0);
        insert.source.video = if still {
            SourceVideo::Still { asset: asset_id() }
        } else {
            SourceVideo::Blank
        };
        insert.source.video_mapping = SourceVideoMapping::FitBeat;
        insert.source.audio = if still {
            None
        } else {
            Some(SourceAudio {
                asset: asset_id(),
                span: span(),
            })
        };
        if still {
            insert.source.audio_mapping = SourceAudioMapping::FitBeat;
        }
        insert.source.link = LinkRelation::Independent;
        let (document, _) = edit(
            &empty(),
            Command::ImportSource {
                id: asset_id(),
                asset: record.clone(),
                insertion: Some(Box::new(insert.clone())),
            },
            "role-import",
        );
        assert_eq!(document.duration().unwrap(), duration(60));
        if still {
            insert.source.video = SourceVideo::Still {
                asset: AssetId::new("other").unwrap(),
            };
            assert_eq!(
                apply(
                    &empty(),
                    &request(
                        &empty(),
                        Command::ImportSource {
                            id: asset_id(),
                            asset: record,
                            insertion: Some(Box::new(insert))
                        },
                        "invalid"
                    )
                )
                .unwrap_err()
                .code,
                EditErrorCode::SourceRangeInvalid
            );
        }
    }
}

#[test]
fn source_import_applies_normal_biased_marks_and_keeps_source_coordinates() {
    let (mut before, _) = edit(&empty(), import(Some(insertion("first", 0))), "first");
    let source_boundary = BoundaryAnchor {
        coordinate: Anchor::Source {
            asset: asset_id(),
            moment: SourceMoment::AudioSample {
                sample: 0,
                sample_rate: 48000,
            },
        },
        bias: InsertionBias::Right,
    };
    for (id, boundary) in [
        (
            "left",
            BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("root"),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Left,
            },
        ),
        (
            "right",
            BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("root"),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
        ),
        ("original-sample", source_boundary.clone()),
        (
            "pinned",
            BoundaryAnchor {
                coordinate: Anchor::Sequence {
                    frame: ProjectFrame(10),
                },
                bias: InsertionBias::Right,
            },
        ),
    ] {
        before = edit(
            &before,
            Command::SetMark {
                id: MarkId::new(id).unwrap(),
                owner: node("root"),
                label: id.into(),
                boundary,
                loss_policy: AnchorLossPolicy::KeepUnresolved,
            },
            id,
        )
        .0;
    }
    let (after, transaction) = edit(
        &before,
        import(Some(insertion("new-first", 0))),
        "prepended",
    );
    for (id, position) in [("left", 0), ("right", 60)] {
        assert_eq!(
            after.marks()[&MarkId::new(id).unwrap()].boundary.coordinate,
            Anchor::Local {
                node: node("root"),
                position: ExactRatio::integer(position)
            }
        );
    }
    assert_eq!(
        after.marks()[&MarkId::new("original-sample").unwrap()].boundary,
        source_boundary
    );
    assert_eq!(
        after.marks()[&MarkId::new("pinned").unwrap()],
        before.marks()[&MarkId::new("pinned").unwrap()]
    );
    let target = AnchorTarget {
        boundary: source_boundary,
        occurrence: Some(InstancePath {
            node: node("first"),
            repeats: vec![],
        }),
    };
    let original = AnchorIndex::new(&before)
        .unwrap()
        .resolve_target(&target)
        .unwrap();
    let inserted = AnchorIndex::new(&after)
        .unwrap()
        .resolve_target(&target)
        .unwrap();
    assert_eq!(
        inserted.exact_frame,
        original
            .exact_frame
            .checked_add(ExactRatio::integer(60))
            .unwrap()
    );
    assert_eq!(
        transaction.inverse.apply(&after).unwrap().marks(),
        before.marks()
    );
}

#[test]
fn import_cannot_substitute_another_registered_asset_or_inject_a_subtree() {
    let other = AssetId::new("other").unwrap();
    let (before, _) = edit(
        &empty(),
        Command::AddAsset {
            id: other.clone(),
            asset: asset(),
        },
        "other-registered",
    );
    for video in [false, true] {
        let mut insert = insertion("first", 0);
        if video {
            insert.source.video = SourceVideo::Stream {
                asset: other.clone(),
                span: span(),
            };
        } else {
            insert.source.audio.as_mut().unwrap().asset = other.clone();
        }
        assert_eq!(
            apply(
                &before,
                &request(&before, import(Some(insert)), "wrong-reference")
            )
            .unwrap_err()
            .code,
            EditErrorCode::SourceRangeInvalid
        );
        assert!(!before.assets().contains_key(&asset_id()));
    }
    let request = request(&before, import(Some(insertion("first", 0))), "imported");
    let mut wire = serde_json::to_value(request).unwrap();
    wire["command"]["insertion"]["subtree"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<CommandRequest>(wire).is_err());
}
