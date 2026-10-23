//! DRAFT ONLY, uncompiled/unexecuted. Child of new_group_birth.rs.
//! #[path = "new_group_birth_authority.rs"] mod authority;
//! Filter: group_birth_processing_history_rejects_incomplete_foreign_and_forged_authority
//! Public handles only; no document/journal fabrication. Literal full4f grids
//! at4/9/15 test the actual first-positive fixture before any negative claim.
use super::*;
use deadpan_plan::{AudioBoundaryRule, AudioOccurrenceWindow, AudioSampleGrid, AudioSoundRoute};

fn route(starts: &[i64], selections: &[(i64, i64, i128)]) -> Trial<AudioSoundRoute<AudioSample>> {
    let length = ExactRatio::integer(4);
    let mut route =
        deadpan_core::SoundRoute::identity(length).map_err(|e| error("route identity", e))?;
    for _ in 1..starts.len() {
        let keep = deadpan_core::SoundRippleMap::new(
            length,
            0,
            vec![deadpan_core::SoundRippleNode::Keep {
                range: deadpan_core::ExactFrameRange {
                    start: ExactRatio::ZERO,
                    end: length,
                },
            }],
        )
        .map_err(|e| error("route keep", e))?;
        route = route.ripple(keep).map_err(|e| error("route ripple", e))?;
    }
    let step = ExactRatio::new(30_000, 48_000 * 1001).unwrap();
    let grids = starts
        .iter()
        .map(|at| {
            AudioSampleGrid::new(
                ExactRatio::integer(-*at),
                step,
                AudioBoundaryRule::RoundEven,
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| error("route grids", e))?;
    let windows = selections
        .iter()
        .map(|(first, end, denominator)| AudioOccurrenceWindow {
            selection: ExactRatio::new(i128::from(*first), *denominator).unwrap()
                ..ExactRatio::new(i128::from(*end), *denominator).unwrap(),
            anchor: ExactRatio::ZERO,
        })
        .collect();
    AudioSoundRoute::<AudioSample>::new(route, grids)
        .map_err(|e| error("route", e))?
        .with_occurrence_windows(windows)
        .map_err(|e| error("route windows", e))
}
fn full(starts: &[i64]) -> Trial<AudioSoundRoute<AudioSample>> {
    route(starts, &vec![(0, 4, 1); starts.len()])
}
fn run(directory: &std::path::Path) -> Trial<serde_json::Value> {
    let born = setup(directory)?;
    let slice = CapturedEditSlice::capture_selection(
        &born,
        &id("top"),
        &SliceCaptureSelection::Child {
            node: id("birth-g"),
        },
        timing("authority-capture"),
    )
    .map_err(|e| error("authority capture", e))?;
    slice
        .validate_capture(&born)
        .map_err(|e| error("authority capture validation", e))?;
    let pasted = command(
        &born,
        "authority-paste",
        Command::SpliceSlice {
            parent: born.root().clone(),
            index: born.children(born.root()).count(),
            identities: slice_identities(&slice, "authority-copy"),
            slice,
            timing: timing("authority-paste"),
        },
    )?;
    retain(directory, "authority-pasted.json", &pasted)?;
    let owner = id("authority-copy-node-2");
    let group = id("authority-copy-node-1");
    let event = &pasted.beat_sounds()[&owner][&sound_id("effect")];
    let recipe = deadpan_plan::AudioSourceVoiceRecipe {
        source: event.source.clone(),
        mapping: event.mapping,
        offset: event.offset,
    };
    let plan =
        deadpan_plan::RenderPlan::compile(&pasted).map_err(|e| error("authority plan", e))?;
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: group.clone(),
                repeats: vec![],
            },
            Default::default(),
        )
        .map_err(|e| error("authority context", e))?;
    let current = context
        .source_voice_occurrence(
            InstancePath {
                node: owner.clone(),
                repeats: vec![],
            },
            recipe.clone(),
            Default::default(),
        )
        .map_err(|e| error("authority current occurrence", e))?;
    let scopes = plan
        .beat_sound_clock_scopes(
            &owner,
            &sound_id("effect"),
            deadpan_core::MAX_DOCUMENT_NODES,
        )
        .map_err(|e| error("authority scopes", e))?;
    require(
        scopes.len() == 2,
        "authority fixture",
        "requires two chronological scopes",
    )?;
    require(
        scopes[0]
            .try_remap_context_with_work(&context, deadpan_core::MAX_DOCUMENT_NODES)
            .map_err(|e| error("authority old alias", e))?
            .0
            .is_none()
            && scopes[1]
                .try_remap_context_with_work(&context, deadpan_core::MAX_DOCUMENT_NODES)
                .map_err(|e| error("authority birth alias", e))?
                .0
                .is_some(),
        "authority fixture",
        "requires oldNone/birthSome contexts",
    )?;
    let historical_plan = plan
        .compile_sound_processing_layout(
            scopes[0].historical_layout(),
            &BTreeSet::from([event.source.asset.clone()]),
        )
        .map_err(|e| error("authority historical plan", e))?;
    let historical = scopes[0]
        .remap_instance(current.instance())
        .map_err(|e| error("authority owner alias", e))?;
    let original = historical_plan
        .source_voice_occurrence(historical, recipe, Default::default())
        .map_err(|e| error("authority original", e))?;
    require(
        original.extent() == (ExactRatio::integer(4)..ExactRatio::integer(8))
            && current.extent() == (ExactRatio::integer(15)..ExactRatio::integer(19)),
        "literal authority extents",
        "actual handles differ from4/9/15 four-frame fixture",
    )?;
    let (binding, _) = scopes[0]
        .bind_historical_plan(&historical_plan, deadpan_core::MAX_DOCUMENT_NODES)
        .map_err(|e| error("authority bind", e))?;
    let (alias, _) = binding
        .alias_occurrence(&original, &current, deadpan_core::MAX_DOCUMENT_NODES)
        .map_err(|e| error("authority alias", e))?;
    let delivery = full(&[4, 9, 15])?;
    let (history, work) = current
        .complete_processing_history_after_birth(
            &original,
            &alias,
            &delivery,
            &scopes,
            &context,
            Default::default(),
        )
        .map_err(|e| error("authority positive", e))?;
    let history =
        history.ok_or_else(|| error("authority positive", "complete suffix returned None"))?;
    require(
        work > 0 && history.route().samples() == (AudioSample(24024)..AudioSample(30431)),
        "authority positive",
        "no retained one-label terminal beyond nominal destination",
    )?;
    let terminal = history
        .route()
        .query(AudioSample(30430)..AudioSample(30431), Default::default())
        .map_err(|e| error("authority positive terminal query", e))?;
    require(
        terminal.spans.iter().any(|span| span.sampling.is_some()),
        "authority positive",
        "terminal route has no retained sampling",
    )?;
    let mut results = Vec::new();
    let mut save = |label: &str,
                    result: std::result::Result<
        (Option<deadpan_plan::AudioSoundProcessingHistory<'_>>, usize),
        deadpan_plan::PlanError,
    >| {
        results.push(serde_json::json!({"case":label,"rejected":result.is_err(),
            "error":result.err().map(|e|format!("{e:?}"))}));
    };
    save(
        "truncated-prefix",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &delivery,
            &scopes[..1],
            &context,
            Default::default(),
        ),
    );
    save(
        "truncated-suffix",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &delivery,
            &scopes[1..],
            &context,
            Default::default(),
        ),
    );
    let mut reversed = scopes.clone();
    reversed.reverse();
    save(
        "reordered-complete-journal",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &delivery,
            &reversed,
            &context,
            Default::default(),
        ),
    );
    let foreign_plan = plan.clone();
    let foreign_scopes = foreign_plan
        .beat_sound_clock_scopes(
            &owner,
            &sound_id("effect"),
            deadpan_core::MAX_DOCUMENT_NODES,
        )
        .map_err(|e| error("foreign scopes", e))?;
    save(
        "equal-contents-foreign-plan-scopes",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &delivery,
            &foreign_scopes,
            &context,
            Default::default(),
        ),
    );
    let fresh_context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: group.clone(),
                repeats: vec![],
            },
            Default::default(),
        )
        .map_err(|e| error("fresh context", e))?;
    save(
        "same-owner-fresh-context-arc",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &delivery,
            &scopes,
            &fresh_context,
            Default::default(),
        ),
    );
    let foreign_context = foreign_plan
        .audio_sound_instance_context(
            &InstancePath {
                node: group,
                repeats: vec![],
            },
            Default::default(),
        )
        .map_err(|e| error("foreign context", e))?;
    save(
        "equal-contents-foreign-context",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &delivery,
            &scopes,
            &foreign_context,
            Default::default(),
        ),
    );
    let forged_grid = full(&[4, 10, 15])?;
    save(
        "forged-intermediary-grid9-to10",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &forged_grid,
            &scopes,
            &context,
            Default::default(),
        ),
    );
    let forged_allocation = route(&[4, 9, 15], &[(0, 4, 1), (0, 3999, 1000), (0, 4, 1)])?;
    save(
        "forged-intermediary-allocation3999-over1000",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &forged_allocation,
            &scopes,
            &context,
            Default::default(),
        ),
    );
    let missing = full(&[4, 15])?;
    save(
        "missing-intermediary-placement",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &missing,
            &scopes,
            &context,
            Default::default(),
        ),
    );
    let extra = full(&[4, 9, 9, 15])?;
    save(
        "extra-intermediary-placement",
        current.complete_processing_history_after_birth(
            &original,
            &alias,
            &extra,
            &scopes,
            &context,
            Default::default(),
        ),
    );
    Ok(serde_json::json!({
        "positive":{"work":work,"samples":[history.route().samples().start.0,history.route().samples().end.0],
            "terminal_query":format!("{terminal:?}")},
        "all_rejected":results.iter().all(|row|row["rejected"]==true),"negatives":results,
        "implementation_claim":false,"executed_by_draft_author":false,
    }))
}
#[test]
fn group_birth_processing_history_rejects_incomplete_foreign_and_forged_authority() {
    let directory = std::env::var_os("DEADPAN_SCOPE_PROBE_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "deadpan-group-birth-authority-{}",
                std::process::id()
            ))
        })
        .join("new-group-birth-authority");
    std::fs::create_dir_all(&directory).unwrap();
    let result=run(&directory).unwrap_or_else(|failure|serde_json::json!({
        "all_rejected":false,"failure":failure,"classification":"setup/API/positive authority failure",
    }));
    retain(&directory, "result.json", &result).unwrap();
    eprintln!("authority evidence: {}\n{result}", directory.display());
    assert!(
        result["all_rejected"] == true,
        "authority evidence retained"
    );
}
