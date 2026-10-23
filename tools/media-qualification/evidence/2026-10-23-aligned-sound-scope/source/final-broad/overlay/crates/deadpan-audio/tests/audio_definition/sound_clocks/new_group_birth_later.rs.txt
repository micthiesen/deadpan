//! A processing owner born after an earlier crop cannot recover that lost input.
//! The allocation is literally6407 ->6406 before birth, then6407 on copy.
use super::split::{active, equalize_delivered, expected_input, raw_comparison};
use super::*;
use deadpan_plan::{AudioBoundaryRule, AudioOccurrenceWindow, AudioSampleGrid, AudioSoundRoute};

// Independent public-route delivery oracle. SOUND_EVENTS.md, "Retained sample
// routes", requires each Keep to preserve the preceding physical audible mask.
// The mask clips OUTPUT only; equalize_delivered still receives the complete
// transported birth input, including its retained FIR context.
// Literal full4f windows at10 ->17 describe G's birth and copy placement. No
// candidate scope, private processing-history result or measured output supplies
// this route or its permission decisions.
fn copied_birth_delivery_mask(first: i64, end: i64) -> Trial<(Vec<bool>, serde_json::Value, bool)> {
    let extent = ExactRatio::integer(4);
    let keep = deadpan_core::SoundRippleMap::new(
        extent,
        0,
        vec![deadpan_core::SoundRippleNode::Keep {
            range: deadpan_core::ExactFrameRange {
                start: ExactRatio::ZERO,
                end: extent,
            },
        }],
    )
    .map_err(|e| error("later independent delivery Keep", e))?;
    let route = deadpan_core::SoundRoute::identity(extent)
        .map_err(|e| error("later independent delivery identity", e))?
        .ripple(keep)
        .map_err(|e| error("later independent delivery ripple", e))?;
    let step = ExactRatio::new(30_000, 48_000 * 1001).unwrap();
    let grids = [10, 17]
        .into_iter()
        .map(|frame| {
            AudioSampleGrid::new(
                ExactRatio::integer(-frame),
                step,
                AudioBoundaryRule::RoundEven,
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| error("later independent delivery grids", e))?;
    let route = AudioSoundRoute::<AudioSample>::new(route, grids)
        .map_err(|e| error("later independent delivery route", e))?
        .with_occurrence_windows(vec![
            AudioOccurrenceWindow {
                selection: ExactRatio::ZERO..extent,
                anchor: ExactRatio::ZERO,
            };
            2
        ])
        .map_err(|e| error("later independent delivery windows", e))?;
    let recipe = route.recipe_samples();
    let allocation = route.samples();
    require(
        recipe == (AudioSample(16_016)..AudioSample(22_422))
            && allocation == (AudioSample(first)..AudioSample(end))
            && first == 27_227
            && end == 33_634,
        "later independent delivery allocation",
        "literal birth/copy grids must allocate6406 ->6407 labels",
    )?;
    let query = route
        .query(
            AudioSample(first)..AudioSample(end),
            deadpan_plan::AudioQueryLimits::default(),
        )
        .map_err(|e| error("later independent delivery query", e))?;
    let mut covered = vec![false; (end - first) as usize];
    let mut recipe_labels = vec![None; (end - first) as usize];
    let mut spans = Vec::new();
    for span in &query.spans {
        require(
            span.samples.start.0 >= first
                && span.samples.end.0 <= end
                && span.samples.start.0 < span.samples.end.0,
            "later independent delivery span",
            "query returned an invalid output interval",
        )?;
        spans.push(serde_json::json!({
            "samples":[span.samples.start.0,span.samples.end.0],
            "gap":span.sampling.is_none(),
        }));
        for sample in span.samples.start.0..span.samples.end.0 {
            let index = (sample - first) as usize;
            require(
                !covered[index],
                "later independent delivery coverage",
                "overlapping spans",
            )?;
            covered[index] = true;
            recipe_labels[index] = span
                .sampling
                .as_ref()
                .map(|mapping| {
                    let local = mapping.local_at(AudioSample(sample))?;
                    route.recipe_grid().boundary(local).map(|label| label.0)
                })
                .transpose()
                .map_err(|e| error("later independent delivery sample map", e))?;
        }
    }
    let maps_33632_to_22421 = recipe_labels[(33_632 - first) as usize] == Some(22_421);
    let gap_33633 =
        recipe_labels[(33_633 - first) as usize].is_none() && covered[(33_633 - first) as usize];
    // Independent integer-label transport checks every label, not only the seam.
    let literal_labels_match = recipe_labels.iter().enumerate().all(|(index, label)| {
        *label
            == if index < 6406 {
                Some(16_016 + index as i64)
            } else {
                None
            }
    });
    let complete = covered.iter().all(|value| *value);
    let mask = recipe_labels
        .iter()
        .map(Option::is_some)
        .collect::<Vec<_>>();
    let evidence = serde_json::json!({
        "authority":"docs/SOUND_EVENTS.md:539-555, retained physical output mask",
        "construction":"public identity Keep; literal full4f placements10 ->17; RoundEven",
        "birth":[recipe.start.0,recipe.end.0],
        "destination":[allocation.start.0,allocation.end.0],
        "spans":spans,"covered_per_sample":covered,
        "recipe_sample_labels":recipe_labels,"delivered_per_sample":mask,
        "complete":complete,"literal_labels_match":literal_labels_match,
        "sample33632_maps_to22421":maps_33632_to_22421,"sample33633_is_gap":gap_33633,
    });
    Ok((
        mask,
        evidence,
        complete && literal_labels_match && maps_33632_to_22421 && gap_33633,
    ))
}

fn run(directory: &std::path::Path) -> Trial<serde_json::Value> {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    wire["nodes"]["lead"] = serde_json::to_value(hold(4, HoldAudio::Silence)).unwrap();
    let initial = ProjectDocument::from_json(&wire.to_string())
        .map_err(|e| error("later lead4 admission", e))?;
    require(
        initial.audio_bindings().is_empty(),
        "later initial",
        "unexpected preexisting journals",
    )?;
    let source_terminal = input(&initial, &id("owner"), 12_812, 12_813, &[1])?;
    let source_terminal_nonzero =
        source_terminal.samples[0].iter().any(|v| *v != 0.0) && !source_terminal.suppressed[0];
    let top = command(
        &initial,
        "later-top",
        Command::Group {
            parent: initial.root().clone(),
            start: 1,
            end: 3,
            id: id("top"),
            label: "Top".into(),
        },
    )?;
    let NodeKind::Hold { recipe } = hold(6, HoldAudio::Silence).kind else {
        unreachable!()
    };
    let moved = command(
        &top,
        "later-prefix-six",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: id("later-prefix"),
            identities: SplitIdentities::default(),
            timing: timing("later-prefix-six"),
        },
    )?;
    // This is a pre-birth delivery control, not a retained raw-owner query:
    // old Source owner history can legitimately expose its complete old input.
    let prebirth_delivery = read_bus(&moved, 22_421, 22_424, &[3])?;
    let selection = SliceCaptureSelection::Child { node: id("owner") };
    require(
        moved
            .group_selection(&id("top"), &selection)
            .map_err(|e| error("later group plan", e))?
            .required_split_ids
            == 0,
        "later group",
        "unexpected splits",
    )?;
    let grouped = command(
        &moved,
        "later-group",
        Command::GroupSelection {
            parent: id("top"),
            selection,
            label: "Later-born group".into(),
            identities: GroupSelectionIdentities {
                group: id("later-g"),
                split: SplitIdentities::default(),
            },
            timing: timing("later-group"),
        },
    )?;
    let treated = command(
        &grouped,
        "later-g-eq",
        Command::SetAudioTreatments {
            node: id("later-g"),
            treatments: eq([4500, -2500, 1500]),
        },
    )?;
    // A terminal silent append chooses a6407-label copy destination without
    // moving G. The insertion path may retain an additional pre-append table;
    // that capture is not an earlier or later placement of the event.
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    let born = command(
        &treated,
        "later-terminal-pad",
        Command::InsertTime {
            at: ProjectFrame(treated.duration().unwrap().frames()),
            hold: recipe,
            id: id("later-pad"),
            identities: SplitIdentities::default(),
            timing: timing("later-terminal-pad"),
        },
    )?;
    retain(directory, "initial.json", &initial)?;
    retain(directory, "before-group.json", &moved)?;
    retain(directory, "before-pad.json", &treated)?;
    retain(directory, "birth.json", &born)?;
    require(
        treated.duration().unwrap().frames() == 16 && born.duration().unwrap().frames() == 17,
        "later pad duration",
        "terminal insertion must extend16f to17f",
    )?;
    require(
        born.beat_sounds() == treated.beat_sounds() && born.sounds() == treated.sounds(),
        "later pad event",
        "terminal insertion changed an attached event recipe",
    )?;
    for name in ["owner", "later-g", "top", "tail", "lead", "later-prefix"] {
        require(
            born.nodes()[&id(name)] == treated.nodes()[&id(name)],
            "later pad recipe",
            format!("terminal insertion changed existing node {name}"),
        )?;
    }
    let before_layout =
        FrozenAudioLayout::capture(&treated).map_err(|e| error("pre-pad layout", e))?;
    let after_layout =
        FrozenAudioLayout::capture(&born).map_err(|e| error("post-pad layout", e))?;
    let g_instance = InstancePath {
        node: id("later-g"),
        repeats: vec![],
    };
    let mut pad_placements = Vec::new();
    for node in ["later-g", "owner"] {
        let instance = InstancePath {
            node: id(node),
            repeats: vec![],
        };
        let before = before_layout
            .sound_occurrence_placement(&instance, Some(&g_instance), MAX_DOCUMENT_NODES)
            .map_err(|e| error("pre-pad placement", e))?;
        let after = after_layout
            .sound_occurrence_placement(&instance, Some(&g_instance), MAX_DOCUMENT_NODES)
            .map_err(|e| error("post-pad placement", e))?;
        require(
            before.owner_origin == ExactRatio::integer(10)
                && before.allocation == (ExactRatio::integer(10)..ExactRatio::integer(14))
                && before.owner_origin == after.owner_origin
                && before.allocation == after.allocation
                && before.delivery_start == after.delivery_start,
            "later pad placement",
            format!("{node} placement changed: {before:?} -> {after:?}"),
        )?;
        pad_placements.push(
            serde_json::json!({"node":node,"before":format!("{before:?}"),
            "after":format!("{after:?}")}),
        );
    }
    let sound = sound_id("effect");
    let prior_journal = &treated.audio_bindings().sound_clocks()[&id("owner")][&sound];
    let padded_journal = &born.audio_bindings().sound_clocks()[&id("owner")][&sound];
    require(
        padded_journal.scope() == prior_journal.scope()
            && padded_journal.clocks().starts_with(prior_journal.clocks())
            && padded_journal.clocks().len() <= prior_journal.clocks().len() + 1,
        "later pad ordered history",
        "terminal insertion rewrote or reordered prior sound history",
    )?;
    for reference in &padded_journal.clocks()[prior_journal.clocks().len()..] {
        require(
            reference.timing() == &timing("later-terminal-pad")
                && reference.scope() == &id("top")
                && reference.owner() == &id("owner")
                && reference.introduced_groups().is_empty()
                && reference.repeats().is_empty()
                && born.audio_bindings().timings().get(reference.timing()) == Some(&before_layout),
            "later pad suffix capture",
            "additional journal reference is not the exact pre-append capture",
        )?;
    }
    for (timing, layout) in treated.audio_bindings().timings() {
        require(
            born.audio_bindings().timings().get(timing) == Some(layout),
            "later pad prior table",
            format!("terminal insertion changed prior table {timing:?}"),
        )?;
    }
    let mut added_tables = Vec::new();
    for (capture, layout) in born.audio_bindings().timings() {
        if treated.audio_bindings().timings().contains_key(capture) {
            continue;
        }
        require(
            capture == &timing("later-terminal-pad") && layout.rate() == before_layout.rate(),
            "later pad capture identity",
            "additional timing is not owned by the terminal insertion",
        )?;
        let sound_referenced = padded_journal
            .clocks()
            .iter()
            .any(|reference| reference.timing() == capture);
        let original_consumers = born
            .audio_bindings()
            .bindings()
            .iter()
            .chain(born.audio_bindings().gap_bindings())
            .filter_map(|(owner, binding)| {
                let uses = binding.lattice.reference.timing == *capture
                    || binding
                        .resume
                        .iter()
                        .flat_map(|resume| &resume.phase.terms)
                        .any(|term| term.placement.reference.timing == *capture)
                    || binding
                        .reanchors
                        .iter()
                        .any(|step| step.placement.reference.timing == *capture);
                uses.then_some(owner.clone())
            })
            .collect::<Vec<_>>();
        require(
            sound_referenced || !original_consumers.is_empty(),
            "later pad capture consumers",
            "additional table has no sound or Original binding consumer",
        )?;
        if sound_referenced {
            // Sound journals need the actual pre-pad G/owner placement. An
            // unrelated Original lattice may use a compact timing skeleton
            // whose padding nodes intentionally have other local extents.
            for node in ["later-g", "owner"] {
                let instance = InstancePath {
                    node: id(node),
                    repeats: vec![],
                };
                let before = before_layout
                    .sound_occurrence_placement(&instance, Some(&g_instance), MAX_DOCUMENT_NODES)
                    .map_err(|e| error("pre-pad sound capture placement", e))?;
                let captured = layout
                    .sound_occurrence_placement(&instance, Some(&g_instance), MAX_DOCUMENT_NODES)
                    .map_err(|e| error("pad sound capture placement", e))?;
                require(
                    before.owner_origin == captured.owner_origin
                        && before.allocation == captured.allocation
                        && before.delivery_start == captured.delivery_start,
                    "later pad sound capture phase",
                    format!("sound capture moved {node}"),
                )?;
            }
        }
        added_tables.push(serde_json::json!({"timing":capture,"layout":layout,
            "sound_journal_referenced":sound_referenced,"original_binding_consumers":original_consumers}));
    }
    let pad_evidence = serde_json::json!({"before_frames":16,"after_frames":17,
        "placements":pad_placements,"previous_journal":prior_journal,"padded_journal":padded_journal,
        "additional_tables":added_tables});
    retain(directory, "pad-capture.json", &pad_evidence)?;
    let birth_first = 16_016;
    let birth_end = 22_422;
    let first = 27_227;
    let end = 33_634;
    let shift = 11_211;
    require(
        frame_sample_boundary(4) == 6406
            && frame_sample_boundary(8) == 12813
            && frame_sample_boundary(10) == birth_first
            && frame_sample_boundary(14) == birth_end
            && frame_sample_boundary(17) == first
            && frame_sample_boundary(21) == end
            && birth_end - birth_first == 6406
            && end - first == 6407
            && 12_812 + 9_610 + shift == 33_633,
        "later literal oracle",
        "unexpected6407->6406->6407 chronology",
    )?;
    let born_input = input(&born, &id("later-g"), birth_first, birth_end, &[173, 256])?;
    let lost_at_birth = input(&born, &id("later-g"), birth_end, birth_end + 1, &[1])?;
    let birth_lost = lost_at_birth.samples[0] == [0.0; 2] && lost_at_birth.suppressed[0];
    require(
        born_input.support == [birth_first, birth_end] && born_input.samples.len() == 6406,
        "later birth support",
        "requires exactly6406 delivered birth labels",
    )?;
    retain(
        directory,
        "source-terminal.json",
        &input_evidence(&source_terminal),
    )?;
    retain(
        directory,
        "delivered-birth-input.json",
        &input_evidence(&born_input),
    )?;
    retain(
        directory,
        "lost-at-birth.json",
        &input_evidence(&lost_at_birth),
    )?;
    let slice = CapturedEditSlice::capture_selection(
        &born,
        &id("top"),
        &SliceCaptureSelection::Child {
            node: id("later-g"),
        },
        timing("later-capture"),
    )
    .map_err(|e| error("later capture", e))?;
    slice
        .validate_capture(&born)
        .map_err(|e| error("later capture validation", e))?;
    retain(directory, "captured-slice.json", &slice)?;
    let captured_wire = serde_json::to_value(&slice).unwrap();
    let captured_bindings =
        AudioBindingState::from_json(&captured_wire["audio_bindings"].to_string())
            .map_err(|e| error("later captured bindings", e))?;
    let captured_journal = &captured_bindings.sound_clocks()[&id("owner")][&sound];
    require(
        captured_journal.scope() == &id("later-g")
            && captured_journal.clocks().len() == padded_journal.clocks().len() + 1
            && captured_journal.clocks()[..padded_journal.clocks().len()]
                .iter()
                .zip(padded_journal.clocks())
                .all(|(new, old)| new.timing() == old.timing())
            && captured_journal.clocks().last().unwrap().timing() == &timing("later-capture"),
        "later captured ordered history",
        "capture did not preserve every padded reference before its own",
    )?;
    let pasted = command(
        &born,
        "later-paste",
        Command::SpliceSlice {
            parent: born.root().clone(),
            index: born.children(born.root()).count(),
            identities: slice_identities(&slice, "later-copy"),
            slice,
            timing: timing("later-paste"),
        },
    )?;
    retain(directory, "pasted.json", &pasted)?;
    require(
        pasted.duration().unwrap().frames() == 21,
        "later final duration",
        "expected21f",
    )?;
    let group = id("later-copy-node-1");
    let owner = id("later-copy-node-2");
    require(
        pasted.beat_sounds()[&owner] == born.beat_sounds()[&id("owner")]
            && pasted.nodes()[&group].audio_treatments
                == born.nodes()[&id("later-g")].audio_treatments,
        "later copied recipes",
        "event or G EQ changed",
    )?;
    let journal = &pasted.audio_bindings().sound_clocks()[&owner][&sound_id("effect")];
    require(
        journal.scope() == &group
            && journal.clocks().len() == captured_journal.clocks().len()
            && journal.clocks()[0].introduced_groups() == &BTreeSet::from([group.clone()])
            && journal
                .clocks()
                .iter()
                .skip(1)
                .all(|reference| reference.introduced_groups().is_empty()),
        "later mixed history",
        "requires retained pre-birth reference followed by the complete G-birth suffix",
    )?;
    let renamed_timings = captured_bindings
        .timings()
        .keys()
        .enumerate()
        .map(|(index, old)| {
            (
                old.clone(),
                AudioTimingId {
                    allocation: RevisionId::new("later-paste").unwrap(),
                    ordinal: 1 + index as u32,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (after, before) in journal.clocks().iter().zip(captured_journal.clocks()) {
        require(
            after.timing() == &renamed_timings[before.timing()],
            "later pasted reference order",
            "paste reordered a prefix, terminal or capture timing",
        )?;
    }
    let plan = deadpan_plan::RenderPlan::compile(&pasted).map_err(|e| error("later plan", e))?;
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: group.clone(),
                repeats: vec![],
            },
            Default::default(),
        )
        .map_err(|e| error("later context", e))?;
    let scopes = plan
        .beat_sound_clock_scopes(
            &owner,
            &sound_id("effect"),
            deadpan_core::MAX_DOCUMENT_NODES,
        )
        .map_err(|e| error("later scopes", e))?;
    require(
        scopes.len() == journal.clocks().len(),
        "later context count",
        "Plan dropped a retained reference",
    )?;
    for (index, scope) in scopes.iter().enumerate() {
        let alias = scope
            .try_remap_context_with_work(&context, deadpan_core::MAX_DOCUMENT_NODES)
            .map_err(|e| error("later context alias", e))?
            .0;
        require(
            if index == 0 {
                alias.is_none()
            } else {
                alias.is_some()
            },
            "later context chronology",
            "G context existed before birth or disappeared in its suffix",
        )?;
    }
    let raw_first = first - 2048;
    let raw_end = end + 2048;
    let (expected_raw, expected_suppressed) =
        expected_input(&born_input, birth_first, shift, raw_first, raw_end);
    let actual_raw = input(&pasted, &group, raw_first, raw_end, &[37, 173, 256])?;
    retain(directory, "copied-input.json", &input_evidence(&actual_raw))?;
    retain(
        directory,
        "expected-input.json",
        &serde_json::json!({
            "first":raw_first,"sample_bits":expected_raw.iter().map(|s|s.map(f64::to_bits)).collect::<Vec<_>>(),
            "suppressed_per_sample":expected_suppressed,
        }),
    )?;
    let raw_diff = raw_comparison(raw_first, &expected_raw, &expected_suppressed, &actual_raw);
    let lost_index = (33_633 - raw_first) as usize;
    let no_resurrection =
        actual_raw.samples[lost_index] == [0.0; 2] && actual_raw.suppressed[lost_index];
    let recipe = pasted.nodes()[&group].audio_treatments.equalizer().unwrap();
    let mut expected =
        equalize_delivered(&born_input, birth_first, shift, first, end, end, recipe)?;
    let before_copy = read_bus(&born, 0, first, &[1021])?;
    expected[..first as usize].copy_from_slice(&before_copy);
    // Preserve the original FIR-only oracle before applying delivery permission.
    // The FIR has already consumed all transported birth input above. A Gap in
    // this separately constructed public route only withholds its output label.
    let fir_only = expected.clone();
    let (delivery_mask, delivery_evidence, delivery_valid) =
        copied_birth_delivery_mask(first, end)?;
    retain(
        directory,
        "independent-delivery-mask.json",
        &delivery_evidence,
    )?;
    for (offset, delivered) in delivery_mask.iter().enumerate() {
        if !delivered {
            expected[first as usize + offset] = [0.0; 2];
        }
    }
    let actual = read_bus(&pasted, 0, end, &[37, 509, 1021, 173])?;
    let fir_only_lost_nonzero = fir_only[33_633].iter().any(|value| *value != 0.0);
    let actual_lost_zero = actual[33_633] == [0.0; 2];
    let unmasked_pcm_diff = differences32(&fir_only, &actual);
    let pcm_diff = differences32(&expected, &actual);
    let partitions = differences32(&actual, &read_bus(&pasted, 0, end, &[1021])?);
    let mut cold = Vec::new();
    for at in [first - 1, first, 33_632, end - 3] {
        let fresh = read_bus(&pasted, at, (at + 3).min(end), &[3])?;
        cold.push(serde_json::json!({"first":at,"comparison":differences32(
            &actual[at as usize..at as usize+fresh.len()],&fresh)}));
    }
    let no_eq = command(
        &pasted,
        "later-ablate-eq",
        Command::SetAudioTreatments {
            node: group,
            treatments: AudioTreatments::default(),
        },
    )?;
    let no_event = command(
        &pasted,
        "later-ablate-event",
        Command::DeleteBeatSound {
            owner,
            id: sound_id("effect"),
        },
    )?;
    let no_eq_pcm = read_bus(&no_eq, 0, end, &[1021])?;
    let no_event_pcm = read_bus(&no_event, 0, end, &[1021])?;
    let eq_active = active(&actual[first as usize..], &no_eq_pcm[first as usize..]);
    let event_active = active(&actual[first as usize..], &no_event_pcm[first as usize..]);
    let controls_valid = source_terminal_nonzero
        && prebirth_delivery[1] == [0.0; 2]
        && birth_lost
        && delivery_valid
        && fir_only_lost_nonzero
        && eq_active > 0
        && event_active > 0
        && partitions["different_channel_samples"] == 0
        && cold
            .iter()
            .all(|row| row["comparison"]["different_channel_samples"] == 0);
    Ok(serde_json::json!({
        "controls_valid":controls_valid,"passed":no_resurrection && actual_lost_zero && raw_diff["passed"]==true
            && pcm_diff["different_channel_samples"]==0,
        "no_resurrection":no_resurrection,
        "source_terminal_nonzero":source_terminal_nonzero,"birth_lost":birth_lost,
        "prebirth_delivery_bits":prebirth_delivery.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "literal_oracle":{"original_allocation":6407,"birth_allocation":6406,"copy_allocation":6407,
            "birth":[birth_first,birth_end],"destination":[first,end],"shift":shift,"lost_copy_label":33633},
        "lost_copy_bits":actual_raw.samples[lost_index].map(f64::to_bits),
        "lost_copy_suppressed":actual_raw.suppressed[lost_index],
        "raw":raw_diff,"full_pcm":pcm_diff,"partitions":partitions,"cold":cold,
        "independent_delivery":delivery_evidence,"delivery_valid":delivery_valid,
        "fir_only_full_pcm":unmasked_pcm_diff,
        "fir_only_sample33633_nonzero":fir_only_lost_nonzero,"actual_sample33633_zero":actual_lost_zero,
        "sample33633_bits":{"fir_only":fir_only[33_633].map(f32::to_bits),
            "masked_expected":expected[33_633].map(f32::to_bits),
            "actual":actual[33_633].map(f32::to_bits)},
        "fir_only_pcm_bits":fir_only.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "eq_active_samples":eq_active,"event_active_samples":event_active,
        "expected_pcm_bits":expected.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "actual_pcm_bits":actual.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "no_eq_pcm_bits":no_eq_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "no_event_pcm_bits":no_event_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "executed_by_draft_author":false,"independent_dsp_claim":false,
    }))
}
#[test]
fn later_born_equalized_group_does_not_revive_prebirth_terminal_when_copy_widens() {
    let directory = std::env::var_os("DEADPAN_SCOPE_PROBE_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("deadpan-group-birth-later-{}", std::process::id()))
        })
        .join("new-group-birth-later");
    std::fs::create_dir_all(&directory).unwrap();
    let result = run(&directory).unwrap_or_else(|failure| {
        serde_json::json!({
            "controls_valid":false,"passed":false,"failure":failure,
            "classification":"guard/admission/setup/API failure; no DSP conclusion",
        })
    });
    retain(&directory, "result.json", &result).unwrap();
    eprintln!(
        "later-birth evidence: {}\ncontrols_valid={} passed={} failure={}",
        directory.display(),
        result["controls_valid"],
        result["passed"],
        result["failure"]
    );
    assert!(
        result["controls_valid"] == true,
        "later-birth control evidence retained"
    );
    assert!(
        result["passed"] == true,
        "prebirth resurrection/PCM evidence retained"
    );
}
