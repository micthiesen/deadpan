//! A new treatment owner retains exactly the input delivered when it was born.
//! The original group/copy regression has been exercised with exact real PCM;
//! its authority and subsequent-edit regressions remain separately testable.
//! Optional DEADPAN_SCOPE_PROBE_EVIDENCE; writes new-group-birth/ beneath it.
//! Otherwise retains evidence under the process-specific temporary directory.
//!
//! Public candidate path only: lead4 + owner4 + tail2; ordinary Top; prefix5;
//! aligned GroupSelection creates G; add G EQ; capture Child(G); append at15.
//! No manual journal rewrite or fabricated source qualification.
//! The raw oracle is precisely G's DELIVERED input at birth [14414,20821),
//! shifted9610, zero elsewhere. Old history outside G's birth is not an oracle.
//! At destination [24024,30430), label30430 is retained future input, not
//! additional audible time. Fixed absolute1024 tiles and public Equalizer
//! process_tile derive every audible destination output, with exact f32 casts.
//! A green result establishes this single Source chronology only.

use super::*;
use std::collections::BTreeSet;

#[path = "new_group_birth_authority.rs"]
mod authority;

#[path = "new_group_birth_split.rs"]
mod split;

#[path = "new_group_birth_later.rs"]
mod later;

#[path = "new_group_birth_family.rs"]
mod family;

type Trial<T> = std::result::Result<T, serde_json::Value>;
const BIRTH_FIRST: i64 = 14_414;
const BIRTH_END: i64 = 20_821;
const DEST_FIRST: i64 = 24_024;
const DEST_END: i64 = 30_430;
const SHIFT: i64 = 9_610;

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: RevisionId::new(name).unwrap(),
        ordinal: 0,
    }
}
fn eq(gains: [i32; 3]) -> AudioTreatments {
    AudioTreatments::default()
        .with_equalizer(Some(
            EqualizerRecipe::new(gains.map(|gain| GainDb::new(gain).unwrap()), None, None).unwrap(),
        ))
        .unwrap()
}
fn differences32(expected: &[[f32; 2]], actual: &[[f32; 2]]) -> serde_json::Value {
    let mut differences = Vec::new();
    for (sample, (a, b)) in expected.iter().zip(actual).enumerate() {
        for channel in 0..2 {
            if a[channel].to_bits() != b[channel].to_bits() {
                differences.push(serde_json::json!({"sample":sample,"channel":channel,
                    "expected":a[channel],"actual":b[channel],
                    "expected_bits":a[channel].to_bits(),"actual_bits":b[channel].to_bits()}));
            }
        }
    }
    serde_json::json!({"length_equal":expected.len()==actual.len(),
        "different_channel_samples":differences.len(),"differences":differences})
}

fn error(stage: &str, value: impl std::fmt::Display) -> serde_json::Value {
    serde_json::json!({"stage":stage,"error":value.to_string()})
}
fn require(ok: bool, stage: &str, detail: impl std::fmt::Display) -> Trial<()> {
    if ok {
        Ok(())
    } else {
        Err(error(stage, detail))
    }
}
fn retain(directory: &std::path::Path, name: &str, value: &impl serde::Serialize) -> Trial<()> {
    std::fs::write(
        directory.join(name),
        serde_json::to_vec(value).map_err(|e| error("serialize", e))?,
    )
    .map_err(|e| error("retain evidence", e))
}
fn command(document: &ProjectDocument, name: &str, command: Command) -> Trial<ProjectDocument> {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(name).map_err(|e| error("revision", e))?,
        command,
    };
    let transaction=apply(document,&request).map_err(|e|serde_json::json!({
        "stage":name,"request":request,"error_code":format!("{:?}",e.code),"error_message":e.message,
    }))?;
    let after = transaction
        .forward
        .apply(document)
        .map_err(|e| error(name, e))?;
    require(
        transaction
            .inverse
            .apply(&after)
            .map_err(|e| error("inverse", e))?
            == *document,
        "inverse",
        "command failed exact inverse roundtrip",
    )?;
    ProjectDocument::from_json(&after.to_json().map_err(|e| error("JSON", e))?)
        .map_err(|e| error("JSON admission", e))
}
fn setup(directory: &std::path::Path) -> Trial<ProjectDocument> {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    wire["nodes"]["lead"] = serde_json::to_value(hold(4, HoldAudio::Silence)).unwrap();
    let initial =
        ProjectDocument::from_json(&wire.to_string()).map_err(|e| error("lead4 admission", e))?;
    require(
        initial.audio_bindings().is_empty(),
        "initial fixture",
        "unexpected preexisting journal",
    )?;
    let event = &initial.beat_sounds()[&id("owner")][&sound_id("effect")];
    require(
        event.source == audio(1024..7432)
            && event.offset == AudioSample(0)
            && matches!(event.mapping, SourceAudioMapping::SelectedPlacement { .. })
            && event.start_edge == AudioEdgePolicy::Hard
            && event.end_edge == AudioEdgePolicy::Hard,
        "initial fixture",
        "established actual-WAV SelectedPlacement/Hard event changed",
    )?;
    let top = command(
        &initial,
        "birth-top",
        Command::Group {
            parent: initial.root().clone(),
            start: 1,
            end: 3,
            id: id("top"),
            label: "Top".into(),
        },
    )?;
    let NodeKind::Hold { recipe } = hold(5, HoldAudio::Silence).kind else {
        unreachable!()
    };
    let moved = command(
        &top,
        "birth-prefix-five",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: id("birth-prefix"),
            identities: SplitIdentities::default(),
            timing: timing("birth-prefix-five"),
        },
    )?;
    retain(directory, "before-group.json", &moved)?;
    let journal = &moved.audio_bindings().sound_clocks()[&id("owner")][&sound_id("effect")];
    require(
        journal.scope() == &id("top")
            && journal.clocks().len() == 1
            && journal.clocks()[0].scope() == &id("top"),
        "prefix journal",
        "expected one broad Top reference",
    )?;
    let selection = SliceCaptureSelection::Child { node: id("owner") };
    let plan = moved
        .group_selection(&id("top"), &selection)
        .map_err(|e| error("group preflight", e))?;
    require(
        plan.required_split_ids == 0,
        "group preflight",
        "whole child unexpectedly requires splits",
    )?;
    let grouped = command(
        &moved,
        "birth-group",
        Command::GroupSelection {
            parent: id("top"),
            selection,
            label: "New group".into(),
            identities: GroupSelectionIdentities {
                group: id("birth-g"),
                split: SplitIdentities::default(),
            },
            timing: timing("birth-group"),
        },
    )?;
    retain(directory, "after-group-before-eq.json", &grouped)?;
    let journal = &grouped.audio_bindings().sound_clocks()[&id("owner")][&sound_id("effect")];
    require(
        journal.scope() == &id("top")
            && journal.clocks().len() == 1
            && journal.clocks()[0].scope() == &id("top")
            && journal.clocks()[0].introduced_groups() == &BTreeSet::from([id("birth-g")]),
        "group relation",
        "candidate must retain Top alias and record new G",
    )?;
    // Keep Top/owner/event neutral so one independently applied G EQ suffices.
    let birth = command(
        &grouped,
        "birth-g-eq",
        Command::SetAudioTreatments {
            node: id("birth-g"),
            treatments: eq([4500, -2500, 1500]),
        },
    )?;
    require(
        birth.duration().map_err(|e| error("duration", e))?.frames() == 15,
        "birth duration",
        "expected15f",
    )?;
    retain(directory, "birth.json", &birth)?;
    Ok(birth)
}

struct Input {
    samples: Vec<[f64; 2]>,
    suppressed: Vec<bool>,
    support: [i64; 2],
    blocks: Vec<serde_json::Value>,
}
fn input(
    document: &ProjectDocument,
    node: &NodeId,
    first: i64,
    end: i64,
    partitions: &[u32],
) -> Trial<Input> {
    let plan = std::sync::Arc::new(
        deadpan_plan::RenderPlan::compile(document).map_err(|e| error("plan", e))?,
    );
    let context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: node.clone(),
                repeats: vec![],
            },
            Default::default(),
        )
        .map_err(|e| error("input context", e))?;
    let support = [context.samples().start.0, context.samples().end.0];
    let mut renderer = StageAudio::new(plan.clone());
    let mut provider = ClockProvider::new(document);
    let mut result = Input {
        samples: Vec::new(),
        suppressed: Vec::new(),
        support,
        blocks: Vec::new(),
    };
    let mut at = first;
    let mut partition = 0;
    while at < end {
        let count = u32::try_from((end - at).min(i64::from(partitions[partition]))).unwrap();
        let block = renderer
            .read_beat_sound_owner_input(
                &mut provider,
                &context,
                AudioSample(at),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .map_err(|e| error(&format!("input {node} at {at}"), e))?;
        require(
            block.samples.len() == count as usize,
            "input length",
            "incomplete read",
        )?;
        require(
            block.samples.iter().flatten().all(|v| v.is_finite()),
            "input",
            "non-finite PCM",
        )?;
        result
            .suppressed
            .extend((at..at + i64::from(count)).map(|sample| {
                block
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&AudioSample(sample)))
            }));
        result
            .blocks
            .push(serde_json::json!({"start":at,"count":count,"suppressed":block.suppressed}));
        result.samples.extend(block.samples);
        at += i64::from(count);
        partition = (partition + 1) % partitions.len();
    }
    Ok(result)
}
fn input_evidence(input: &Input) -> serde_json::Value {
    serde_json::json!({
        "support":input.support,"sample_bits":input.samples.iter().map(|s|s.map(f64::to_bits)).collect::<Vec<_>>(),
        "suppressed_per_sample":input.suppressed,"blocks":input.blocks,
    })
}
fn differences64(first: i64, expected: &[[f64; 2]], actual: &[[f64; 2]]) -> serde_json::Value {
    let mut differences = Vec::new();
    for (offset, (a, b)) in expected.iter().zip(actual).enumerate() {
        for channel in 0..2 {
            if a[channel].to_bits() != b[channel].to_bits() {
                differences.push(
                    serde_json::json!({"sample":first+offset as i64,"channel":channel,
                    "expected_bits":a[channel].to_bits(),"actual_bits":b[channel].to_bits(),
                    "expected":a[channel],"actual":b[channel]}),
                );
            }
        }
    }
    serde_json::json!({"length_equal":expected.len()==actual.len(),
        "different_channel_samples":differences.len(),"differences":differences})
}
fn read_bus(
    document: &ProjectDocument,
    first: i64,
    end: i64,
    partitions: &[u32],
) -> Trial<Vec<[f32; 2]>> {
    let plan = std::sync::Arc::new(
        deadpan_plan::RenderPlan::compile(document).map_err(|e| error("bus plan", e))?,
    );
    let mut renderer = StageAudio::new(plan);
    let mut provider = ClockProvider::new(document);
    let mut samples = Vec::new();
    let mut at = first;
    let mut partition = 0;
    while at < end {
        let count = u32::try_from((end - at).min(i64::from(partitions[partition]))).unwrap();
        let block = renderer
            .prepare_authored_bus(
                &mut provider,
                AudioSample(at),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .map_err(|e| error(&format!("bus at {at}"), e))?;
        require(
            block.samples.len() == count as usize,
            "bus length",
            "incomplete read",
        )?;
        require(
            block.samples.iter().flatten().all(|v| v.is_finite()),
            "bus",
            "non-finite PCM",
        )?;
        samples.extend(block.samples);
        at += i64::from(count);
        partition = (partition + 1) % partitions.len();
    }
    Ok(samples)
}

fn run(directory: &std::path::Path) -> Trial<serde_json::Value> {
    require(
        frame_sample_boundary(9) == BIRTH_FIRST
            && frame_sample_boundary(13) == BIRTH_END
            && frame_sample_boundary(15) == DEST_FIRST
            && frame_sample_boundary(19) == DEST_END
            && BIRTH_END - BIRTH_FIRST == 6407
            && DEST_END - DEST_FIRST == 6406
            && BIRTH_END - 1 + SHIFT == DEST_END,
        "literal arithmetic",
        "unexpected NTSC boundaries or terminal transport",
    )?;
    let birth = setup(directory)?;
    // This samples G's delivered input only. Finite old history beyond these
    // 6407 labels is deliberately excluded from the independent oracle.
    let born = input(&birth, &id("birth-g"), BIRTH_FIRST, BIRTH_END, &[256])?;
    retain(
        directory,
        "birth-delivered-input.json",
        &input_evidence(&born),
    )?;
    require(
        born.support == [BIRTH_FIRST, BIRTH_END],
        "birth support",
        "G nominal support differs",
    )?;
    let terminal_at_birth = born.samples.last().unwrap().iter().any(|v| *v != 0.0);
    let slice=CapturedEditSlice::capture_selection(&birth,&id("top"),
        &SliceCaptureSelection::Child { node:id("birth-g") },timing("birth-capture"))
        .map_err(|e|serde_json::json!({"stage":"capture whole Child(G)","code":format!("{:?}",e.code),"message":e.message}))?;
    slice
        .validate_capture(&birth)
        .map_err(|e| error("validate capture", e))?;
    let slice_json = slice.to_json().map_err(|e| error("slice JSON", e))?;
    retain(
        directory,
        "captured-slice.json",
        &serde_json::from_str::<serde_json::Value>(&slice_json).unwrap(),
    )?;
    let slice =
        CapturedEditSlice::from_json(&slice_json).map_err(|e| error("slice admission", e))?;
    let slice_wire: serde_json::Value = serde_json::from_str(&slice_json).unwrap();
    let captured: AudioBindingState = serde_json::from_value(slice_wire["audio_bindings"].clone())
        .map_err(|e| error("capture bindings", e))?;
    let captured_journal = &captured.sound_clocks()[&id("owner")][&sound_id("effect")];
    require(
        captured_journal.scope() == &id("birth-g") && captured_journal.clocks().len() == 2,
        "captured journal",
        "expected live G scope and two chronological references",
    )?;
    let old = &captured_journal.clocks()[0];
    let new = &captured_journal.clocks()[1];
    require(
        old.timing() == &timing("birth-prefix-five")
            && old.scope() == &id("owner")
            && old.owner() == &id("owner")
            && old.introduced_groups() == &BTreeSet::from([id("birth-g")])
            && new.timing() == &timing("birth-capture")
            && new.scope() == &id("birth-g")
            && new.owner() == &id("owner")
            && new.introduced_groups().is_empty(),
        "captured mixed journal",
        "wrong historical narrowing, introduced G or reference order",
    )?;
    let identities = slice_identities(&slice, "new-birth-copy");
    retain(directory, "paste-identities.json", &identities)?;
    let pasted = command(
        &birth,
        "birth-paste",
        Command::SpliceSlice {
            parent: birth.root().clone(),
            index: birth.children(birth.root()).count(),
            slice: slice.clone(),
            identities: identities.clone(),
            timing: timing("birth-paste"),
        },
    )?;
    retain(directory, "pasted.json", &pasted)?;
    require(
        pasted
            .duration()
            .map_err(|e| error("duration", e))?
            .frames()
            == 19,
        "paste duration",
        "expected19f",
    )?;
    let wrapper = pasted
        .children(pasted.root())
        .last()
        .ok_or_else(|| error("copied wrapper", "missing root child"))?
        .clone();
    let NodeKind::Sequence { children: wrapped } = &pasted.nodes()[&wrapper].kind else {
        return Err(error(
            "copied wrapper",
            "whole-child paste did not create its neutral Sequence",
        ));
    };
    require(
        wrapped.len() == 1
            && wrapper == identities.authored.nodes[0]
            && pasted.nodes()[&wrapper].audio_treatments == AudioTreatments::default(),
        "copied wrapper",
        "expected neutral wrapper containing whole copied G",
    )?;
    let copied_g = wrapped[0].clone();
    let NodeKind::Sequence { children } = &pasted.nodes()[&copied_g].kind else {
        return Err(error(
            "copied G",
            "whole-child paste did not retain ordinary G",
        ));
    };
    require(
        children.len() == 1,
        "copied G",
        "expected one copied unchanged owner",
    )?;
    let copied_owner = children[0].clone();
    require(
        copied_g == identities.authored.nodes[1]
            && copied_owner == identities.authored.nodes[2]
            && pasted.beat_sounds()[&copied_owner] == birth.beat_sounds()[&id("owner")]
            && pasted.nodes()[&copied_g].audio_treatments
                == birth.nodes()[&id("birth-g")].audio_treatments,
        "copied authored identity",
        "wrong live G/owner IDs, recipe or G EQ",
    )?;
    let journal = &pasted.audio_bindings().sound_clocks()[&copied_owner][&sound_id("effect")];
    require(
        journal.scope() == &copied_g && journal.clocks().len() == 2,
        "pasted journal",
        "wrong live scope or reference count",
    )?;
    let expected_timings=captured.timings().keys().enumerate().map(|(index,old)|
        // Seam paste reserves ordinal0 for its suffix even at the root end.
        (old.clone(),AudioTimingId { allocation:RevisionId::new("birth-paste").unwrap(),ordinal:1+index as u32 }))
        .collect::<BTreeMap<_,_>>();
    for (reference, source) in journal.clocks().iter().zip(captured_journal.clocks()) {
        require(
            reference.timing() == &expected_timings[source.timing()]
                && reference.repeats().is_empty()
                && identities.aliases.contains(reference.owner())
                && identities.aliases.contains(reference.scope())
                && reference.owner() != &copied_owner,
            "renamed historical identity",
            "wrong exact timing rename or historical/live alias separation",
        )?;
    }
    require(
        journal.clocks()[0].scope() == journal.clocks()[0].owner()
            && journal.clocks()[0].introduced_groups() == &BTreeSet::from([copied_g.clone()])
            && journal.clocks()[1].scope() != journal.clocks()[1].owner()
            && journal.clocks()[1].introduced_groups().is_empty(),
        "renamed mixed journal",
        "old owner alias / new G alias / live G relation differs",
    )?;
    let plan = std::sync::Arc::new(
        deadpan_plan::RenderPlan::compile(&pasted).map_err(|e| error("scope plan", e))?,
    );
    let g_context = plan
        .audio_sound_instance_context(
            &InstancePath {
                node: copied_g.clone(),
                repeats: vec![],
            },
            Default::default(),
        )
        .map_err(|e| error("G context", e))?;
    let scopes = plan
        .beat_sound_clock_scopes(
            &copied_owner,
            &sound_id("effect"),
            deadpan_core::MAX_DOCUMENT_NODES,
        )
        .map_err(|e| error("public clock scopes", e))?;
    require(
        scopes.len() == 2,
        "public scopes",
        "expected mixed two-reference history",
    )?;
    let mut alias_evidence = Vec::new();
    for (index, scope) in scopes.iter().enumerate() {
        let (g, work) = scope
            .try_remap_context_with_work(&g_context, deadpan_core::MAX_DOCUMENT_NODES)
            .map_err(|e| error("public G context alias", e))?;
        let owner = scope
            .remap_instance(&InstancePath {
                node: copied_owner.clone(),
                repeats: vec![],
            })
            .map_err(|e| error("public owner alias", e))?;
        require(
            owner.node == *journal.clocks()[index].owner(),
            "owner alias",
            "historical owner rename mismatch",
        )?;
        require(
            if index == 0 {
                g.is_none()
            } else {
                g.as_ref().is_some_and(|alias| {
                    alias.node == *journal.clocks()[index].scope() && alias.repeats.is_empty()
                })
            },
            "mixed None/Some alias",
            "G was either invented before birth or lost in birth capture",
        )?;
        alias_evidence.push(serde_json::json!({"index":index,"timing":scope.timing(),
            "historical_scope":scope.historical_scope(),"historical_owner":scope.historical_owner(),
            "g_alias":g,"owner_alias":owner,"work":work}));
    }
    retain(directory, "public-aliases.json", &alias_evidence)?;

    let first = DEST_FIRST - 2048;
    let end = DEST_END + 2048;
    let expected_raw = (first..end)
        .map(|at| {
            let born_index = at - SHIFT - BIRTH_FIRST;
            if (0..born.samples.len() as i64).contains(&born_index) {
                born.samples[born_index as usize]
            } else {
                [0.0; 2]
            }
        })
        .collect::<Vec<_>>();
    let expected_suppressed = (first..end)
        .map(|at| {
            let born_index = at - SHIFT - BIRTH_FIRST;
            if (0..born.suppressed.len() as i64).contains(&born_index) {
                born.suppressed[born_index as usize]
            } else {
                true
            }
        })
        .collect::<Vec<_>>();
    let copied_input = input(&pasted, &copied_g, first, end, &[37, 173, 256])?;
    let input_diff = differences64(first, &expected_raw, &copied_input.samples);
    let suppression_diffs = expected_suppressed
        .iter()
        .zip(&copied_input.suppressed)
        .enumerate()
        .filter(|(_, (expected, actual))| expected != actual)
        .map(|(index, (expected, actual))| {
            serde_json::json!({
                "sample":first+index as i64,"expected":expected,"actual":actual })
        })
        .collect::<Vec<_>>();
    retain(
        directory,
        "copied-input.json",
        &input_evidence(&copied_input),
    )?;
    retain(
        directory,
        "expected-input.json",
        &serde_json::json!({
            "first":first,"sample_bits":expected_raw.iter().map(|s|s.map(f64::to_bits)).collect::<Vec<_>>(),
            "suppressed_per_sample":expected_suppressed,
        }),
    )?;
    let terminal_index = usize::try_from(DEST_END - first).unwrap();
    let copied_terminal_nonzero = copied_input.samples[terminal_index]
        .iter()
        .any(|v| *v != 0.0);
    let recipe = pasted.nodes()[&copied_g]
        .audio_treatments
        .equalizer()
        .ok_or_else(|| error("oracle EQ", "new G EQ missing"))?;
    let mut filter = deadpan_audio::Equalizer::new(recipe, &AtomicBool::new(false))
        .map_err(|e| error("oracle EQ compile", e))?;
    let mut oracle = Vec::new();
    for tile in (DEST_FIRST.div_euclid(1024) * 1024..DEST_END).step_by(1024) {
        let mut left = vec![0.0; 5120];
        let mut right = vec![0.0; 5120];
        for index in 0..5120 {
            let at = tile - 2048 + index as i64;
            let born_index = at - SHIFT - BIRTH_FIRST;
            if (0..born.samples.len() as i64).contains(&born_index) {
                left[index] = born.samples[born_index as usize][0];
                right[index] = born.samples[born_index as usize][1];
            }
        }
        let mut a = vec![0.0; 1024];
        let mut b = vec![0.0; 1024];
        filter
            .process_tile(&left, &right, &mut a, &mut b, &AtomicBool::new(false))
            .map_err(|e| error("oracle process_tile", e))?;
        for at in tile.max(DEST_FIRST)..(tile + 1024).min(DEST_END) {
            let index = (at - tile) as usize;
            oracle.push([a[index] as f32, b[index] as f32]);
        }
    }
    let mut expected = read_bus(&birth, 0, DEST_FIRST, &[1021])?;
    expected.extend(oracle.iter().copied());
    let actual = read_bus(&pasted, 0, DEST_END, &[37, 509, 1021, 173])?;
    let partitions = differences32(&actual, &read_bus(&pasted, 0, DEST_END, &[1021])?);
    let full_diff = differences32(&expected, &actual);
    let mut cold = Vec::new();
    for at in [DEST_FIRST - 1, DEST_FIRST, DEST_END - 3] {
        let fresh = read_bus(&pasted, at, (at + 3).min(DEST_END), &[3])?;
        let begin = at as usize;
        cold.push(serde_json::json!({"first":at,"comparison":differences32(
            &actual[begin..begin+fresh.len()],&fresh)}));
    }
    let no_eq = command(
        &pasted,
        "birth-ablate-eq",
        Command::SetAudioTreatments {
            node: copied_g.clone(),
            treatments: AudioTreatments::default(),
        },
    )?;
    let no_event = command(
        &pasted,
        "birth-ablate-event",
        Command::DeleteBeatSound {
            owner: copied_owner.clone(),
            id: sound_id("effect"),
        },
    )?;
    let no_eq_pcm = read_bus(&no_eq, 0, DEST_END, &[1021])?;
    let no_event_pcm = read_bus(&no_event, 0, DEST_END, &[1021])?;
    let active = |a: &[[f32; 2]], b: &[[f32; 2]]| {
        a.iter()
            .zip(b)
            .filter(|(a, b)| a[0] != b[0] || a[1] != b[1])
            .count()
    };
    let eq_active = active(
        &actual[DEST_FIRST as usize..],
        &no_eq_pcm[DEST_FIRST as usize..],
    );
    let event_active = active(
        &actual[DEST_FIRST as usize..],
        &no_event_pcm[DEST_FIRST as usize..],
    );
    let mut unavailable = ClockProvider::new(&pasted);
    unavailable.fixture.unavailable = true;
    let source_admission = StageAudio::new(plan.clone()).prepare_authored_bus(
        &mut unavailable,
        AudioSample(DEST_FIRST),
        32,
        TIMEOUT,
        &AtomicBool::new(false),
    );
    let source_rejected = source_admission.is_err();
    let controls_valid = terminal_at_birth
        && eq_active > 0
        && event_active > 0
        && source_rejected
        && born.samples.len() == 6407
        && expected.len() == DEST_END as usize
        && copied_input.support == [DEST_FIRST, DEST_END]
        && partitions["different_channel_samples"] == 0
        && cold
            .iter()
            .all(|v| v["comparison"]["different_channel_samples"] == 0);
    let raw_equal = input_diff["length_equal"] == true
        && input_diff["different_channel_samples"] == 0
        && suppression_diffs.is_empty();
    let pcm_equal = full_diff["different_channel_samples"] == 0;
    Ok(serde_json::json!({
        "classification":if !controls_valid {"control witness/setup not established"} else if raw_equal && pcm_equal {
            "mixed history preserves this group-birth terminal"
        } else {"mixed history differs from established group-birth oracle"},
        "controls_valid":controls_valid,"raw_equal":raw_equal,"pcm_equal":pcm_equal,
        "birth_terminal_nonzero":terminal_at_birth,"copied_terminal_nonzero":copied_terminal_nonzero,
        "birth_terminal_bits":born.samples.last().unwrap().map(f64::to_bits),
        "copied_terminal_bits":copied_input.samples[terminal_index].map(f64::to_bits),
        "literal_oracle":{"birth":[BIRTH_FIRST,BIRTH_END],"destination":[DEST_FIRST,DEST_END],
            "shift":SHIFT,"delivered_birth_count":6407,"copied_nominal_count":6406,"transported_terminal":DEST_END},
        "aliases":alias_evidence,"raw_input":input_diff,"suppression_differences":suppression_diffs,
        "full_pcm":full_diff,"partitions":partitions,"cold":cold,
        "eq_active_samples":eq_active,"event_active_samples":event_active,
        "eq_ablation":differences32(&actual,&no_eq_pcm),"event_ablation":differences32(&actual,&no_event_pcm),
        "source_rejected":source_rejected,"source_error":source_admission.err().map(|e|e.to_string()),
        "expected_pcm_bits":expected.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "actual_pcm_bits":actual.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "no_eq_pcm_bits":no_eq_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "no_event_pcm_bits":no_event_pcm.iter().map(|s|s.map(f32::to_bits)).collect::<Vec<_>>(),
        "implementation_claim":false,"independent_dsp_claim":false,"listening_claim":false,
    }))
}

#[test]
fn candidate_new_group_birth_copy_keeps_mixed_history_terminal() {
    let directory = std::env::var_os("DEADPAN_SCOPE_PROBE_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("deadpan-new-group-birth-{}", std::process::id()))
        })
        .join("new-group-birth");
    std::fs::create_dir_all(&directory).unwrap();
    let result = match run(&directory) {
        Ok(value) => value,
        Err(failure) => serde_json::json!({
            "classification":"guard, admission, setup, or API failure; no DSP conclusion",
            "failure":failure,"controls_valid":false,"raw_equal":false,"pcm_equal":false,
            "implementation_claim":false,
        }),
    };
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
    let summary = serde_json::json!({
        "classification":result["classification"],"controls_valid":result["controls_valid"],
        "raw_equal":result["raw_equal"],"pcm_equal":result["pcm_equal"],
        "birth_terminal_nonzero":result["birth_terminal_nonzero"],
        "copied_terminal_nonzero":result["copied_terminal_nonzero"],
        "raw_different_channel_samples":result["raw_input"]["different_channel_samples"],
        "pcm_different_channel_samples":result["full_pcm"]["different_channel_samples"],
        "failure":result["failure"],"implementation_claim":false,
    });
    std::fs::write(
        directory.join("summary.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    eprintln!("evidence: {}\n{summary}", directory.display());
    // Never classify a rejected edit, inactive witness or missing source check
    // as a DSP defect. Persist complete bits/differences before any assertion.
    assert!(
        result["controls_valid"] == true,
        "witness/setup not established; inspect retained evidence"
    );
    assert!(
        result["raw_equal"] == true,
        "raw group-birth input differs; inspect retained evidence"
    );
    assert!(
        result["pcm_equal"] == true,
        "group-birth EQ oracle differs; inspect retained evidence"
    );
}
